/*
*   Copyright (c) 2026. caoccao.com Sam Cao
*   All rights reserved.

*   Licensed under the Apache License, Version 2.0 (the "License");
*   you may not use this file except in compliance with the License.
*   You may obtain a copy of the License at

*   http://www.apache.org/licenses/LICENSE-2.0

*   Unless required by applicable law or agreed to in writing, software
*   distributed under the License is distributed on an "AS IS" BASIS,
*   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
*   See the License for the specific language governing permissions and
*   limitations under the License.
*/

//! MKVToolNix: finding it, listing a file's tracks with `mkvmerge -J`, and
//! starting `mkvextract`.

#[cfg(target_os = "macos")]
use std::cmp::Ordering;
#[cfg(target_os = "macos")]
use std::fs;
use std::io::{BufReader, PipeReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use crate::extract::ExtractError;
use crate::protocol::{MkvToolNixStatus, MkvTrack, TrackListError};

/// Lists a file's tracks.
const MKVMERGE: &str = "mkvmerge";
/// Extracts them.
const MKVEXTRACT: &str = "mkvextract";
/// MKVToolNix is found where both tools the app runs are.
const MKVTOOLNIX_TOOLS: [&str; 2] = [MKVMERGE, MKVEXTRACT];

/// How long `mkvmerge -J` may run for one file. Reading a file's headers
/// takes well under a second; a listing still running after this (a stalled
/// network or removable drive) is stopped and fails as
/// [`TrackListError::TimedOut`].
pub const LIST_TRACKS_TIMEOUT: Duration = Duration::from_secs(60);
/// How often a listing checks whether `mkvmerge` has exited, was cancelled
/// or ran out of time.
const LIST_POLL: Duration = Duration::from_millis(20);
/// How long a stopped `mkvmerge` is waited for before it is left to exit by
/// itself (a process blocked in the kernel ends only when its I/O does).
const KILL_WAIT: Duration = Duration::from_secs(1);

/// An MKVToolNix directory a track listing or an extraction found by itself
/// (on macOS, the newest `MKVToolNix-<version>.app` when the default path has
/// no MKVToolNix), with the configured path it looked from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutoDetected {
  /// The MKVToolNix path the listing or the extraction was given.
  pub configured: String,
  /// The directory it found instead.
  pub detected: String,
}

/// Receives each [`AutoDetected`] directory. Called from a worker thread; it
/// must not panic and should return quickly, e.g. by sending into a channel.
pub type AutoDetectedSink = Box<dyn Fn(AutoDetected) + Send + Sync>;

static AUTO_DETECTED_SINK: OnceLock<AutoDetectedSink> = OnceLock::new();

/// Install the sink for auto-detected MKVToolNix directories. The core never
/// writes the config itself: the application stores the path through its own
/// config store, so a later settings write keeps it. Only the first call
/// counts; without a sink the directory is found again every time.
pub fn init_auto_detected(sink: AutoDetectedSink) {
  if AUTO_DETECTED_SINK.set(sink).is_err() {
    log::warn!("mkvtoolnix::init_auto_detected called more than once; keeping the first sink");
  }
}

