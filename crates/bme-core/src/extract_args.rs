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

//! The `mkvextract` command of one source file, as 0.3.0 built it: the
//! profile's auto-selection of a file's rows, the output file name of each
//! selected row (the profile's template for the row type, then the extension
//! of its codec), the arguments `tracks <id>:<path>… chapters <path>
//! attachments <id>:<path>…`, and the copyable command. The golden test
//! `tests/extract_args_golden.rs` holds these functions to the values
//! recorded from 0.3.0.

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::{Component, MAIN_SEPARATOR_STR, Path};

use crate::config::ConfigProfile;
use crate::protocol::MkvTrack;

/// The values a filename template can name: `{file_name}`, `{track_id}`,
/// `{track_number}`, `{language}`, `{codec_name}` and `{track_name}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemplateContext<'a> {
  /// The source file's name without its extension.
  pub file_name: &'a str,
  pub track_id: i64,
  pub track_number: i64,
  pub language: &'a str,
  pub codec_name: &'a str,
  pub track_name: &'a str,
}

impl<'a> TemplateContext<'a> {
  /// The context of `track` of the source file whose name without its
  /// extension is `file_name`.
  pub fn new(file_name: &'a str, track: &'a MkvTrack) -> Self {
    Self {
      file_name,
      track_id: track.id,
      track_number: track.number,
      language: &track.language,
      codec_name: &track.codec,
      track_name: &track.track_name,
    }
  }

  /// The text of the placeholder `name`; `None` for a name that is not one.
  fn value(&self, name: &str) -> Option<Cow<'a, str>> {
    match name {
      "file_name" => Some(Cow::Borrowed(self.file_name)),
      "track_id" => Some(Cow::Owned(self.track_id.to_string())),
      "track_number" => Some(Cow::Owned(self.track_number.to_string())),
      "language" => Some(sanitize_file_name_part(self.language)),
      "codec_name" => Some(sanitize_file_name_part(self.codec_name)),
      "track_name" => Some(sanitize_file_name_part(self.track_name)),
      _ => None,
    }
  }
}

/// The characters a file name cannot hold on Windows (`/` nowhere).
const UNSAFE_FILE_NAME_CHARS: [char; 9] = ['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// `value` with every [`UNSAFE_FILE_NAME_CHARS`] replaced by `_`, so a codec
/// or track name cannot add a directory or an invalid character.
fn sanitize_file_name_part(value: &str) -> Cow<'_, str> {
  if value.contains(UNSAFE_FILE_NAME_CHARS) {
    Cow::Owned(value.replace(UNSAFE_FILE_NAME_CHARS, "_"))
  } else {
    Cow::Borrowed(value)
  }
}

/// A filename template with its placeholders filled in, in one pass from left
/// to right. `{{` and `}}` are a literal `{` and `}`. A `{name}` that is not
/// a placeholder, and a `{` that is never closed, stay as they are, so a typo
/// shows in the file name. `{language}`, `{codec_name}` and `{track_name}`
/// come from the file's metadata and are sanitised
/// ([`sanitize_file_name_part`]); the other values are used as they are.
pub fn render_template(template: &str, context: &TemplateContext) -> String {
  let bytes = template.as_bytes();
  let len = bytes.len();
  let mut out = String::with_capacity(len);
  let mut i = 0;
  // The braces are ASCII, so every index the scan stops at is a character
  // boundary of the UTF-8 text.
  while i < len {
    match bytes[i] {
      b'{' => {
        if i + 1 < len && bytes[i + 1] == b'{' {
          out.push('{');
          i += 2;
          continue;
        }
        let mut j = i + 1;
        while j < len && bytes[j] != b'}' && bytes[j] != b'{' {
          j += 1;
        }
        if j < len && bytes[j] == b'}' {
          match context.value(&template[i + 1..j]) {
            Some(value) => out.push_str(&value),
            None => out.push_str(&template[i..=j]),
          }
          i = j + 1;
        } else {
          out.push_str(&template[i..j]);
          i = j;
        }
      }
      b'}' => {
        out.push('}');
        i += if i + 1 < len && bytes[i + 1] == b'}' { 2 } else { 1 };
      }
      _ => {
        let next = template[i..].find(['{', '}']).map_or(len, |offset| i + offset);
        out.push_str(&template[i..next]);
        i = next;
      }
    }
  }
  out
}

/// The profile's template for a row type; a type other than video, audio,
/// subtitles, chapters and attachment uses the video template.
pub fn pick_template_for_track_type<'a>(profile: &'a ConfigProfile, track_type: &str) -> &'a str {
  match track_type {
    MkvTrack::TYPE_AUDIO => &profile.audio_template,
    MkvTrack::TYPE_SUBTITLES => &profile.subtitle_template,
    MkvTrack::TYPE_CHAPTERS => &profile.chapters_template,
    MkvTrack::TYPE_ATTACHMENT => &profile.attachments_template,
    _ => &profile.video_template,
  }
}

/// Whether `mkvextract` can extract `track` in its own format: the chapters
/// row, every attachment, and a track whose codec `mkvextract`'s manual lists
/// under "Output file formats" (checked against `mkvextract` v101.0, which
/// also takes AV1). Any other track (FFV1, ProRes, MPEG-4 Part 2, Motion
/// JPEG, uncompressed video, floating-point PCM, DVB subtitles, VobSub
/// buttons) makes `mkvextract` refuse the whole extraction ("Extraction of
/// track ID … with the CodecID … is not supported"), so it is never
/// selected.
pub fn is_extractable(track: &MkvTrack) -> bool {
  match track.track_type.as_str() {
    MkvTrack::TYPE_CHAPTERS | MkvTrack::TYPE_ATTACHMENT => true,
    _ => is_extractable_codec(&track.codec_id),
  }
}

