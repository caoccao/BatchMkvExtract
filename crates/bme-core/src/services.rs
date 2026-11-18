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

//! File intake, output directories, BetterMediaInfo and running processes.
//! Every function is blocking; the caller moves it off its UI thread.

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::protocol::BetterMediaInfoStatus;

const MKV_EXTENSION: &str = "mkv";

fn better_media_info_exe_name() -> &'static str {
  if cfg!(target_os = "windows") {
    "BetterMediaInfo.exe"
  } else {
    "BetterMediaInfo"
  }
}

/// Whether `path` names a Matroska video file (`.mkv`, in any case), the
/// only files the app takes.
pub fn is_mkv(path: &Path) -> bool {
  path
    .extension()
    .and_then(|e| e.to_str())
    .map(|e| e.eq_ignore_ascii_case(MKV_EXTENSION))
    .unwrap_or(false)
}

/// Whether an extraction can write into the output directory `path`: its
/// nearest existing ancestor (the path itself, if it exists) is a directory
/// a file can be created in; the missing directories are created before the
/// extraction ([`ensure_output_path`]). An existing file is not a directory.
/// Nothing is changed: the probe is a new file of this call's own
/// ([`unique_file::create`](crate::unique_file::create), which never opens
/// a file or a link that is there), removed again.
pub fn check_output_dir_writable(path: String) -> Result<bool> {
  let mut current = PathBuf::from(&path);
  if current.as_os_str().is_empty() {
    return Ok(false);
  }
  loop {
    if current.exists() {
      break;
    }
    current = match current.parent() {
      // A relative path's ancestors end in the working directory, where
      // creating it and `mkvextract` would write.
      Some(parent) if parent.as_os_str().is_empty() => PathBuf::from("."),
      Some(parent) => parent.to_path_buf(),
      None => return Ok(false),
    };
  }
  if !current.is_dir() {
    return Ok(false);
  }
  match crate::unique_file::create(|tag| current.join(format!(".batchmkvextract_writecheck_{tag}"))) {
    Ok((probe, file)) => {
      drop(file);
      let _ = std::fs::remove_file(&probe);
      Ok(true)
    }
    Err(_) => Ok(false),
  }
}

/// Whether the exact directory `path` already exists. Used by the output-path
/// dialog to warn that a non-existent path will be created at extract time.
pub fn output_path_exists(path: String) -> Result<bool> {
  Ok(Path::new(&path).is_dir())
}

/// Makes sure the output directory `path` exists before an extraction
/// writes into it, creating it and its missing parents. A file in its place
/// is an error.
pub fn ensure_output_path(path: &str) -> Result<()> {
  let p = Path::new(path);
  if p.exists() {
    if !p.is_dir() {
      anyhow::bail!("{path} exists but is not a directory");
    }
    return Ok(());
  }
  std::fs::create_dir_all(p).map_err(anyhow::Error::msg)?;
  Ok(())
}