/// One line of `mkvextract` output.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MkvextractLine<'a> {
  /// A percentage, 0–100.
  Progress(u32),
  Warning(&'a str),
  Error(&'a str),
  /// Anything else: informational text, or what the process wrote to stderr.
  Other(&'a str),
}

/// Classifies a line of `mkvextract --gui-mode` output. In GUI mode progress,
/// warnings and errors are `#GUI#progress 42%`, `#GUI#warning …` and
/// `#GUI#error …` whatever the interface language, while the plain-text
/// `Progress:` / `Warning:` / `Error:` prefixes are translated; those are
/// still recognised in English. Other `#GUI#` markers are dropped.
pub(crate) fn parse_mkvextract_line(line: &str) -> Option<MkvextractLine<'_>> {
  let trimmed = line.trim();
  if trimmed.is_empty() {
    return None;
  }
  let progress = trimmed
    .strip_prefix("#GUI#progress")
    .or_else(|| trimmed.strip_prefix("Progress:"));
  if let Some(percent) = progress.and_then(|rest| rest.trim().trim_end_matches('%').trim().parse::<u32>().ok()) {
    return Some(MkvextractLine::Progress(percent.min(100)));
  }
  let message = |prefixes: [&str; 2]| {
    prefixes
      .iter()
      .find_map(|prefix| trimmed.strip_prefix(prefix))
      .map(str::trim)
  };
  if let Some(text) = message(["#GUI#warning", "Warning:"]) {
    Some(MkvextractLine::Warning(text))
  } else if let Some(text) = message(["#GUI#error", "Error:"]) {
    Some(MkvextractLine::Error(text))
  } else if trimmed.starts_with("#GUI#") {
    None
  } else {
    Some(MkvextractLine::Other(trimmed))
  }
}

/// Splits the output at carriage returns (progress lines) and newlines and
/// hands every non-empty line to `on_line`, until the output ends.
pub(crate) fn read_mkvextract_output<F>(reader: impl Read, mut on_line: F)
where
  F: FnMut(&str),
{
  let mut buf_reader = BufReader::new(reader);
  let mut current_line = Vec::new();
  let mut byte = [0u8; 1];
  loop {
    match buf_reader.read(&mut byte) {
      Ok(0) => break,
      Ok(_) => {
        if byte[0] == b'\r' || byte[0] == b'\n' {
          if !current_line.is_empty() {
            let line = String::from_utf8_lossy(&current_line);
            on_line(&line);
            current_line.clear();
          }
        } else {
          current_line.push(byte[0]);
        }
      }
      Err(_) => break,
    }
  }
  if !current_line.is_empty() {
    let line = String::from_utf8_lossy(&current_line);
    on_line(&line);
  }
}

struct MkvToolNixResolution {
  path: PathBuf,
  auto_detected: bool,
}

fn get_tool_path(path: &Path, tool: &str) -> PathBuf {
  #[cfg(target_os = "windows")]
  {
    let exe_path = path.join(format!("{}.exe", tool));
    if exe_path.exists() && exe_path.is_file() {
      return exe_path;
    }
  }
  path.join(tool)
}

/// The tool to run from the MKVToolNix directory `configured`, the path the
/// caller read from its settings; an auto-detected directory goes to the
/// sink of [`init_auto_detected`].
fn resolve_mkvtoolnix_tool_path(configured: &str, tool: &str) -> PathBuf {
  let resolution = resolve_mkvtoolnix(configured, tool);
  report_auto_detected(&resolution, configured, |detected| {
    if let Some(sink) = AUTO_DETECTED_SINK.get() {
      sink(AutoDetected {
        configured: configured.to_owned(),
        detected,
      });
    }
  });
  get_tool_path(&resolution.path, tool)
}

/// A command for the tool at `tool_path`, without a console window on
/// Windows.
fn create_tool_command(tool_path: &Path) -> std::process::Command {
  #[cfg_attr(not(target_os = "windows"), allow(unused_mut))]
  let mut cmd = std::process::Command::new(tool_path);
  #[cfg(target_os = "windows")]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
  }
  cmd
}

fn has_tool(path: &Path, tool: &str) -> bool {
  let tool_path = path.join(tool);
  if tool_path.exists() && tool_path.is_file() {
    return true;
  }
  #[cfg(target_os = "windows")]
  {
    let tool_exe_path = path.join(format!("{}.exe", tool));
    if tool_exe_path.exists() && tool_exe_path.is_file() {
      return true;
    }
  }
  false
}

/// Whether `path` holds both tools the app runs.
fn has_mkvtoolnix(path: &Path) -> bool {
  MKVTOOLNIX_TOOLS.iter().all(|tool| has_tool(path, tool))
}

#[cfg(target_os = "macos")]
fn compare_version_parts(left: &[u32], right: &[u32]) -> Ordering {
  let len = left.len().max(right.len());
  for i in 0..len {
    let l = left.get(i).copied().unwrap_or(0);
    let r = right.get(i).copied().unwrap_or(0);
    match l.cmp(&r) {
      Ordering::Equal => continue,
      non_eq => return non_eq,
    }
  }
  Ordering::Equal
}

#[cfg(target_os = "macos")]
fn parse_version_parts(version: &str) -> Vec<u32> {
  version
    .split('.')
    .filter_map(|part| {
      let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
      if digits.is_empty() {
        None
      } else {
        digits.parse::<u32>().ok()
      }
    })
    .collect()
}

#[cfg(target_os = "macos")]
fn is_default_macos_mkvtoolnix_path(path: &str) -> bool {
  path.trim().trim_end_matches('/') == "/Applications/MKVToolNix.app/Contents/MacOS"
}

#[cfg(target_os = "macos")]
fn find_latest_versioned_macos_mkvtoolnix_path(tool: &str) -> Option<PathBuf> {
  let entries = fs::read_dir("/Applications").ok()?;
  let mut latest: Option<(Vec<u32>, PathBuf)> = None;
  for entry in entries.flatten() {
    let file_name = entry.file_name();
    let app_name = match file_name.to_str() {
      Some(value) => value,
      None => continue,
    };
    if !app_name.starts_with("MKVToolNix-") || !app_name.ends_with(".app") {
      continue;
    }
    let version = &app_name["MKVToolNix-".len()..app_name.len() - ".app".len()];
    let version_parts = parse_version_parts(version);
    if version_parts.is_empty() {
      continue;
    }
    let mkvtoolnix_path = entry.path().join("Contents").join("MacOS");
    if !has_tool(&mkvtoolnix_path, tool) {
      continue;
    }
    match &latest {
      None => latest = Some((version_parts, mkvtoolnix_path)),
      Some((latest_version, _)) => {
        if compare_version_parts(&version_parts, latest_version) == Ordering::Greater {
          latest = Some((version_parts, mkvtoolnix_path));
        }
      }
    }
  }
  latest.map(|(_, path)| path)
}