fn is_extractable_codec(codec_id: &str) -> bool {
  const EXACT: [&str; 34] = [
    "A_AC3",
    "A_EAC3",
    "A_ALAC",
    "A_FLAC",
    "A_MPEG/L2",
    "A_MPEG/L3",
    "A_OPUS",
    "A_PCM/INT/LIT",
    "A_PCM/INT/BIG",
    "A_TRUEHD",
    "A_MLP",
    "A_TTA1",
    "A_VORBIS",
    "A_WAVPACK4",
    "S_HDMV/PGS",
    "S_HDMV/TEXTST",
    "S_KATE",
    "S_TEXT/SSA",
    "S_TEXT/ASS",
    "S_SSA",
    "S_ASS",
    "S_TEXT/UTF8",
    "S_TEXT/ASCII",
    "S_VOBSUB",
    "S_TEXT/USF",
    "S_TEXT/WEBVTT",
    "V_MPEG1",
    "V_MPEG2",
    "V_MPEG4/ISO/AVC",
    "V_MPEGH/ISO/HEVC",
    "V_MS/VFW/FOURCC",
    "V_THEORA",
    "V_VP8",
    "V_VP9",
  ];
  const PREFIXES: [&str; 5] = ["A_AAC", "A_AC3/", "A_DTS", "A_REAL/", "V_REAL/"];
  codec_id == "V_AV1" || EXACT.contains(&codec_id) || PREFIXES.iter().any(|prefix| codec_id.starts_with(prefix))
}

/// The profile's auto-selection of a file's rows, as in 0.3.0. A video,
/// audio or subtitle track is selected when
/// its type's checkbox is on and its language is in the type's
/// comma-separated list, compared without case; an empty list matches every
/// language. The chapters row and attachments are selected exactly when their
/// checkbox is on. Any other row, and a track `mkvextract` cannot extract
/// ([`is_extractable`]), is never selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSelector {
  select_video: bool,
  select_audio: bool,
  select_subtitle: bool,
  select_chapters: bool,
  select_attachments: bool,
  video_languages: Option<HashSet<String>>,
  audio_languages: Option<HashSet<String>>,
  subtitle_languages: Option<HashSet<String>>,
}

impl TrackSelector {
  pub fn new(profile: &ConfigProfile) -> Self {
    Self {
      select_video: profile.select_video,
      select_audio: profile.select_audio,
      select_subtitle: profile.select_subtitle,
      select_chapters: profile.select_chapters,
      select_attachments: profile.select_attachments,
      video_languages: parse_language_filter(&profile.video_languages),
      audio_languages: parse_language_filter(&profile.audio_languages),
      subtitle_languages: parse_language_filter(&profile.subtitle_languages),
    }
  }

  /// Whether the profile selects `track`.
  pub fn selects(&self, track: &MkvTrack) -> bool {
    if !is_extractable(track) {
      return false;
    }
    let by_language = |enabled: bool, filter: &Option<HashSet<String>>| {
      enabled
        && filter
          .as_ref()
          .is_none_or(|languages| languages.contains(&track.language.to_lowercase()))
    };
    match track.track_type.as_str() {
      MkvTrack::TYPE_VIDEO => by_language(self.select_video, &self.video_languages),
      MkvTrack::TYPE_AUDIO => by_language(self.select_audio, &self.audio_languages),
      MkvTrack::TYPE_SUBTITLES => by_language(self.select_subtitle, &self.subtitle_languages),
      MkvTrack::TYPE_CHAPTERS => self.select_chapters,
      MkvTrack::TYPE_ATTACHMENT => self.select_attachments,
      _ => false,
    }
  }
}

/// A comma-separated language list as a lowercased set; `None` when it names
/// no language.
fn parse_language_filter(filter: &str) -> Option<HashSet<String>> {
  let languages: HashSet<String> = filter
    .split(',')
    .map(|language| language.trim().to_lowercase())
    .filter(|language| !language.is_empty())
    .collect();
  (!languages.is_empty()).then_some(languages)
}

/// The extension of an attachment, from its codec column (its file
/// extension, else the subtype of its MIME type, both from the file's
/// metadata): the common font and image types get their usual extension,
/// anything else is used lowercased with a character a file name cannot
/// hold replaced ([`sanitize_file_name_part`]), so it cannot add a
/// directory, and nothing gives `bin`.
fn attachment_extension(codec: &str) -> String {
  let lower = codec.to_lowercase();
  match lower.as_str() {
    "jpeg" => "jpg".to_string(),
    "x-truetype-font" => "ttf".to_string(),
    "x-opentype-font" | "font-sfnt" => "otf".to_string(),
    "" => "bin".to_string(),
    _ => sanitize_file_name_part(&lower).into_owned(),
  }
}

/// The extension of an extracted row, without the dot. A video (`V_`), audio
/// (`A_`) or subtitle (`S_`) codec id decides by itself, with `bin` for a
/// codec `mkvextract` writes no known file type for. Otherwise the row type
/// decides: `bin` for video and audio, `srt` for subtitles, `xml` for
/// chapters, the codec column for an attachment ([`attachment_extension`]),
/// `bin` for anything else.
pub fn get_track_extension(codec_id: &str, track_type: &str) -> String {
  if codec_id.starts_with("V_") {
    let extension = match codec_id {
      "V_MPEGH/ISO/HEVC" => "h265",
      "V_MPEG4/ISO/AVC" => "h264",
      "V_MPEG1" | "V_MPEG2" => "mpg",
      "V_MPEG4/ISO/SP" | "V_MPEG4/ISO/ASP" | "V_MPEG4/ISO/AP" | "V_MPEG4/MS/V3" => "mpeg4",
      "V_MS/VFW/FOURCC" => "avi",
      "V_VP8" | "V_VP9" | "V_AV1" => "ivf",
      "V_THEORA" => "ogg",
      "V_PRORES" => "prores",
      "V_FFV1" => "ffv1",
      _ if codec_id.starts_with("V_REAL/") => "rm",
      _ => "bin",
    };
    return extension.to_string();
  }
  if codec_id.starts_with("A_") {
    let extension = match codec_id {
      "A_AC3" | "A_AC3/BSID9" | "A_AC3/BSID10" => "ac3",
      "A_EAC3" => "eac3",
      "A_TRUEHD" => "thd",
      "A_MLP" => "mlp",
      "A_MPEG/L1" => "mp1",
      "A_MPEG/L2" => "mp2",
      "A_MPEG/L3" => "mp3",
      "A_FLAC" => "flac",
      "A_VORBIS" => "ogg",
      "A_OPUS" => "opus",
      "A_WAVPACK4" => "wv",
      "A_TTA1" => "tta",
      "A_ALAC" => "caf",
      _ if codec_id.starts_with("A_PCM/") => "wav",
      _ if codec_id.starts_with("A_AAC") => "aac",
      _ if codec_id.starts_with("A_DTS") => "dts",
      _ if codec_id.starts_with("A_REAL/") => "rm",
      _ => "bin",
    };
    return extension.to_string();
  }
  if codec_id.starts_with("S_") {
    let extension = match codec_id {
      "S_TEXT/UTF8" | "S_TEXT/ASCII" => "srt",
      "S_TEXT/ASS" | "S_ASS" => "ass",
      "S_TEXT/SSA" | "S_SSA" => "ssa",
      "S_TEXT/WEBVTT" => "vtt",
      "S_TEXT/USF" => "usf",
      "S_VOBSUB" => "sub",
      "S_HDMV/PGS" => "sup",
      "S_HDMV/TEXTST" => "textst",
      "S_KATE" => "ogg",
      _ => "bin",
    };
    return extension.to_string();
  }
  match track_type {
    MkvTrack::TYPE_SUBTITLES => "srt".to_string(),
    MkvTrack::TYPE_CHAPTERS => "xml".to_string(),
    MkvTrack::TYPE_ATTACHMENT => attachment_extension(codec_id),
    _ => "bin".to_string(),
  }
}