fn common_better_media_info_dirs() -> Vec<PathBuf> {
  let mut dirs: Vec<PathBuf> = Vec::new();
  #[cfg(target_os = "windows")]
  {
    for env_var in ["LOCALAPPDATA"] {
      if let Ok(value) = std::env::var(env_var) {
        if !value.is_empty() {
          dirs.push(PathBuf::from(value).join("Programs").join("BetterMediaInfo"));
        }
      }
    }
    for env_var in ["ProgramFiles", "ProgramFiles(x86)"] {
      if let Ok(value) = std::env::var(env_var) {
        if !value.is_empty() {
          dirs.push(PathBuf::from(value).join("BetterMediaInfo"));
        }
      }
    }
  }
  #[cfg(target_os = "macos")]
  {
    dirs.push(PathBuf::from("/Applications/BetterMediaInfo.app/Contents/MacOS"));
  }
  #[cfg(target_os = "linux")]
  {
    dirs.push(PathBuf::from("/usr/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
  }
  dirs
}

/// Whether BetterMediaInfo is at `user_path` (its directory or the
/// executable itself), looking nowhere else: for a path being typed, which
/// a BetterMediaInfo found elsewhere must not replace. An empty path is not
/// found.
pub fn find_better_media_info_at(user_path: &str) -> BetterMediaInfoStatus {
  let trimmed = user_path.trim();
  match (!trimmed.is_empty())
    .then(|| find_better_media_info_dir(Path::new(trimmed)))
    .flatten()
  {
    Some(found) => BetterMediaInfoStatus {
      found: true,
      path: found.to_string_lossy().to_string(),
    },
    None => BetterMediaInfoStatus {
      found: false,
      path: String::new(),
    },
  }
}

/// Finds BetterMediaInfo without touching the config: with `check_running`,
/// the directory of a running BetterMediaInfo first, then `user_path` (its
/// directory or the executable itself), then the usual install places.
pub fn detect_better_media_info(user_path: String, check_running: bool) -> Result<BetterMediaInfoStatus> {
  if check_running {
    if let Some(dir) = find_running_better_media_info_dir() {
      return Ok(BetterMediaInfoStatus {
        found: true,
        path: dir.to_string_lossy().to_string(),
      });
    }
  }
  let trimmed = user_path.trim();
  if !trimmed.is_empty() {
    if let Some(found) = find_better_media_info_dir(Path::new(trimmed)) {
      return Ok(BetterMediaInfoStatus {
        found: true,
        path: found.to_string_lossy().to_string(),
      });
    }
  }
  for dir in common_better_media_info_dirs() {
    if let Some(found) = find_better_media_info_dir(&dir) {
      return Ok(BetterMediaInfoStatus {
        found: true,
        path: found.to_string_lossy().to_string(),
      });
    }
  }
  Ok(BetterMediaInfoStatus {
    found: false,
    path: String::new(),
  })
}

fn find_better_media_info_dir(path: &Path) -> Option<PathBuf> {
  if !path.exists() {
    return None;
  }
  let exe_name = better_media_info_exe_name();
  if path.is_file() {
    let matches = path
      .file_name()
      .and_then(|n| n.to_str())
      .map(|n| n.eq_ignore_ascii_case(exe_name))
      .unwrap_or(false);
    if matches {
      return path.parent().map(|p| p.to_path_buf());
    }
    return None;
  }
  if path.is_dir() && path.join(exe_name).is_file() {
    return Some(path.to_path_buf());
  }
  None
}

#[cfg(target_os = "macos")]
fn find_macos_app_bundle(bin: &Path) -> Option<PathBuf> {
  // The Mach-O binary lives at `<bundle>.app/Contents/MacOS/BetterMediaInfo`,
  // so the `.app` ancestor is at most three levels up. Walk a bounded number
  // of parents instead of looping unbounded.
  let mut current = bin.parent()?;
  for _ in 0..4 {
    if current.extension().and_then(|s| s.to_str()) == Some("app") {
      return Some(current.to_path_buf());
    }
    current = current.parent()?;
  }
  None
}

fn find_running_better_media_info_dir() -> Option<PathBuf> {
  find_running_process_dir(better_media_info_exe_name())
}

/// The directory of the executable of a running process called `exe_name`
/// (compared case-insensitively).
pub fn find_running_process_dir(exe_name: &str) -> Option<PathBuf> {
  let sys = sysinfo::System::new_all();
  for process in sys.processes().values() {
    let name = process.name().to_string_lossy();
    if !name.eq_ignore_ascii_case(exe_name) {
      continue;
    }
    if let Some(exe) = process.exe() {
      if let Some(parent) = exe.parent() {
        return Some(parent.to_path_buf());
      }
    }
  }
  None
}

/// The paths the app was started with (an "Open with" or a drop on its
/// icon), for intake as if they were dropped. Read as the OS gives them, so
/// a name that is not valid UTF-8 is skipped by [`get_mkv_files`] instead of
/// stopping the app.
pub fn get_launch_args() -> Vec<PathBuf> {
  std::env::args_os().skip(1).map(PathBuf::from).collect()
}

/// The local path of a `file://` URL, as macOS hands files to an app
/// ("Open With", a drop on its Dock icon): the host must be empty or
/// `localhost`, and the path is percent-decoded byte by byte, so a name
/// that is not valid UTF-8 survives on Linux and macOS. `None` for any
/// other URL.
pub fn path_from_file_url(url: &str) -> Option<PathBuf> {
  let rest = url.strip_prefix("file://").or_else(|| url.strip_prefix("FILE://"))?;
  let path = match rest.find('/') {
    Some(0) => rest,
    Some(slash) if rest[..slash].eq_ignore_ascii_case("localhost") => &rest[slash..],
    _ => return None,
  };
  let path = path.split(['?', '#']).next().unwrap_or_default();
  let bytes = path.as_bytes();
  let mut decoded = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = |b: u8| (b as char).to_digit(16).map(|digit| digit as u8);
    match (
      bytes[i],
      bytes.get(i + 1).copied().and_then(hex),
      bytes.get(i + 2).copied().and_then(hex),
    ) {
      (b'%', Some(high), Some(low)) => {
        decoded.push(high * 16 + low);
        i += 3;
      }
      (byte, _, _) => {
        decoded.push(byte);
        i += 1;
      }
    }
  }
  #[cfg(unix)]
  {
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(decoded)))
  }
  #[cfg(not(unix))]
  {
    // `file:///C:/Media/a.mkv`: the drive follows the first slash.
    let text = String::from_utf8(decoded).ok()?;
    let text = text
      .strip_prefix('/')
      .filter(|rest| rest.as_bytes().get(1) == Some(&b':'))
      .unwrap_or(&text);
    Some(PathBuf::from(text.replace('/', "\\")))
  }
}