fn resolve_mkvtoolnix(path: &str, tool: &str) -> MkvToolNixResolution {
  let trimmed_path = path.trim();
  let configured_path = PathBuf::from(trimmed_path);
  if has_tool(&configured_path, tool) {
    return MkvToolNixResolution {
      path: configured_path,
      auto_detected: false,
    };
  }
  #[cfg(target_os = "macos")]
  {
    if is_default_macos_mkvtoolnix_path(trimmed_path) {
      if let Some(latest_path) = find_latest_versioned_macos_mkvtoolnix_path(tool) {
        return MkvToolNixResolution {
          path: latest_path,
          auto_detected: true,
        };
      }
    }
  }
  MkvToolNixResolution {
    path: configured_path,
    auto_detected: false,
  }
}

/// The `mkvextract` an extraction with the MKVToolNix directory
/// `mkv_toolnix_path` runs, found the way [`spawn_mkvextract`] finds it,
/// without reporting an auto-detected directory. For the copyable command.
pub fn mkvextract_executable(mkv_toolnix_path: &str) -> PathBuf {
  get_tool_path(&resolve_mkvtoolnix(mkv_toolnix_path, MKVEXTRACT).path, MKVEXTRACT)
}

/// Hands an auto-detected directory that differs from the configured path
/// to `report`.
fn report_auto_detected(resolution: &MkvToolNixResolution, configured: &str, report: impl FnOnce(String)) {
  if !resolution.auto_detected {
    return;
  }
  let path = resolution.path.to_string_lossy().to_string();
  if path != configured {
    report(path);
  }
}

/// The source must be a file; the tools would otherwise report it in their
/// own words (`mkvextract` calls a missing source an unknown mode).
fn source_problem(file: &str) -> Option<String> {
  let path = Path::new(file);
  if !path.exists() {
    Some(format!("Path {} does not exist.", path.display()))
  } else if !path.is_file() {
    Some(format!("Path {} is not a file.", path.display()))
  } else {
    None
  }
}

/// The tracks of `file` from `mkvmerge -J`, as 0.3.0 listed them: its
/// tracks, then one `chapters` row when its first chapter edition has
/// entries, then one row per attachment. `mkv_toolnix_path` is the
/// MKVToolNix directory from the caller's settings. `mkvmerge` is stopped
/// when `cancelled` is set ([`TrackListError::Cancelled`]) or after
/// [`LIST_TRACKS_TIMEOUT`] ([`TrackListError::TimedOut`]). Blocking; the
/// caller moves it off its UI thread.
pub fn list_tracks(
  file: &str,
  mkv_toolnix_path: &str,
  cancelled: &AtomicBool,
) -> Result<Vec<MkvTrack>, TrackListError> {
  if let Some(problem) = source_problem(file) {
    return Err(TrackListError::Other(problem));
  }
  let mkvmerge = resolve_mkvtoolnix_tool_path(mkv_toolnix_path, MKVMERGE);
  identify(&mkvmerge, file, cancelled, LIST_TRACKS_TIMEOUT)
}

/// Runs `mkvmerge -J file` with the `mkvmerge` at `mkvmerge_path`, until it
/// exits, `cancelled` is set or `timeout` passed.
fn identify(
  mkvmerge_path: &Path,
  file: &str,
  cancelled: &AtomicBool,
  timeout: Duration,
) -> Result<Vec<MkvTrack>, TrackListError> {
  let mut cmd = create_tool_command(mkvmerge_path);
  cmd
    .arg("-J")
    .arg(file)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child = cmd.spawn().map_err(|error| TrackListError::NotAvailable {
    path: mkvmerge_path.display().to_string(),
    detail: error.to_string(),
  })?;
  // Both pipes are read at once, so neither fills up while the other waits.
  let read_all = |pipe: Option<Box<dyn Read + Send>>| {
    std::thread::spawn(move || {
      let mut bytes = Vec::new();
      if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut bytes);
      }
      bytes
    })
  };
  let stdout = read_all(child.stdout.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
  let stderr = read_all(child.stderr.take().map(|pipe| Box::new(pipe) as Box<dyn Read + Send>));
  let deadline = Instant::now() + timeout;
  let status = loop {
    match child.try_wait() {
      Ok(Some(status)) => break status,
      Ok(None) => {}
      Err(error) => return Err(TrackListError::Other(error.to_string())),
    }
    if cancelled.load(AtomicOrdering::Relaxed) {
      stop(&mut child);
      return Err(TrackListError::Cancelled);
    }
    if Instant::now() >= deadline {
      stop(&mut child);
      return Err(TrackListError::TimedOut {
        seconds: timeout.as_secs(),
      });
    }
    std::thread::sleep(LIST_POLL);
  };
  let stdout = stdout.join().unwrap_or_default();
  let stderr = stderr.join().unwrap_or_default();
  identify_output(status.code(), &stdout, &stderr)
}