/// The characters that separate a path's components on this OS: `/`, and on
/// Windows `\\` too. A `\\` is part of a name on Linux and macOS.
const PATH_SEPARATORS: &[char] = if cfg!(windows) { &['/', '\\'] } else { &['/'] };

/// Everything before the last separator ([`PATH_SEPARATORS`]; empty without
/// one), as 0.3.0 split a path on Windows. 0.3.0 took `\\` as a separator on
/// every OS, which split a Linux file name holding one.
pub fn parent_dir(path: &str) -> &str {
  path.rfind(PATH_SEPARATORS).map_or("", |slash| &path[..slash])
}

/// Everything after the last separator (the whole path without one).
pub fn file_name(path: &str) -> &str {
  path.rfind(PATH_SEPARATORS).map_or(path, |slash| &path[slash + 1..])
}

/// The source file's name without its extension, as the OS splits a path
/// (`/media/My Movie.2024.mkv` → `My Movie.2024`); empty for a path without a
/// file name.
pub fn file_name_without_ext(path: &str) -> String {
  Path::new(path)
    .file_stem()
    .map(|stem| stem.to_string_lossy().into_owned())
    .unwrap_or_default()
}

/// The directory an extraction writes into: the card's override, else the
/// global output directory, else the source file's own directory. Empty
/// strings count as unset.
pub fn resolve_output_dir(file: &str, override_dir: Option<&str>, global_dir: Option<&str>) -> String {
  if let Some(dir) = override_dir.filter(|dir| !dir.is_empty()) {
    return dir.to_string();
  }
  if let Some(dir) = global_dir.filter(|dir| !dir.is_empty()) {
    return dir.to_string();
  }
  Path::new(file)
    .parent()
    .map(|dir| dir.to_string_lossy().into_owned())
    .unwrap_or_default()
}

/// The file `name` inside the directory `dir`. Both are normalised without
/// looking at the disk ([`normalize_path`]); `name`'s components follow
/// `dir`'s, and a name's `..` never climbs above `dir`, nor does a leading
/// separator or drive leave it, so a template's subfolders stay inside the
/// chosen folder (0.3.0 resolved `..` across both, so `../x` left it). A
/// relative `dir` stays relative, with its leading `..`. An empty result is
/// `.`.
pub fn join_output_path(dir: &str, name: &str) -> String {
  let mut path = normalize_path(Path::new(dir));
  let floor = path.parts.len();
  for component in Path::new(name).components() {
    match component {
      Component::Normal(part) => path.parts.push(part.to_string_lossy().into_owned()),
      Component::ParentDir => {
        if path.parts.len() > floor {
          path.parts.pop();
        }
      }
      Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
    }
  }
  let joined = path.text();
  if joined.is_empty() { ".".to_string() } else { joined }
}

/// A path as [`normalize_path`] leaves it.
#[derive(Default)]
struct NormalizedPath {
  /// The prefix and the root as written (`C:`, `\\server\share\`, `/`),
  /// empty for a relative path.
  anchor: String,
  /// The components after it; a relative path's leading `..` among them.
  parts: Vec<String>,
}

impl NormalizedPath {
  fn text(&self) -> String {
    let mut text = self.anchor.clone();
    text.push_str(&self.parts.join(MAIN_SEPARATOR_STR));
    text
  }
}

/// 0.3.0's path normalisation: the path's components with `.` and empty
/// ones dropped and each `..` removing the component before it, not past the
/// root; at the start of a relative path a `..` stays, as it names a folder
/// above the working directory. 0.3.0 also stripped a Windows `\\?\` prefix
/// where it could; paths here never carry one, so that step is left out.
fn normalize_path(path: &Path) -> NormalizedPath {
  let mut normalized = NormalizedPath::default();
  for component in path.components() {
    match component {
      Component::Prefix(_) | Component::RootDir => normalized.anchor.push_str(&component.as_os_str().to_string_lossy()),
      Component::CurDir => {}
      Component::ParentDir => match normalized.parts.last() {
        Some(last) if last != ".." => {
          normalized.parts.pop();
        }
        _ if normalized.anchor.is_empty() => normalized.parts.push("..".to_string()),
        _ => {}
      },
      Component::Normal(part) => normalized.parts.push(part.to_string_lossy().into_owned()),
    }
  }
  normalized
}

/// The file name a row is extracted to: the profile's template for its type
/// rendered for the row, a dot, and the row's extension
/// ([`get_track_extension`]). `file_name_without_ext` is the source file's
/// name without its extension.
pub fn build_output_file_name(file_name_without_ext: &str, track: &MkvTrack, profile: &ConfigProfile) -> String {
  let template = pick_template_for_track_type(profile, &track.track_type);
  let base = render_template(template, &TemplateContext::new(file_name_without_ext, track));
  format!("{base}.{}", get_track_extension(&track.codec_id, &track.track_type))
}

/// The arguments of one extraction and every file it writes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractPlan {
  /// The `mkvextract` arguments after the source file ([`build_extract_args`]).
  pub args: Vec<String>,
  /// Every file `mkvextract` writes, in argument order: each track's, the
  /// chapters file and each attachment's, and after a VobSub track's `.sub`
  /// the `.idx` it writes beside it.
  pub outputs: Vec<String>,
}