/// What [`get_mkv_files`] found in the paths it was given.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MkvFiles {
  /// The `.mkv` files: a given file as it is, a folder's files sorted, in
  /// the order of the paths.
  pub files: Vec<String>,
  /// What could not be used: a path that does not exist, a folder that
  /// cannot be listed, and a path or a file in a folder whose name is not
  /// valid UTF-8 (shown lossily). Files of other types are not counted; they
  /// are left out on purpose.
  pub skipped: Vec<String>,
}

/// The `.mkv` files of dropped, chosen or launch-argument `paths`. A folder
/// contributes the `.mkv` files directly inside it (one level, as 0.3.0
/// did: a subfolder is a separate drop). A relative path (a launch argument)
/// is taken from the working directory, as the shell meant it, so every
/// listed file has a full path. One path that cannot be read never costs
/// the others; it is listed in [`MkvFiles::skipped`].
pub fn get_mkv_files(paths: impl IntoIterator<Item = impl Into<PathBuf>>) -> MkvFiles {
  let mut found = MkvFiles::default();
  for input in paths {
    let input: PathBuf = input.into();
    let input = std::path::absolute(&input).unwrap_or(input);
    let Some(text) = input.to_str() else {
      log::warn!("Skipped {}: its name is not valid UTF-8", input.display());
      found.skipped.push(input.to_string_lossy().into_owned());
      continue;
    };
    let input = text.to_owned();
    let path = Path::new(input.as_str());
    if !path.exists() {
      log::warn!("Skipped {input}: it does not exist");
      found.skipped.push(input);
      continue;
    }
    if path.is_dir() {
      let listing = match path.read_dir() {
        Ok(listing) => listing,
        Err(error) => {
          log::warn!("Skipped the folder {input}: {error}");
          found.skipped.push(input);
          continue;
        }
      };
      let mut entries: Vec<PathBuf> = listing
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && is_mkv(p))
        .collect();
      entries.sort();
      for p in entries {
        match p.to_str() {
          Some(s) => found.files.push(s.to_owned()),
          None => {
            log::warn!("Skipped {}: its name is not valid UTF-8", p.display());
            found.skipped.push(p.to_string_lossy().into_owned());
          }
        }
      }
    } else if path.is_file() && is_mkv(path) {
      found.files.push(input);
    }
  }
  found
}

/// Opens `paths` in the BetterMediaInfo installed in `dir` (the directory
/// [`detect_better_media_info`] reports).
pub fn launch_better_media_info_from(dir: &str, paths: Vec<String>) -> Result<()> {
  let exe = Path::new(dir.trim()).join(better_media_info_exe_name());
  if !exe.is_file() {
    anyhow::bail!("BetterMediaInfo executable not found at {}", exe.display());
  }

  // On macOS, going through `open` lets Launch Services activate the bundle
  // cleanly — spawning the bundle's Mach-O binary directly causes a brief
  // window flash because the child never goes through proper app activation.
  // BetterMediaInfo reads its files from its launch arguments only, and
  // `open --args` passes them only to an instance it starts, so `-n` starts
  // a new one each time, as spawning the executable does elsewhere.
  #[cfg(target_os = "macos")]
  {
    if let Some(app_bundle) = find_macos_app_bundle(&exe) {
      let mut cmd = std::process::Command::new("/usr/bin/open");
      cmd.args(open_args(&app_bundle, &paths));
      cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
      let child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to launch BetterMediaInfo via open: {}", e))?;
      reap(child);
      return Ok(());
    }
  }

  let mut cmd = std::process::Command::new(&exe);
  cmd
    .args(&paths)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
  #[cfg(target_os = "windows")]
  {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
  }
  let child = cmd
    .spawn()
    .map_err(|e| anyhow::anyhow!("Failed to launch BetterMediaInfo: {}", e))?;
  reap(child);
  Ok(())
}