/// Kills a listing's `mkvmerge` and waits for it a moment; one that does not
/// end (blocked in the kernel) is left to exit by itself, with its pipe
/// readers.
fn stop(child: &mut Child) {
  let _ = child.kill();
  let deadline = Instant::now() + KILL_WAIT;
  while Instant::now() < deadline {
    if !matches!(child.try_wait(), Ok(None)) {
      return;
    }
    std::thread::sleep(LIST_POLL);
  }
}

/// The tracks from `mkvmerge -J`'s exit code and output. It exits with 0,
/// with 1 after warnings, and with 2 on an error, which it reports in the
/// JSON's `errors` (stderr stays empty then).
fn identify_output(code: Option<i32>, stdout: &[u8], stderr: &[u8]) -> Result<Vec<MkvTrack>, TrackListError> {
  if matches!(code, Some(0 | 1)) {
    return parse_identify_json(stdout);
  }
  let reported: Vec<String> = serde_json::from_slice::<serde_json::Value>(stdout)
    .ok()
    .and_then(|json| {
      json["errors"].as_array().map(|errors| {
        errors
          .iter()
          .filter_map(|error| error.as_str())
          .map(|error| error.trim().to_owned())
          .filter(|error| !error.is_empty())
          .collect()
      })
    })
    .unwrap_or_default();
  let messages = if reported.is_empty() {
    String::from_utf8_lossy(stderr)
      .lines()
      .map(str::trim)
      .filter(|line| !line.is_empty())
      .map(str::to_owned)
      .collect()
  } else {
    reported
  };
  Err(TrackListError::Failed { messages })
}

/// The table rows of `mkvmerge -J`'s JSON output, as 0.3.0 built them.
pub fn parse_identify_json(bytes: &[u8]) -> Result<Vec<MkvTrack>, TrackListError> {
  let json: serde_json::Value = serde_json::from_slice(bytes).map_err(|error| TrackListError::Parse {
    detail: error.to_string(),
  })?;
  let mut tracks: Vec<MkvTrack> = json["tracks"]
    .as_array()
    .map(|arr| {
      arr
        .iter()
        .map(|t| {
          let props = &t["properties"];
          MkvTrack {
            id: t["id"].as_i64().unwrap_or(0),
            number: props["number"].as_i64().unwrap_or(0),
            track_type: t["type"].as_str().unwrap_or("").to_owned(),
            codec: t["codec"].as_str().unwrap_or("").to_owned(),
            codec_id: props["codec_id"].as_str().unwrap_or("").to_owned(),
            track_name: props["track_name"].as_str().unwrap_or("").to_owned(),
            language: props["language"].as_str().unwrap_or("und").to_owned(),
          }
        })
        .collect()
    })
    .unwrap_or_default();

  let chapter_entries: i64 = json["chapters"]
    .as_array()
    .and_then(|arr| arr.first())
    .and_then(|v| v["num_entries"].as_i64())
    .unwrap_or(0);
  if chapter_entries > 0 {
    tracks.push(MkvTrack {
      id: 0,
      number: 0,
      track_type: MkvTrack::TYPE_CHAPTERS.to_owned(),
      codec: "xml".to_owned(),
      codec_id: "xml".to_owned(),
      track_name: String::new(),
      language: String::new(),
    });
  }

  if let Some(atts) = json["attachments"].as_array() {
    for att in atts {
      let id = att["id"].as_i64().unwrap_or(0);
      let file_name = att["file_name"].as_str().unwrap_or("").to_owned();
      let content_type = att["content_type"].as_str().unwrap_or("").to_owned();
      let subtype = derive_attachment_subtype(&file_name, &content_type);
      tracks.push(MkvTrack {
        id,
        number: 0,
        track_type: MkvTrack::TYPE_ATTACHMENT.to_owned(),
        codec: subtype.clone(),
        codec_id: subtype,
        track_name: file_name,
        language: String::new(),
      });
    }
  }

  Ok(tracks)
}

/// An attachment's codec column: its file extension, else the subtype of its
/// MIME type, lowercased.
fn derive_attachment_subtype(file_name: &str, content_type: &str) -> String {
  if let Some(ext_pos) = file_name.rfind('.') {
    let ext = &file_name[ext_pos + 1..];
    if !ext.is_empty() {
      return ext.to_ascii_lowercase();
    }
  }
  if let Some(slash) = content_type.find('/') {
    let subtype = &content_type[slash + 1..];
    if !subtype.is_empty() {
      return subtype.to_ascii_lowercase();
    }
  }
  String::new()
}