/// [`build_extract_args`] with the files the extraction writes.
pub fn build_extract_plan(file: &str, output_dir: &str, tracks: &[MkvTrack], profile: &ConfigProfile) -> ExtractPlan {
  let stem = file_name_without_ext(file);
  let mut track_args = Vec::new();
  let mut track_outputs = Vec::new();
  let mut chapters = None;
  let mut attachments = Vec::new();
  let mut attachment_outputs = Vec::new();
  for track in tracks {
    let path = join_output_path(output_dir, &build_output_file_name(&stem, track, profile));
    match track.track_type.as_str() {
      MkvTrack::TYPE_CHAPTERS => {
        chapters.get_or_insert(path);
      }
      MkvTrack::TYPE_ATTACHMENT => {
        attachments.push(format!("{}:{path}", track.id));
        attachment_outputs.push(path);
      }
      _ => {
        track_args.push(format!("{}:{path}", track.id));
        if track.codec_id == VOBSUB_CODEC_ID {
          // `mkvextract` writes the index beside the `.sub`, named as the
          // given path with its extension replaced.
          let index = Path::new(&path).with_extension("idx").to_string_lossy().into_owned();
          track_outputs.push(path);
          track_outputs.push(index);
        } else {
          track_outputs.push(path);
        }
      }
    }
  }
  let mut plan = ExtractPlan::default();
  if !track_args.is_empty() {
    plan.args.push("tracks".to_string());
    plan.args.extend(track_args);
    plan.outputs.extend(track_outputs);
  }
  if let Some(chapters) = chapters {
    plan.args.push("chapters".to_string());
    plan.args.push(chapters.clone());
    plan.outputs.push(chapters);
  }
  if !attachments.is_empty() {
    plan.args.push("attachments".to_string());
    plan.args.extend(attachments);
    plan.outputs.extend(attachment_outputs);
  }
  plan
}

/// The codec id of VobSub subtitles, which `mkvextract` writes as a `.sub`
/// and an `.idx`.
const VOBSUB_CODEC_ID: &str = "S_VOBSUB";

/// The `mkvextract` arguments that follow the source file: `tracks` and an
/// `<id>:<path>` per track, then `chapters` and the path of the first
/// chapters row, then `attachments` and an `<id>:<path>` per attachment. Each
/// mode keeps the order of `tracks` and is left out when it has no row. Every
/// path is [`build_output_file_name`] inside `output_dir`
/// ([`join_output_path`]).
pub fn build_extract_args(file: &str, output_dir: &str, tracks: &[MkvTrack], profile: &ConfigProfile) -> Vec<String> {
  build_extract_plan(file, output_dir, tracks, profile).args
}

/// What two output paths are compared by: the path itself on Linux, and
/// lowercased on Windows and macOS, whose file systems ignore the letter
/// case by default. Two paths with one key are one file.
pub fn output_key(path: &str) -> String {
  if cfg!(any(windows, target_os = "macos")) {
    path.to_lowercase()
  } else {
    path.to_string()
  }
}

/// The index of the first of `outputs` that an earlier one names already
/// ([`output_key`]): two rows of one extraction that would write one file.
pub fn repeated_output(outputs: &[String]) -> Option<usize> {
  let mut seen = HashSet::new();
  outputs.iter().position(|output| !seen.insert(output_key(output)))
}

/// The copyable shell command of an extraction: the `mkvextract` an
/// extraction with this MKVToolNix directory runs
/// ([`mkvextract_executable`](crate::mkvtoolnix::mkvextract_executable)), the
/// source file and [`build_extract_args`], each quoted for the platform's
/// shell ([`shell_quote`]). It has no `--gui-mode`: that is only for reading
/// the progress.
pub fn build_command_string(
  file: &str,
  output_dir: &str,
  mkv_toolnix_path: &str,
  tracks: &[MkvTrack],
  profile: &ConfigProfile,
) -> String {
  let mkvextract = crate::mkvtoolnix::mkvextract_executable(mkv_toolnix_path)
    .to_string_lossy()
    .into_owned();
  [mkvextract, file.to_string()]
    .into_iter()
    .chain(build_extract_args(file, output_dir, tracks, profile))
    .map(|arg| shell_quote(&arg))
    .collect::<Vec<_>>()
    .join(" ")
}

/// Quote one argument of the copyable command for the platform's shell:
/// [`posix_quote`] on Linux and macOS, [`windows_quote`] on Windows.
fn shell_quote(value: &str) -> String {
  if cfg!(target_os = "windows") {
    windows_quote(value)
  } else {
    posix_quote(value)
  }
}

/// A POSIX shell word: as it is when every character is one no shell treats
/// specially (`A-Z a-z 0-9 _ @ % + = : , . / -`), else in single quotes, in
/// which nothing expands (`$`, a backtick, `!`, a glob), with an inner `'`
/// written as `'\''`.
fn posix_quote(value: &str) -> String {
  let plain =
    |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-');
  if !value.is_empty() && value.chars().all(plain) {
    return value.to_string();
  }
  format!("'{}'", value.replace('\'', r"'\''"))
}

/// A Command Prompt (`cmd.exe`) argument: as it is unless it is empty or has
/// whitespace, a quote or one of cmd's operators (`& | < > ^ ( )`), else in
/// double quotes by the rules `CommandLineToArgvW` and the C runtime parse
/// it back with. A `"` becomes `\"`, and backslashes are doubled before a
/// quote and before the closing quote (`C:\dir\` → `"C:\dir\\"`).
///
/// cmd expands `%NAME%` before anything else, inside quotes too, so a value
/// with a `%` is quoted in pieces with each `%` outside the quotes behind a
/// caret: `a%USERNAME%b` → `"a"^%"USERNAME"^%"b"`. A name between two `%`
/// then holds quotes and carets, which no variable has, and cmd's next step
/// takes the caret away; the pieces join back into one argument.
fn windows_quote(value: &str) -> String {
  if value.contains('%') {
    return value.split('%').map(windows_quote_piece).collect::<Vec<_>>().join("^%");
  }
  let special = |c: char| c.is_whitespace() || matches!(c, '"' | '&' | '|' | '<' | '>' | '^' | '(' | ')');
  if !value.is_empty() && !value.chars().any(special) {
    return value.to_string();
  }
  windows_quote_piece(value)
}