/// The arguments of `/usr/bin/open` that start a new instance of `bundle`
/// with `paths` as its launch arguments.
#[cfg(any(target_os = "macos", test))]
fn open_args(bundle: &Path, paths: &[String]) -> Vec<std::ffi::OsString> {
  let mut args: Vec<std::ffi::OsString> = vec!["-n".into(), "-a".into(), bundle.as_os_str().to_owned()];
  if !paths.is_empty() {
    args.push("--args".into());
    args.extend(paths.iter().map(Into::into));
  }
  args
}

/// Waits for a launched process on a thread of its own, so an ended one does
/// not stay behind as a zombie until the app exits.
fn reap(mut child: std::process::Child) {
  std::thread::spawn(move || {
    let _ = child.wait();
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A fresh directory under the system temp dir, removed when dropped.
  struct Dir(PathBuf);

  impl Dir {
    fn new(name: &str) -> Self {
      let path = std::env::temp_dir().join(format!("bme_services_{name}_{}", std::process::id()));
      let _ = std::fs::remove_dir_all(&path);
      std::fs::create_dir_all(&path).unwrap();
      Self(path)
    }

    fn path(&self, name: &str) -> String {
      self.0.join(name).to_string_lossy().into_owned()
    }
  }

  impl Drop for Dir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  #[test]
  fn only_mkv_files_are_taken_in_any_case() {
    assert!(is_mkv(Path::new("/m/a.mkv")));
    assert!(is_mkv(Path::new("/m/a.MKV")));
    assert!(!is_mkv(Path::new("/m/a.mka")));
    assert!(!is_mkv(Path::new("/m/mkv")));
    assert!(!is_mkv(Path::new("/m/a.mkv.part")));
  }

  #[test]
  fn open_starts_a_new_instance_with_the_files_as_arguments() {
    let bundle = Path::new("/Applications/BetterMediaInfo.app");
    let args = |paths: &[&str]| -> Vec<String> {
      let paths: Vec<String> = paths.iter().map(|path| path.to_string()).collect();
      open_args(bundle, &paths)
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect()
    };
    assert_eq!(
      args(&["/m/a.mkv", "/m/b c.mkv"]),
      [
        "-n",
        "-a",
        "/Applications/BetterMediaInfo.app",
        "--args",
        "/m/a.mkv",
        "/m/b c.mkv"
      ]
    );
    assert_eq!(args(&[]), ["-n", "-a", "/Applications/BetterMediaInfo.app"]);
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn the_app_bundle_of_an_executable_is_found_a_few_levels_up() {
    assert_eq!(
      find_macos_app_bundle(Path::new(
        "/Applications/BetterMediaInfo.app/Contents/MacOS/BetterMediaInfo"
      )),
      Some(PathBuf::from("/Applications/BetterMediaInfo.app"))
    );
    assert_eq!(
      find_macos_app_bundle(Path::new("/opt/bmi/bin/tools/BetterMediaInfo")),
      None
    );
    assert_eq!(find_macos_app_bundle(Path::new("/BetterMediaInfo")), None);
  }

  #[cfg(unix)]
  #[test]
  fn a_reaped_child_leaves_no_zombie() {
    let child = std::process::Command::new("/bin/sh")
      .arg("-c")
      .arg("exit 0")
      .spawn()
      .unwrap();
    let pid = child.id().to_string();
    reap(child);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
      let state = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid])
        .output()
        .unwrap();
      let state = String::from_utf8_lossy(&state.stdout).trim().to_owned();
      if state.is_empty() {
        break;
      }
      assert!(
        std::time::Instant::now() < deadline,
        "process {pid} is still listed as {state:?}"
      );
      std::thread::sleep(std::time::Duration::from_millis(20));
    }
  }

  #[test]
  fn one_unreadable_path_does_not_cost_the_others() {
    let dir = Dir::new("mkv_files");
    std::fs::create_dir_all(dir.0.join("ok")).unwrap();
    std::fs::create_dir_all(dir.0.join("locked")).unwrap();
    std::fs::write(dir.0.join("ok").join("b.mkv"), b"").unwrap();
    std::fs::write(dir.0.join("ok").join("A.MKV"), b"").unwrap();
    std::fs::write(dir.0.join("ok").join("a.srt"), b"").unwrap();
    std::fs::write(dir.0.join("ok").join("notes.txt"), b"").unwrap();
    std::fs::create_dir_all(dir.0.join("ok").join("sub.mkv")).unwrap();
    std::fs::write(dir.0.join("locked").join("c.mkv"), b"").unwrap();
    std::fs::write(dir.0.join("single.mkv"), b"").unwrap();
    std::fs::write(dir.0.join("single.mp4"), b"").unwrap();
    // `DirEntry::path` joins with the platform's separator (`\` on Windows).
    let listed = |folder: &str, name: &str| dir.0.join(folder).join(name).to_string_lossy().into_owned();
    #[cfg(unix)]
    let locked = {
      use std::os::unix::fs::PermissionsExt;
      std::fs::set_permissions(dir.0.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
      // Root reads any folder; then there is nothing to skip.
      std::fs::read_dir(dir.0.join("locked")).is_err()
    };
    // A folder cannot be made unreadable this way on Windows.
    #[cfg(not(unix))]
    let locked = false;
    let found = get_mkv_files(vec![
      dir.path("locked"),
      dir.path("ok"),
      dir.path("missing.mkv"),
      dir.path("single.mkv"),
      dir.path("single.mp4"),
    ]);
    let mut files = vec![listed("ok", "A.MKV"), listed("ok", "b.mkv"), dir.path("single.mkv")];
    let mut skipped = vec![dir.path("missing.mkv")];
    if locked {
      skipped.insert(0, dir.path("locked"));
    } else {
      files.insert(0, listed("locked", "c.mkv"));
    }
    assert_eq!(found.files, files);
    assert_eq!(found.skipped, skipped);
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      std::fs::set_permissions(dir.0.join("locked"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
  }

  /// A file in a folder whose name is not valid UTF-8 is named in the
  /// skipped list (lossily) instead of being dropped silently.
  /// A launch argument that is not valid UTF-8 is skipped and named, and the
  /// others are taken.
  #[cfg(target_os = "linux")]
  #[test]
  fn a_path_that_is_not_utf8_is_skipped_and_the_others_are_taken() {
    use std::os::unix::ffi::OsStrExt;
    let dir = Dir::new("args_not_utf8");
    std::fs::write(dir.0.join("ok.mkv"), b"").unwrap();
    let bad = dir.0.join(std::ffi::OsStr::from_bytes(b"caf\xe9.mkv"));
    std::fs::write(&bad, b"").unwrap();
    let found = get_mkv_files([dir.0.join("ok.mkv"), bad.clone()]);
    assert_eq!(found.files, [dir.path("ok.mkv")]);
    assert_eq!(found.skipped, [bad.to_string_lossy().into_owned()]);
  }

  #[test]
  fn file_urls_become_local_paths() {
    #[cfg(unix)]
    {
      assert_eq!(
        path_from_file_url("file:///Users/me/My%20Movie%20%5B1080p%5D.mkv"),
        Some(PathBuf::from("/Users/me/My Movie [1080p].mkv"))
      );
      assert_eq!(
        path_from_file_url("file://localhost/media/%E6%98%A0%E7%94%BB/"),
        Some(PathBuf::from("/media/映画/"))
      );
      assert_eq!(
        path_from_file_url("file:///a%2"),
        Some(PathBuf::from("/a%2")),
        "a stray % stays"
      );
      #[cfg(target_os = "linux")]
      {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
          path_from_file_url("file:///m/caf%E9.mkv")
            .unwrap()
            .as_os_str()
            .as_bytes(),
          b"/m/caf\xe9.mkv"
        );
      }
    }
    #[cfg(windows)]
    assert_eq!(
      path_from_file_url("file:///C:/Media/My%20Movie.mkv"),
      Some(PathBuf::from(r"C:\Media\My Movie.mkv"))
    );
    assert_eq!(path_from_file_url("https://example.com/a.mkv"), None);
    assert_eq!(path_from_file_url("file://server/share/a.mkv"), None);
  }

  #[test]
  fn a_relative_path_is_taken_from_the_working_directory() {
    let name = format!("bme_relative_intake_{}.mkv", std::process::id());
    std::fs::write(&name, b"").unwrap();
    let found = get_mkv_files([name.clone()]);
    let _ = std::fs::remove_file(&name);
    let full = std::env::current_dir().unwrap().join(&name);
    assert_eq!(found.files, [full.to_string_lossy().into_owned()]);
    assert!(Path::new(&found.files[0]).is_absolute());
  }

  #[cfg(target_os = "linux")]
  #[test]
  fn a_file_name_that_is_not_utf8_is_skipped() {
    use std::os::unix::ffi::OsStrExt;
    let dir = Dir::new("not_utf8");
    let name = std::ffi::OsStr::from_bytes(b"caf\xe9.mkv");
    std::fs::write(dir.0.join(name), b"").unwrap();
    let found = get_mkv_files(vec![dir.0.to_string_lossy().into_owned()]);
    assert!(found.files.is_empty());
    assert_eq!(found.skipped, [dir.0.join(name).to_string_lossy().into_owned()]);
  }

  #[test]
  fn an_output_directory_is_writable_where_an_extraction_can_write_it() {
    let dir = Dir::new("writable");
    assert!(check_output_dir_writable(dir.path("")).unwrap(), "the directory");
    assert!(check_output_dir_writable(dir.path("new/sub")).unwrap(), "created");
    std::fs::write(dir.0.join("Movie.mkv"), b"extracted").unwrap();
    assert!(
      !check_output_dir_writable(dir.path("Movie.mkv")).unwrap(),
      "a file is no directory"
    );
    assert_eq!(
      std::fs::read(dir.0.join("Movie.mkv")).unwrap(),
      b"extracted",
      "left as it was"
    );
    assert!(
      !check_output_dir_writable(dir.path("Movie.mkv/inside")).unwrap(),
      "under a file"
    );
    assert!(!check_output_dir_writable(String::new()).unwrap(), "no path at all");
    // A new relative folder is created in the working directory, which is
    // checked.
    let relative = format!("bme_relative_check_{}/sub", std::process::id());
    assert!(check_output_dir_writable(relative.clone()).unwrap(), "relative");
    assert!(!Path::new(&relative).exists(), "nothing created");
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      std::fs::create_dir_all(dir.0.join("read-only")).unwrap();
      std::fs::set_permissions(dir.0.join("read-only"), std::fs::Permissions::from_mode(0o555)).unwrap();
      let writable = std::fs::File::create(dir.0.join("read-only").join("probe")).is_ok();
      // Root writes into any folder.
      if !writable {
        assert!(
          !check_output_dir_writable(dir.path("read-only/new")).unwrap(),
          "read-only"
        );
      }
      std::fs::set_permissions(dir.0.join("read-only"), std::fs::Permissions::from_mode(0o755)).unwrap();
    }
  }

  /// The probe never opens a file that is there, also one with the name an
  /// earlier version used, and checks running at once each use a name of
  /// their own and leave nothing behind.
  #[test]
  fn the_writable_check_leaves_existing_files_alone() {
    let dir = Dir::new("writable_probe");
    let earlier = dir
      .0
      .join(format!(".batchmkvextract_writecheck_{}", std::process::id()));
    std::fs::write(&earlier, b"kept").unwrap();
    let checks: Vec<_> = (0..8)
      .map(|_| {
        let path = dir.path("");
        std::thread::spawn(move || check_output_dir_writable(path).unwrap())
      })
      .collect();
    for check in checks {
      assert!(check.join().unwrap());
    }
    assert_eq!(std::fs::read(&earlier).unwrap(), b"kept");
    let left: Vec<_> = std::fs::read_dir(&dir.0)
      .unwrap()
      .map(|entry| entry.unwrap().path())
      .collect();
    assert_eq!(left, [earlier], "only the file that was there");
  }

  #[test]
  fn an_output_path_exists_only_as_a_directory() {
    let dir = Dir::new("exists");
    std::fs::write(dir.0.join("file"), b"").unwrap();
    assert!(output_path_exists(dir.path("")).unwrap());
    assert!(!output_path_exists(dir.path("file")).unwrap());
    assert!(!output_path_exists(dir.path("missing")).unwrap());
  }

  #[test]
  fn the_output_directory_is_created_unless_a_file_is_in_its_place() {
    let dir = Dir::new("ensure");
    let nested = dir.path("new/sub");
    ensure_output_path(&nested).unwrap();
    assert!(Path::new(&nested).is_dir());
    ensure_output_path(&nested).unwrap();
    std::fs::write(dir.0.join("file"), b"").unwrap();
    let error = ensure_output_path(&dir.path("file")).unwrap_err();
    assert!(error.to_string().contains("is not a directory"), "{error}");
    let error = ensure_output_path(&dir.path("file/inside")).unwrap_err();
    assert!(!error.to_string().is_empty());
  }

  #[test]
  fn better_media_info_is_found_by_its_directory_or_its_executable() {
    let dir = Dir::new("bmi");
    let exe = dir.0.join(better_media_info_exe_name());
    let status = |path: &str| detect_better_media_info(path.to_owned(), false).unwrap();
    let elsewhere = status("");
    assert_eq!(elsewhere.found, !elsewhere.path.is_empty(), "the usual places");
    assert!(!status(&dir.path("missing")).found || elsewhere.found);
    std::fs::write(&exe, b"").unwrap();
    let path = dir.0.to_string_lossy().into_owned();
    assert_eq!(
      status(&format!(" {path} ")),
      BetterMediaInfoStatus {
        found: true,
        path: path.clone()
      }
    );
    assert_eq!(
      status(&exe.to_string_lossy()),
      BetterMediaInfoStatus {
        found: true,
        path: path.clone()
      }
    );
    // At the path only: the same, and nothing for a path without it, even
    // when BetterMediaInfo is installed in a usual place.
    assert_eq!(find_better_media_info_at(&exe.to_string_lossy()), status(&path));
    let not_there = BetterMediaInfoStatus {
      found: false,
      path: String::new(),
    };
    assert_eq!(find_better_media_info_at(&dir.path("missing")), not_there);
    assert_eq!(find_better_media_info_at(&format!("{path}/Better")), not_there);
    assert_eq!(find_better_media_info_at(" "), not_there);
    std::fs::write(dir.0.join("Other.exe"), b"").unwrap();
    assert_eq!(find_better_media_info_dir(&dir.0.join("Other.exe")), None);
    assert_eq!(find_better_media_info_dir(&dir.0.join("missing")), None);
    let empty = Dir::new("bmi_empty");
    assert_eq!(find_better_media_info_dir(&empty.0), None);
  }

  /// A running BetterMediaInfo (there may be one on a developer machine) is
  /// found first; either way the status names a directory exactly when it
  /// found one.
  #[test]
  fn better_media_info_detection_looks_at_a_running_one_first() {
    let status = detect_better_media_info(String::new(), true).unwrap();
    assert_eq!(status.found, !status.path.is_empty(), "{status:?}");
  }

  #[test]
  fn a_process_that_is_not_running_has_no_directory() {
    assert_eq!(find_running_process_dir("bme-no-such-process-name"), None);
  }

  #[test]
  fn the_launch_arguments_leave_out_the_program() {
    let args = get_launch_args();
    assert_eq!(args.len(), std::env::args().count() - 1);
  }

  #[test]
  fn launching_better_media_info_from_a_directory_without_it_fails() {
    let dir = Dir::new("no_bmi");
    let err = launch_better_media_info_from(&dir.path(""), vec!["/media/a.mkv".to_owned()]).unwrap_err();
    assert!(err.to_string().contains("executable not found"), "{err}");
  }

  /// Outside an app bundle the executable is started with the files as its
  /// arguments.
  #[cfg(unix)]
  #[test]
  fn better_media_info_gets_the_files_as_its_arguments() {
    use std::os::unix::fs::PermissionsExt;
    let dir = Dir::new("launch_bmi");
    let received = dir.0.join("received.txt");
    let exe = dir.0.join(better_media_info_exe_name());
    std::fs::write(
      &exe,
      format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{0}.tmp'\nmv '{0}.tmp' '{0}'\n",
        received.display()
      ),
    )
    .unwrap();
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    launch_better_media_info_from(
      &format!(" {} ", dir.0.display()),
      vec!["/m/a.mkv".to_owned(), "/m/b c.mkv".to_owned()],
    )
    .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !received.exists() {
      assert!(
        std::time::Instant::now() < deadline,
        "BetterMediaInfo was never started"
      );
      std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(std::fs::read_to_string(&received).unwrap(), "/m/a.mkv\n/m/b c.mkv\n");
    // A file that cannot be executed is reported.
    std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o644)).unwrap();
    let err = launch_better_media_info_from(&dir.path(""), Vec::new()).unwrap_err();
    assert!(err.to_string().contains("Failed to launch"), "{err}");
  }
}