/// Starts `mkvextract` for the source `file` with the extraction arguments
/// `args` (`tracks <id>:<path>… chapters <path> attachments <id>:<path>…`),
/// from the MKVToolNix directory `mkv_toolnix_path` the extraction was
/// queued with. Returns the process and one pipe that carries both its stdout and its
/// stderr, so neither can fill up unread and whatever it prints can be
/// shown. `--gui-mode` keeps its progress, warning and error lines
/// recognisable in every interface language ([`parse_mkvextract_line`]);
/// the copyable command does not need it.
pub fn spawn_mkvextract(
  file: &str,
  mkv_toolnix_path: &str,
  args: &[String],
) -> Result<(Child, PipeReader), ExtractError> {
  if !Path::new(file).is_file() {
    return Err(ExtractError::SourceMissing { path: file.to_owned() });
  }
  spawn_extract_tool(&resolve_mkvtoolnix_tool_path(mkv_toolnix_path, MKVEXTRACT), file, args)
}

/// Starts the `mkvextract` at `mkvextract_path` as `mkvextract --gui-mode
/// <file> <args>…`.
fn spawn_extract_tool(
  mkvextract_path: &Path,
  file: &str,
  args: &[String],
) -> Result<(Child, PipeReader), ExtractError> {
  let mut cmd = create_tool_command(mkvextract_path);
  let pipe_error = |error: std::io::Error| ExtractError::Other(format!("Couldn't create the output pipe: {error}"));
  let (output, stdout) = std::io::pipe().map_err(pipe_error)?;
  let stderr = stdout.try_clone().map_err(pipe_error)?;
  cmd
    .arg("--gui-mode")
    .arg(file)
    .args(args)
    .stdin(Stdio::null())
    .stdout(stdout)
    .stderr(stderr);
  let child = cmd.spawn().map_err(|error| ExtractError::NotAvailable {
    path: mkvextract_path.display().to_string(),
    detail: error.to_string(),
  })?;
  // The command holds the parent's copies of the pipe's write end; reading
  // reaches EOF only once they are closed and the process has exited.
  drop(cmd);
  Ok((child, output))
}

fn mkvtoolnix_gui_process_name() -> &'static str {
  if cfg!(target_os = "windows") {
    "mkvtoolnix-gui.exe"
  } else {
    "mkvtoolnix-gui"
  }
}