/// `value` in double quotes by the rules of [`windows_quote`].
fn windows_quote_piece(value: &str) -> String {
  let mut quoted = String::from('"');
  let mut backslashes = 0;
  for c in value.chars() {
    match c {
      '\\' => backslashes += 1,
      '"' => {
        quoted.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
        quoted.push('"');
        backslashes = 0;
      }
      _ => {
        quoted.extend(std::iter::repeat_n('\\', backslashes));
        quoted.push(c);
        backslashes = 0;
      }
    }
  }
  quoted.extend(std::iter::repeat_n('\\', backslashes * 2));
  quoted.push('"');
  quoted
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::path::MAIN_SEPARATOR;

  fn row(track_type: &str, id: i64, codec_id: &str, language: &str) -> MkvTrack {
    MkvTrack {
      id,
      number: id + 1,
      track_type: track_type.to_string(),
      codec: String::new(),
      codec_id: codec_id.to_string(),
      track_name: String::new(),
      language: language.to_string(),
    }
  }

  fn context() -> TemplateContext<'static> {
    TemplateContext {
      file_name: "Movie",
      track_id: 2,
      track_number: 3,
      language: "eng",
      codec_name: "AVC/H.264",
      track_name: r#"Full: "Forced""#,
    }
  }

  #[test]
  fn renders_placeholders_in_one_pass() {
    let context = context();
    assert_eq!(
      render_template("{file_name}.{track_id}.{language}", &context),
      "Movie.2.eng"
    );
    assert_eq!(
      render_template("{track_number}-{codec_name}-{track_name}", &context),
      "3-AVC_H.264-Full_ _Forced_"
    );
    // A value is not rendered again.
    let braces = TemplateContext {
      file_name: "{track_id}",
      ..context
    };
    assert_eq!(render_template("{file_name}", &braces), "{track_id}");
    assert_eq!(render_template("", &context), "");
    assert_eq!(render_template("映画.{language}", &context), "映画.eng");
  }

  #[test]
  fn escapes_braces_and_keeps_unknown_placeholders() {
    let context = context();
    assert_eq!(render_template("{{file_name}}", &context), "{file_name}");
    assert_eq!(render_template("{{{file_name}}}", &context), "{Movie}");
    assert_eq!(
      render_template("{unknown}.{FILE_NAME}", &context),
      "{unknown}.{FILE_NAME}"
    );
    assert_eq!(render_template("{file_name", &context), "{file_name");
    assert_eq!(render_template("{a{file_name}", &context), "{aMovie");
    assert_eq!(render_template("a}b}", &context), "a}b}");
    assert_eq!(render_template("{}", &context), "{}");
  }

  #[test]
  fn sanitises_the_values_from_the_files_metadata() {
    assert_eq!(sanitize_file_name_part(r#"\/:*?"<>|"#), "_________");
    assert!(matches!(sanitize_file_name_part("plain"), Cow::Borrowed("plain")));
    // The source's own name is used as it is; the language comes from the
    // file's metadata.
    let context = TemplateContext {
      file_name: "a:b",
      language: "../x/y",
      ..context()
    };
    assert_eq!(render_template("{file_name}.{language}", &context), "a:b..._x_y");
  }

  /// Metadata cannot move an output out of the output directory: not an
  /// attachment's MIME subtype or file extension, not a language.
  #[test]
  fn metadata_cannot_add_a_directory_to_an_output() {
    let profile = ConfigProfile::default();
    let mut attachment = row(MkvTrack::TYPE_ATTACHMENT, 1, "../../../../tmp/evil", "");
    attachment.codec = attachment.codec_id.clone();
    let name = build_output_file_name("Movie", &attachment, &profile);
    assert_eq!(name, "Movie.1..._.._.._.._tmp_evil");
    assert_eq!(
      join_output_path("/home/me/out", &name),
      Path::new("/home/me/out").join(&name).to_string_lossy()
    );
    let mut subtitles = row(MkvTrack::TYPE_SUBTITLES, 3, "S_TEXT/UTF8", r"..\..\x");
    subtitles.codec = "SubRip/SRT".into();
    assert_eq!(
      build_output_file_name("Movie", &subtitles, &profile),
      "Movie.3..._.._x.srt"
    );
  }

  #[test]
  fn picks_the_template_of_each_row_type() {
    let profile = ConfigProfile {
      video_template: "v".into(),
      audio_template: "a".into(),
      subtitle_template: "s".into(),
      chapters_template: "c".into(),
      attachments_template: "t".into(),
      ..ConfigProfile::default()
    };
    let picked: Vec<&str> = ["video", "audio", "subtitles", "chapters", "attachment", "buttons", ""]
      .into_iter()
      .map(|track_type| pick_template_for_track_type(&profile, track_type))
      .collect();
    assert_eq!(picked, ["v", "a", "s", "c", "t", "v", "v"]);
  }

  #[test]
  fn selects_rows_by_checkbox_and_language() {
    let video = row(MkvTrack::TYPE_VIDEO, 0, "V_VP9", "und");
    let audio = row(MkvTrack::TYPE_AUDIO, 1, "A_OPUS", "JPN");
    let subtitles = row(MkvTrack::TYPE_SUBTITLES, 2, "S_TEXT/UTF8", "eng");
    let chapters = row(MkvTrack::TYPE_CHAPTERS, 0, "xml", "");
    let attachment = row(MkvTrack::TYPE_ATTACHMENT, 1, "ttf", "");
    let buttons = row(MkvTrack::TYPE_BUTTONS, 3, "B_VOBBTN", "und");

    let default = TrackSelector::new(&ConfigProfile::default());
    assert!(default.selects(&subtitles));
    assert!(!default.selects(&video) && !default.selects(&audio));
    assert!(!default.selects(&chapters) && !default.selects(&attachment));

    let mut profile = ConfigProfile {
      select_video: true,
      select_audio: true,
      select_chapters: true,
      select_attachments: true,
      audio_languages: " jpn , ,".into(),
      subtitle_languages: " , ".into(),
      ..ConfigProfile::default()
    };
    let everything = TrackSelector::new(&profile);
    for selected in [&video, &audio, &subtitles, &chapters, &attachment] {
      assert!(everything.selects(selected), "{selected:?}");
    }
    assert!(!everything.selects(&buttons));

    profile.audio_languages = "eng".into();
    profile.select_subtitle = false;
    let filtered = TrackSelector::new(&profile);
    assert!(!filtered.selects(&audio));
    assert!(!filtered.selects(&subtitles));
  }

  /// The codecs `mkvextract` v101.0 extracts and refuses, as tried with
  /// one-track files made by ffmpeg and listed by `mkvmerge -J`.
  #[test]
  fn only_codecs_mkvextract_writes_are_extractable() {
    let extracted = [
      ("video", "V_MPEG4/ISO/AVC"),
      ("video", "V_MPEGH/ISO/HEVC"),
      ("video", "V_MPEG2"),
      ("video", "V_MS/VFW/FOURCC"),
      ("video", "V_VP8"),
      ("video", "V_VP9"),
      ("video", "V_AV1"),
      ("video", "V_REAL/RV40"),
      ("audio", "A_AAC"),
      ("audio", "A_AAC/MPEG4/LC/SBR"),
      ("audio", "A_AC3"),
      ("audio", "A_AC3/BSID9"),
      ("audio", "A_EAC3"),
      ("audio", "A_DTS"),
      ("audio", "A_ALAC"),
      ("audio", "A_FLAC"),
      ("audio", "A_MPEG/L2"),
      ("audio", "A_MPEG/L3"),
      ("audio", "A_OPUS"),
      ("audio", "A_PCM/INT/LIT"),
      ("audio", "A_PCM/INT/BIG"),
      ("audio", "A_TRUEHD"),
      ("audio", "A_MLP"),
      ("audio", "A_TTA1"),
      ("audio", "A_WAVPACK4"),
      ("subtitles", "S_TEXT/UTF8"),
      ("subtitles", "S_TEXT/ASS"),
      ("subtitles", "S_VOBSUB"),
      ("subtitles", "S_HDMV/PGS"),
      ("chapters", ""),
      ("attachment", "ttf"),
    ];
    let refused = [
      ("video", "V_FFV1"),
      ("video", "V_PRORES"),
      ("video", "V_MPEG4/ISO/ASP"),
      ("video", "V_MJPEG"),
      ("video", "V_QUICKTIME"),
      ("video", "V_UNCOMPRESSED"),
      ("audio", "A_PCM/FLOAT/IEEE"),
      ("audio", "A_MS/ACM"),
      ("subtitles", "S_DVBSUB"),
      ("subtitles", "D_WEBVTT/SUBTITLES"),
      ("buttons", "B_VOBBTN"),
      ("video", ""),
    ];
    for (track_type, codec_id) in extracted {
      assert!(is_extractable(&row(track_type, 1, codec_id, "eng")), "{codec_id}");
    }
    for (track_type, codec_id) in refused {
      assert!(!is_extractable(&row(track_type, 1, codec_id, "eng")), "{codec_id}");
    }
    let everything = TrackSelector::new(&ConfigProfile {
      select_video: true,
      ..ConfigProfile::default()
    });
    assert!(everything.selects(&row("video", 0, "V_MPEG4/ISO/AVC", "und")));
    assert!(!everything.selects(&row("video", 0, "V_FFV1", "und")));
  }

  #[test]
  fn maps_codecs_to_extensions() {
    assert_eq!(get_track_extension("V_MPEGH/ISO/HEVC", "video"), "h265");
    assert_eq!(get_track_extension("V_REAL/RV40", "video"), "rm");
    assert_eq!(get_track_extension("V_UNCOMPRESSED", "video"), "bin");
    assert_eq!(get_track_extension("A_PCM/INT/LIT", "audio"), "wav");
    assert_eq!(get_track_extension("A_AAC/MPEG4/LC", "audio"), "aac");
    assert_eq!(get_track_extension("A_DTS/EXPRESS", "audio"), "dts");
    assert_eq!(get_track_extension("A_REAL/COOK", "audio"), "rm");
    assert_eq!(get_track_extension("A_MS/ACM", "audio"), "bin");
    assert_eq!(get_track_extension("S_HDMV/PGS", "subtitles"), "sup");
    assert_eq!(get_track_extension("S_DVBSUB", "subtitles"), "bin");
    // The codec id wins over the row type.
    assert_eq!(get_track_extension("V_MPEG4/ISO/AVC", "subtitles"), "h264");
    assert_eq!(get_track_extension("", "video"), "bin");
    assert_eq!(get_track_extension("", "subtitles"), "srt");
    assert_eq!(get_track_extension("xml", "chapters"), "xml");
    assert_eq!(get_track_extension("B_VOBBTN", "buttons"), "bin");
  }

  #[test]
  fn names_attachments_by_their_type() {
    let cases = [
      ("JPEG", "jpg"),
      ("x-truetype-font", "ttf"),
      ("x-opentype-font", "otf"),
      ("font-sfnt", "otf"),
      ("PNG", "png"),
      ("", "bin"),
    ];
    for (codec, extension) in cases {
      assert_eq!(get_track_extension(codec, "attachment"), extension, "{codec}");
    }
  }

  #[test]
  fn splits_paths_on_the_separators_of_the_os() {
    if cfg!(windows) {
      assert_eq!(parent_dir(r"C:\media\Movie.mkv"), r"C:\media");
      assert_eq!(file_name(r"C:\media\Movie.mkv"), "Movie.mkv");
      assert_eq!(parent_dir(r"/mixed\dir/x\y"), r"/mixed\dir/x");
    } else {
      // A backslash is part of a name.
      assert_eq!(parent_dir(r"/media/a\b.mkv"), "/media");
      assert_eq!(file_name(r"/media/a\b.mkv"), r"a\b.mkv");
    }
    assert_eq!(parent_dir("/media/a/Movie.mkv"), "/media/a");
    assert_eq!(file_name("/media/a/Movie.mkv"), "Movie.mkv");
    assert_eq!(parent_dir(r"/mixed\dir/x"), r"/mixed\dir");
    assert_eq!(parent_dir("Movie.mkv"), "");
    assert_eq!(file_name("Movie.mkv"), "Movie.mkv");
    assert_eq!(file_name("/media/"), "");
  }

  #[test]
  fn strips_the_extension_of_the_source() {
    assert_eq!(file_name_without_ext("/media/My Movie.2024.mkv"), "My Movie.2024");
    assert_eq!(file_name_without_ext("Movie.MKV"), "Movie");
    assert_eq!(file_name_without_ext("Movie"), "Movie");
    assert_eq!(file_name_without_ext("/"), "");
  }

  #[test]
  fn resolves_the_output_directory() {
    assert_eq!(
      resolve_output_dir("/in/a.mkv", Some("/override"), Some("/global")),
      "/override"
    );
    assert_eq!(resolve_output_dir("/in/a.mkv", Some(""), Some("/global")), "/global");
    assert_eq!(resolve_output_dir("/in/a.mkv", None, Some("")), "/in");
    assert_eq!(resolve_output_dir("/in/a.mkv", None, None), "/in");
    assert_eq!(resolve_output_dir("a.mkv", None, None), "");
    assert_eq!(resolve_output_dir("/", None, None), "");
  }

  #[test]
  fn joins_names_into_the_output_directory() {
    let sep = MAIN_SEPARATOR;
    assert_eq!(join_output_path("out", "a.srt"), format!("out{sep}a.srt"));
    assert_eq!(join_output_path("out/", "a.srt"), format!("out{sep}a.srt"));
    // A leading separator stays inside the directory.
    assert_eq!(join_output_path("out/", "/a.srt"), format!("out{sep}a.srt"));
    // A name's `..` never leaves the directory; the directory's own resolve.
    assert_eq!(join_output_path("out/./x/", "../a.srt"), format!("out{sep}x{sep}a.srt"));
    assert_eq!(
      join_output_path("out/x/..", "subs/../../a.srt"),
      format!("out{sep}a.srt")
    );
    // A relative directory above the working directory stays there.
    assert_eq!(
      join_output_path("../media", "Movie.1.srt"),
      format!("..{sep}media{sep}Movie.1.srt")
    );
    assert_eq!(join_output_path("a/../../b", "x"), format!("..{sep}b{sep}x"));
    assert_eq!(
      join_output_path("out", "subs//a.srt"),
      format!("out{sep}subs{sep}a.srt")
    );
    assert_eq!(join_output_path("", "a.srt"), "a.srt");
    assert_eq!(join_output_path("", "./a.srt"), "a.srt");
    assert_eq!(join_output_path("", ""), ".");
    assert_eq!(join_output_path("out", ".."), "out");
    assert_eq!(join_output_path("", "../a.srt"), "a.srt");
  }

  #[cfg(not(windows))]
  #[test]
  fn joins_absolute_posix_paths() {
    assert_eq!(join_output_path("/out", "a.srt"), "/out/a.srt");
    assert_eq!(join_output_path("/out/", "/a.srt"), "/out/a.srt");
    // `..` stops at the root.
    assert_eq!(join_output_path("/", "../../a.srt"), "/a.srt");
    // A backslash is part of a name on Linux and macOS.
    assert_eq!(join_output_path("/out", r"a\b.srt"), r"/out/a\b.srt");
  }

  #[cfg(windows)]
  #[test]
  fn joins_windows_paths() {
    assert_eq!(join_output_path(r"D:\Out", "a.srt"), r"D:\Out\a.srt");
    assert_eq!(join_output_path(r"D:\Out\", "subs/a.srt"), r"D:\Out\subs\a.srt");
    assert_eq!(join_output_path(r"D:\Out", r"..\..\a.srt"), r"D:\Out\a.srt");
    assert_eq!(join_output_path(r"D:\Out", r"C:\x\a.srt"), r"D:\Out\x\a.srt");
    assert_eq!(join_output_path(r"\\nas\share\Out", "a.srt"), r"\\nas\share\Out\a.srt");
    assert_eq!(join_output_path("/out", "a.srt"), r"\out\a.srt");
  }

  #[test]
  fn builds_names_and_arguments() {
    let profile = ConfigProfile::default();
    let subtitles = row(MkvTrack::TYPE_SUBTITLES, 3, "S_TEXT/UTF8", "eng");
    let chapters = row(MkvTrack::TYPE_CHAPTERS, 0, "xml", "");
    let mut attachment = row(MkvTrack::TYPE_ATTACHMENT, 1, "ttf", "");
    attachment.track_name = "font.ttf".into();
    assert_eq!(build_output_file_name("Movie", &subtitles, &profile), "Movie.3.eng.srt");
    assert_eq!(build_output_file_name("Movie", &chapters, &profile), "Movie.0.xml");

    let sep = MAIN_SEPARATOR;
    let args = build_extract_args(
      "/media/Movie.mkv",
      "out",
      &[attachment, chapters.clone(), subtitles, chapters],
      &profile,
    );
    assert_eq!(
      args,
      [
        "tracks".to_string(),
        format!("3:out{sep}Movie.3.eng.srt"),
        "chapters".to_string(),
        format!("out{sep}Movie.0.xml"),
        "attachments".to_string(),
        format!("1:out{sep}Movie.1.ttf"),
      ]
    );
    assert!(build_extract_args("/media/Movie.mkv", "out", &[], &profile).is_empty());
  }

  #[test]
  fn a_plan_names_every_file_the_extraction_writes() {
    let sep = MAIN_SEPARATOR;
    let profile = ConfigProfile::default();
    let vobsub = row(MkvTrack::TYPE_SUBTITLES, 2, "S_VOBSUB", "eng");
    let srt = row(MkvTrack::TYPE_SUBTITLES, 3, "S_TEXT/UTF8", "eng");
    let chapters = row(MkvTrack::TYPE_CHAPTERS, 0, "xml", "");
    let attachment = row(MkvTrack::TYPE_ATTACHMENT, 1, "ttf", "");
    let rows = [attachment, chapters.clone(), vobsub, srt, chapters];
    let plan = build_extract_plan("/media/Movie.mkv", "out", &rows, &profile);
    assert_eq!(
      plan.args,
      build_extract_args("/media/Movie.mkv", "out", &rows, &profile)
    );
    assert_eq!(
      plan.outputs,
      [
        format!("out{sep}Movie.2.eng.sub"),
        format!("out{sep}Movie.2.eng.idx"),
        format!("out{sep}Movie.3.eng.srt"),
        format!("out{sep}Movie.0.xml"),
        format!("out{sep}Movie.1.ttf"),
      ],
      "the first chapters row only, and the VobSub index"
    );
    assert_eq!(repeated_output(&plan.outputs), None);
  }

  #[test]
  fn a_template_that_names_two_rows_alike_repeats_an_output() {
    let profile = ConfigProfile {
      subtitle_template: "{file_name}".into(),
      ..ConfigProfile::default()
    };
    let english = row(MkvTrack::TYPE_SUBTITLES, 2, "S_TEXT/UTF8", "eng");
    let japanese = row(MkvTrack::TYPE_SUBTITLES, 3, "S_TEXT/UTF8", "jpn");
    let plan = build_extract_plan("/media/Movie.mkv", "out", &[english, japanese], &profile);
    assert_eq!(plan.outputs[1], join_output_path("out", "Movie.srt"));
    assert_eq!(repeated_output(&plan.outputs), Some(1));
    // A `.sub` named like another row's `.idx` clashes with the index.
    let index = ConfigProfile {
      subtitle_template: "{file_name}".into(),
      attachments_template: "{file_name}".into(),
      ..ConfigProfile::default()
    };
    let vobsub = row(MkvTrack::TYPE_SUBTITLES, 2, "S_VOBSUB", "eng");
    let idx = row(MkvTrack::TYPE_ATTACHMENT, 1, "idx", "");
    let plan = build_extract_plan("/media/Movie.mkv", "out", &[vobsub, idx], &index);
    assert_eq!(plan.outputs[2], join_output_path("out", "Movie.idx"));
    assert_eq!(repeated_output(&plan.outputs), Some(2));
  }

  #[test]
  fn outputs_are_compared_as_the_file_system_compares_names() {
    let outputs = ["/out/Movie.srt".to_string(), "/out/MOVIE.srt".to_string()];
    if cfg!(any(windows, target_os = "macos")) {
      assert_eq!(output_key("/Out/A.SRT"), "/out/a.srt");
      assert_eq!(repeated_output(&outputs), Some(1));
    } else {
      assert_eq!(output_key("/Out/A.SRT"), "/Out/A.SRT");
      assert_eq!(repeated_output(&outputs), None);
    }
  }

  #[test]
  fn builds_copyable_commands() {
    let sep = MAIN_SEPARATOR;
    let mut subtitles = row(MkvTrack::TYPE_SUBTITLES, 3, "S_TEXT/UTF8", "eng");
    subtitles.track_name = "Director's cut".into();
    let profile = ConfigProfile {
      subtitle_template: "{file_name} {track_name}".into(),
      ..ConfigProfile::default()
    };
    let command = build_command_string("/in/My Movie.mkv", "out", " /opt/mkv ", &[subtitles], &profile);
    let quote = shell_quote;
    assert_eq!(
      command,
      format!(
        "{} {} tracks {}",
        quote(&format!("/opt/mkv{sep}mkvextract")),
        quote("/in/My Movie.mkv"),
        quote(&format!("3:out{sep}My Movie Director's cut.srt")),
      )
    );
    let nothing = build_command_string("/in/a.mkv", "out", "/opt/mkv", &[], &profile);
    assert_eq!(
      nothing,
      format!("{} {}", quote(&format!("/opt/mkv{sep}mkvextract")), quote("/in/a.mkv"))
    );
  }

  #[test]
  fn posix_words_are_single_quoted_unless_plain() {
    assert_eq!(posix_quote("plain-arg_1.mkv"), "plain-arg_1.mkv");
    assert_eq!(posix_quote("3:/out/Movie.3.eng.srt"), "3:/out/Movie.3.eng.srt");
    assert_eq!(posix_quote(""), "''");
    assert_eq!(posix_quote("a b"), "'a b'");
    assert_eq!(posix_quote(r#"say "hi""#), r#"'say "hi"'"#);
    assert_eq!(posix_quote("it's"), r"'it'\''s'");
    for metacharacters in [
      "Tom&Jerry.mkv",
      "Movie(2020).mkv",
      "a;b.mkv",
      "What?.mkv",
      "[Group]Show*.mkv",
      "#1.mkv",
    ] {
      assert_eq!(posix_quote(metacharacters), format!("'{metacharacters}'"));
    }
    // Nothing expands inside single quotes.
    assert_eq!(
      posix_quote("My $1 $(rm -rf ~) `id`.mkv"),
      "'My $1 $(rm -rf ~) `id`.mkv'"
    );
    assert_eq!(posix_quote(r"C:\dir\"), r"'C:\dir\'");
    assert_eq!(posix_quote("\u{6620}\u{753b}.mkv"), "'\u{6620}\u{753b}.mkv'");
  }

  #[test]
  fn windows_arguments_follow_the_command_line_rules() {
    assert_eq!(windows_quote(r"C:\in\Movie.mkv"), r"C:\in\Movie.mkv");
    assert_eq!(windows_quote(""), r#""""#);
    assert_eq!(windows_quote(r"C:\My Movies\a.mkv"), r#""C:\My Movies\a.mkv""#);
    assert_eq!(windows_quote(r"C:\My Dir\"), r#""C:\My Dir\\""#);
    assert_eq!(windows_quote(r#"3:Director's "Cut""#), r#""3:Director's \"Cut\"""#);
    assert_eq!(windows_quote(r#"a\"b"#), r#""a\\\"b""#);
    assert_eq!(windows_quote("Tom&Jerry.mkv"), r#""Tom&Jerry.mkv""#);
    assert_eq!(windows_quote("it's"), "it's");
  }

  /// A `%` is put outside the quotes behind a caret, so `%NAME%` cannot
  /// name a variable; backslashes before a piece's closing quote are doubled.
  #[test]
  fn windows_arguments_keep_percent_signs_literal() {
    assert_eq!(
      windows_quote(r"C:\Users\%USERNAME%\a.mkv"),
      r#""C:\Users\\"^%"USERNAME"^%"\a.mkv""#
    );
    assert_eq!(windows_quote("100%"), r#""100"^%"""#);
    assert_eq!(windows_quote("%"), r#"""^%"""#);
    assert_eq!(
      windows_quote(r#"3:D:\Out\50% "x".srt"#),
      r#""3:D:\Out\50"^%" \"x\".srt""#
    );
  }
}