/// Finds MKVToolNix without touching the config: with `check_running`, the
/// directory of a running `mkvtoolnix-gui` first, then `path` (on macOS the
/// default path also finds the newest `MKVToolNix-<version>.app`). Found
/// means both `mkvmerge` and `mkvextract` are there. The application stores
/// the returned path through its own config store.
pub fn detect_mkvtoolnix(path: &str, check_running: bool) -> MkvToolNixStatus {
  if check_running
    && let Some(dir) = crate::services::find_running_process_dir(mkvtoolnix_gui_process_name())
    && has_mkvtoolnix(&dir)
  {
    return MkvToolNixStatus {
      found: true,
      mkv_toolnix_path: dir.to_string_lossy().to_string(),
    };
  }
  let trimmed_path = path.trim();
  if trimmed_path.is_empty() {
    return MkvToolNixStatus {
      found: false,
      mkv_toolnix_path: String::new(),
    };
  }
  let resolution = resolve_mkvtoolnix(trimmed_path, MKVMERGE);
  MkvToolNixStatus {
    found: has_mkvtoolnix(&resolution.path),
    mkv_toolnix_path: resolution.path.to_string_lossy().to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Long enough for any listing of a test that should finish.
  const TIMEOUT: Duration = Duration::from_secs(10);

  /// A fresh directory under the system temp dir, removed when dropped.
  struct Dir(PathBuf);

  impl Dir {
    fn new(name: &str) -> Self {
      let path = std::env::temp_dir().join(format!("bme_mkvtoolnix_{name}_{}", std::process::id()));
      let _ = std::fs::remove_dir_all(&path);
      std::fs::create_dir_all(&path).unwrap();
      Self(path)
    }
  }

  impl Drop for Dir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  fn tool_file(tool: &str) -> String {
    if cfg!(target_os = "windows") {
      format!("{tool}.exe")
    } else {
      tool.to_owned()
    }
  }

  /// A shell script at `dir/name` running `body`, made executable.
  #[cfg(unix)]
  fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
  }

  fn reported(auto_detected: bool, path: &str, configured: &str) -> Option<String> {
    let resolution = MkvToolNixResolution {
      path: PathBuf::from(path),
      auto_detected,
    };
    let mut reported = None;
    report_auto_detected(&resolution, configured, |path| reported = Some(path));
    reported
  }

  #[test]
  fn only_an_auto_detected_directory_other_than_the_configured_one_is_reported() {
    let detected = "/Applications/MKVToolNix-98.0.app/Contents/MacOS";
    let default = "/Applications/MKVToolNix.app/Contents/MacOS";
    assert_eq!(reported(true, detected, default).as_deref(), Some(detected));
    assert_eq!(reported(true, detected, detected), None, "already configured");
    assert_eq!(reported(false, default, default), None, "the configured path itself");
  }

  #[test]
  fn detection_finds_both_tools_in_the_given_directory() {
    let dir = Dir::new("found");
    std::fs::write(dir.0.join(tool_file(MKVMERGE)), b"").unwrap();
    let path = dir.0.to_string_lossy().to_string();
    let status = detect_mkvtoolnix(&format!("  {path} "), false);
    assert!(!status.found, "mkvextract is missing");
    assert_eq!(status.mkv_toolnix_path, path);
    std::fs::write(dir.0.join(tool_file(MKVEXTRACT)), b"").unwrap();
    let status = detect_mkvtoolnix(&format!("  {path} "), false);
    assert!(status.found);
    assert_eq!(status.mkv_toolnix_path, path);
  }

  /// A running `mkvtoolnix-gui` (there may be one on a developer machine)
  /// is found with both tools; without one, an empty path finds nothing.
  #[test]
  fn detection_looks_at_a_running_mkvtoolnix_gui_first() {
    let status = detect_mkvtoolnix("", true);
    assert_eq!(status.found, !status.mkv_toolnix_path.is_empty(), "{status:?}");
  }

  #[test]
  fn detection_reports_a_directory_without_mkvtoolnix_as_not_found() {
    let dir = Dir::new("missing");
    let path = dir.0.to_string_lossy().to_string();
    let status = detect_mkvtoolnix(&path, false);
    assert!(!status.found);
    assert_eq!(status.mkv_toolnix_path, path);
  }

  #[test]
  fn detection_of_an_empty_path_finds_nothing() {
    let status = detect_mkvtoolnix("   ", false);
    assert!(!status.found);
    assert!(status.mkv_toolnix_path.is_empty());
  }

  #[test]
  fn the_executable_of_the_copyable_command_is_in_the_configured_directory() {
    let dir = Dir::new("executable");
    let path = dir.0.to_string_lossy().to_string();
    assert_eq!(mkvextract_executable(&path), dir.0.join(MKVEXTRACT));
    std::fs::write(dir.0.join(tool_file(MKVEXTRACT)), b"").unwrap();
    assert_eq!(mkvextract_executable(&path), dir.0.join(tool_file(MKVEXTRACT)));
  }

  #[test]
  fn gui_mode_and_english_lines_are_classified() {
    use MkvextractLine::*;
    assert_eq!(parse_mkvextract_line("#GUI#progress 42%"), Some(Progress(42)));
    assert_eq!(parse_mkvextract_line("Progress: 7%"), Some(Progress(7)));
    assert_eq!(parse_mkvextract_line("#GUI#progress 120%"), Some(Progress(100)));
    assert_eq!(
      parse_mkvextract_line("#GUI#warning track 7 not found."),
      Some(Warning("track 7 not found."))
    );
    assert_eq!(parse_mkvextract_line("Warning: plain"), Some(Warning("plain")));
    assert_eq!(
      parse_mkvextract_line("#GUI#error No track with the ID 5 was found in the source file."),
      Some(Error("No track with the ID 5 was found in the source file."))
    );
    assert_eq!(parse_mkvextract_line("Error: plain"), Some(Error("plain")));
    assert_eq!(
      parse_mkvextract_line("Fortschritt: 25%"),
      Some(Other("Fortschritt: 25%"))
    );
    assert_eq!(parse_mkvextract_line("#GUI#begin_scanning_playlists"), None);
    assert_eq!(parse_mkvextract_line("   "), None);
  }

  #[test]
  fn output_is_split_at_carriage_returns_and_newlines() {
    let mut lines = Vec::new();
    read_mkvextract_output(&b"Progress: 25%\rProgress: 100%\r\nDone.\n\nlast"[..], |line| {
      lines.push(line.to_owned())
    });
    assert_eq!(lines, ["Progress: 25%", "Progress: 100%", "Done.", "last"]);
  }

  #[test]
  fn reading_stops_at_an_error_of_the_output() {
    struct Failing(bool);
    impl Read for Failing {
      fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if std::mem::replace(&mut self.0, true) {
          Err(std::io::Error::other("closed"))
        } else {
          buf[..4].copy_from_slice(b"half");
          Ok(4)
        }
      }
    }
    let mut lines = Vec::new();
    read_mkvextract_output(Failing(false), |line| lines.push(line.to_owned()));
    assert_eq!(lines, ["half"]);
  }

  #[test]
  fn a_missing_source_is_reported_before_mkvextract_is_looked_for() {
    match spawn_mkvextract("/definitely/not/here.mkv", "/opt/mkvtoolnix", &[]) {
      Err(ExtractError::SourceMissing { path }) => assert_eq!(path, "/definitely/not/here.mkv"),
      other => panic!("expected SourceMissing, got {:?}", other.map(|_| ())),
    }
  }

  #[test]
  fn listing_a_missing_or_non_file_source_fails_before_mkvmerge_runs() {
    let running = AtomicBool::new(false);
    match list_tracks("/definitely/not/here.mkv", "/opt/mkvtoolnix", &running) {
      Err(TrackListError::Other(message)) => assert!(message.contains("does not exist"), "{message}"),
      other => panic!("expected Other, got {other:?}"),
    }
    let dir = Dir::new("not_a_file");
    match list_tracks(&dir.0.to_string_lossy(), "/opt/mkvtoolnix", &running) {
      Err(TrackListError::Other(message)) => assert!(message.contains("is not a file"), "{message}"),
      other => panic!("expected Other, got {other:?}"),
    }
  }

  /// `mkvmerge -J` of a file with a subtitle track, two chapter entries and
  /// a font attachment, as `mkvmerge` v101.0 prints it (trimmed).
  const IDENTIFY: &str = r#"{
    "attachments": [
      {"content_type": "font/ttf", "file_name": "Font.TTF", "id": 1, "size": 10},
      {"content_type": "text/plain", "file_name": "notes", "id": 2, "size": 3},
      {"content_type": "", "file_name": "README.", "id": 3, "size": 3}
    ],
    "chapters": [{"num_entries": 2}],
    "container": {"recognized": true, "supported": true, "type": "Matroska"},
    "errors": [],
    "file_name": "test.mkv",
    "tracks": [
      {"codec": "AVC/H.264/MPEG-4p10", "id": 0, "type": "video",
       "properties": {"codec_id": "V_MPEG4/ISO/AVC", "language": "und", "number": 1}},
      {"codec": "SubRip/SRT", "id": 1, "type": "subtitles",
       "properties": {"codec_id": "S_TEXT/UTF8", "language": "eng", "number": 2, "track_name": "Full"}},
      {"codec": "AAC", "id": 2, "type": "audio", "properties": {"number": 3}}
    ],
    "warnings": []
  }"#;

  fn track(
    id: i64,
    number: i64,
    track_type: &str,
    codec: &str,
    codec_id: &str,
    name: &str,
    language: &str,
  ) -> MkvTrack {
    MkvTrack {
      id,
      number,
      track_type: track_type.to_owned(),
      codec: codec.to_owned(),
      codec_id: codec_id.to_owned(),
      track_name: name.to_owned(),
      language: language.to_owned(),
    }
  }

  #[test]
  fn identify_json_gives_the_tracks_a_chapters_row_and_the_attachments() {
    assert_eq!(
      parse_identify_json(IDENTIFY.as_bytes()).unwrap(),
      [
        track(0, 1, "video", "AVC/H.264/MPEG-4p10", "V_MPEG4/ISO/AVC", "", "und"),
        track(1, 2, "subtitles", "SubRip/SRT", "S_TEXT/UTF8", "Full", "eng"),
        track(2, 3, "audio", "AAC", "", "", "und"),
        track(0, 0, "chapters", "xml", "xml", "", ""),
        track(1, 0, "attachment", "ttf", "ttf", "Font.TTF", ""),
        track(2, 0, "attachment", "plain", "plain", "notes", ""),
        track(3, 0, "attachment", "", "", "README.", ""),
      ]
    );
  }

  #[test]
  fn identify_json_without_tracks_chapter_entries_or_attachments_is_empty() {
    let unrecognised = br#"{"container": {"recognized": false, "supported": false}, "errors": [],
      "file_name": "note.txt", "identification_format_version": 20, "warnings": []}"#;
    assert_eq!(parse_identify_json(unrecognised).unwrap(), []);
    let empty_chapters = br#"{"tracks": [], "chapters": [{"num_entries": 0}], "attachments": []}"#;
    assert_eq!(parse_identify_json(empty_chapters).unwrap(), []);
    match parse_identify_json(b"not json") {
      Err(TrackListError::Parse { detail }) => assert!(!detail.is_empty()),
      other => panic!("expected Parse, got {other:?}"),
    }
  }

  #[test]
  fn an_identify_failure_carries_the_json_errors_or_else_stderr() {
    let missing = br#"{"errors": ["The file 'missing.mkv' could not be opened for reading: open file error.\n"],
      "warnings": []}"#;
    assert_eq!(
      identify_output(Some(2), missing, b""),
      Err(TrackListError::Failed {
        messages: vec!["The file 'missing.mkv' could not be opened for reading: open file error.".to_owned()]
      })
    );
    assert_eq!(
      identify_output(None, b"", b"dyld: Library not loaded\n\n  libQt6Core  \n"),
      Err(TrackListError::Failed {
        messages: vec!["dyld: Library not loaded".to_owned(), "libQt6Core".to_owned()]
      })
    );
    assert_eq!(
      identify_output(Some(2), br#"{"errors": []}"#, b""),
      Err(TrackListError::Failed { messages: Vec::new() })
    );
    assert_eq!(
      identify_output(Some(1), IDENTIFY.as_bytes(), b"").unwrap().len(),
      7,
      "warnings only"
    );
  }

  #[cfg(unix)]
  #[test]
  fn identify_runs_mkvmerge_with_the_json_switch_and_the_file() {
    let dir = Dir::new("identify");
    let source = dir.0.join("a.mkv");
    std::fs::write(&source, b"").unwrap();
    let fixture = dir.0.join("identify.json");
    std::fs::write(&fixture, IDENTIFY).unwrap();
    let args = dir.0.join("args.txt");
    let mkvmerge = script(
      &dir.0,
      MKVMERGE,
      &format!(
        "printf '%s\\n' \"$@\" > '{}'\ncat '{}'",
        args.display(),
        fixture.display()
      ),
    );
    let running = AtomicBool::new(false);
    let identify = |mkvmerge: &Path, source: &str| identify(mkvmerge, source, &running, TIMEOUT);
    let tracks = identify(&mkvmerge, &source.to_string_lossy()).unwrap();
    assert_eq!(tracks.len(), 7);
    assert_eq!(
      std::fs::read_to_string(&args).unwrap(),
      format!("-J\n{}\n", source.display())
    );
    let failing = script(&dir.0, "failing", "echo 'cannot run' >&2\nexit 2");
    assert_eq!(
      identify(&failing, &source.to_string_lossy()),
      Err(TrackListError::Failed {
        messages: vec!["cannot run".to_owned()]
      })
    );
    match identify(&dir.0.join("not-there"), &source.to_string_lossy()) {
      Err(TrackListError::NotAvailable { path, detail }) => {
        assert_eq!(path, dir.0.join("not-there").display().to_string());
        assert!(!detail.is_empty());
      }
      other => panic!("expected NotAvailable, got {other:?}"),
    }
  }

  /// A listing that does not end is stopped when it is cancelled or runs
  /// out of time, and a listing with much output on both pipes still ends.
  #[cfg(unix)]
  #[test]
  fn a_listing_is_stopped_when_cancelled_or_out_of_time() {
    let dir = Dir::new("identify_stops");
    let source = dir.0.join("a.mkv");
    std::fs::write(&source, b"").unwrap();
    let source = source.to_string_lossy().into_owned();
    let stalled = script(&dir.0, "stalled", "exec sleep 30");
    let running = AtomicBool::new(false);
    let started = Instant::now();
    assert_eq!(
      identify(&stalled, &source, &running, Duration::from_millis(200)),
      Err(TrackListError::TimedOut { seconds: 0 })
    );
    assert!(started.elapsed() < Duration::from_secs(5), "stopped, not waited for");
    let cancelled = AtomicBool::new(true);
    let started = Instant::now();
    assert_eq!(
      identify(&stalled, &source, &cancelled, TIMEOUT),
      Err(TrackListError::Cancelled)
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    let fixture = dir.0.join("identify.json");
    std::fs::write(&fixture, IDENTIFY).unwrap();
    let chatty = script(
      &dir.0,
      "chatty",
      &format!("yes warning | head -c 200000 >&2\ncat '{}'", fixture.display()),
    );
    assert_eq!(identify(&chatty, &source, &running, TIMEOUT).unwrap().len(), 7);
  }

  #[cfg(unix)]
  #[test]
  fn mkvextract_runs_in_gui_mode_with_its_output_and_errors_in_one_pipe() {
    let dir = Dir::new("extract");
    let mkvextract = script(
      &dir.0,
      MKVEXTRACT,
      "for arg in \"$@\"; do echo \"arg $arg\"; done\necho '#GUI#progress 50%'\necho 'on stderr' >&2\nexit 1",
    );
    let args = vec!["tracks".to_owned(), "0:/out/a b.srt".to_owned()];
    let (mut child, output) = spawn_extract_tool(&mkvextract, "/in/a.mkv", &args).unwrap();
    let mut lines = Vec::new();
    read_mkvextract_output(output, |line| lines.push(line.to_owned()));
    assert_eq!(
      lines,
      [
        "arg --gui-mode",
        "arg /in/a.mkv",
        "arg tracks",
        "arg 0:/out/a b.srt",
        "#GUI#progress 50%",
        "on stderr"
      ]
    );
    assert_eq!(child.wait().unwrap().code(), Some(1));
    match spawn_extract_tool(&dir.0.join("not-there"), "/in/a.mkv", &args) {
      Err(ExtractError::NotAvailable { path, .. }) => assert_eq!(path, dir.0.join("not-there").display().to_string()),
      other => panic!("expected NotAvailable, got {:?}", other.map(|_| ())),
    }
  }
}
