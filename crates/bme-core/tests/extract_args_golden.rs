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

//! Golden test of the extraction logic ported from 0.3.0. The expected values
//! of `tests/golden/*.json` were recorded by running 0.3.0's own extraction
//! code, unchanged, under Deno (`extract_args.json` with a stand-in for its
//! path functions that follows their behaviour on a `/` platform), so they
//! pin 0.3.0's file names and arguments: change them only when the behaviour
//! is meant to change. `commands.json` is written by hand per platform,
//! because the copyable command is quoted differently from 0.3.0 on purpose.
//! `format_hms.json` is checked by `bme-app`'s `domain::format`.

use std::collections::HashMap;
use std::path::{MAIN_SEPARATOR, Path};

use bme_core::config::ConfigProfile;
use bme_core::extract::get_drive_key;
use bme_core::extract_args::{
  TemplateContext, TrackSelector, build_command_string, build_extract_args, build_output_file_name, file_name,
  get_track_extension, parent_dir, render_template,
};
use bme_core::protocol::MkvTrack;
use serde_json::Value;

fn golden(name: &str) -> Value {
  let path = Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("tests")
    .join("golden")
    .join(name);
  let text = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
  serde_json::from_str(&text).unwrap_or_else(|err| panic!("{}: {err}", path.display()))
}

fn text(value: &Value) -> &str {
  value.as_str().unwrap_or_else(|| panic!("not a string: {value}"))
}

fn texts(value: &Value) -> Vec<String> {
  value
    .as_array()
    .unwrap_or_else(|| panic!("not an array: {value}"))
    .iter()
    .map(|item| text(item).to_string())
    .collect()
}

fn items(value: &Value) -> &[Value] {
  value.as_array().unwrap_or_else(|| panic!("not an array: {value}"))
}

/// A row in the shape 0.3.0 recorded.
fn track(value: &Value) -> MkvTrack {
  MkvTrack {
    id: value["id"].as_i64().unwrap(),
    number: value["number"].as_i64().unwrap(),
    track_type: text(&value["type"]).to_string(),
    codec: text(&value["codec"]).to_string(),
    codec_id: text(&value["codecId"]).to_string(),
    track_name: text(&value["trackName"]).to_string(),
    language: text(&value["language"]).to_string(),
  }
}

fn tracks(value: &Value) -> Vec<MkvTrack> {
  items(value).iter().map(track).collect()
}

/// A profile as the config file stores it.
fn profile(value: &Value) -> ConfigProfile {
  serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn templates_render_as_recorded() {
  let golden = golden("render_template.json");
  let contexts: Vec<(String, MkvTrack)> = items(&golden["contexts"])
    .iter()
    .map(|context| {
      let row = MkvTrack {
        id: context["trackId"].as_i64().unwrap(),
        number: context["trackNumber"].as_i64().unwrap(),
        track_type: String::new(),
        codec: text(&context["codecName"]).to_string(),
        codec_id: String::new(),
        track_name: text(&context["trackName"]).to_string(),
        language: text(&context["language"]).to_string(),
      };
      (text(&context["fileName"]).to_string(), row)
    })
    .collect();
  let cases = items(&golden["cases"]);
  assert!(cases.len() >= 20, "cases missing");
  for case in cases {
    let template = text(&case["template"]);
    let expected = texts(&case["expected"]);
    let actual: Vec<String> = contexts
      .iter()
      .map(|(file_name, row)| render_template(template, &TemplateContext::new(file_name, row)))
      .collect();
    assert_eq!(actual, expected, "template {template:?}");
  }
}

#[test]
fn extensions_match_the_recorded_table() {
  let golden = golden("track_extension.json");
  let types = texts(&golden["types"]);
  for case in items(&golden["cases"]) {
    let codec_id = text(&case["codecId"]);
    let actual: Vec<String> = types
      .iter()
      .map(|track_type| get_track_extension(codec_id, track_type))
      .collect();
    assert_eq!(actual, texts(&case["expected"]), "codec id {codec_id:?}");
  }
}

#[test]
fn profiles_select_the_recorded_rows() {
  let golden = golden("track_selector.json");
  let rows = tracks(&golden["tracks"]);
  for case in items(&golden["cases"]) {
    let profile = profile(&case["profile"]);
    let selector = TrackSelector::new(&profile);
    let selected: Vec<String> = rows
      .iter()
      .filter(|row| selector.selects(row))
      .map(MkvTrack::key)
      .collect();
    assert_eq!(selected, texts(&case["selected"]), "profile {}", profile.name);
  }
}

#[test]
fn output_file_names_match_the_recorded_ones() {
  let golden = golden("output_file_name.json");
  let rows = tracks(&golden["tracks"]);
  let profiles: Vec<ConfigProfile> = items(&golden["profiles"]).iter().map(profile).collect();
  for case in items(&golden["cases"]) {
    let profile = &profiles[case["profile"].as_u64().unwrap() as usize];
    let stem = text(&case["fileNameWithoutExt"]);
    let actual: Vec<String> = rows
      .iter()
      .map(|row| build_output_file_name(stem, row, profile))
      .collect();
    assert_eq!(
      actual,
      texts(&case["expected"]),
      "profile {}, file {stem:?}",
      profile.name
    );
  }
}

#[test]
fn paths_split_as_recorded() {
  for case in items(&golden("paths.json")["cases"]) {
    let path = text(&case["path"]);
    assert_eq!(parent_dir(path), text(&case["parentDir"]), "parent of {path:?}");
    assert_eq!(file_name(path), text(&case["fileName"]), "file name of {path:?}");
  }
}

/// The drive key groups the queue's rows and picks a file's per-drive queue.
/// 0.3.0 computed it twice: the frontend with a regex over the path (these
/// recorded values), the backend's queue from the path's prefix on Windows
/// and as `default` elsewhere. 1.0.0 has only the backend's key, which names
/// every spelling of a drive or share alike, so these paths differ from the
/// recorded values on purpose.
const WINDOWS_DRIVE_KEY_DIFFERENCES: [(&str, &str); 4] = [
  (r"\\server/share/Movie.mkv", r"\\SERVER\SHARE"),
  ("//server/share/Movie.mkv", r"\\SERVER\SHARE"),
  (r"\\?\C:\Media\Movie.mkv", "C:"),
  (r"\\?\UNC\server\share\Movie.mkv", r"\\SERVER\SHARE"),
];

#[test]
fn drive_keys_match_the_recorded_ones_where_intended() {
  let cases = items(&golden("paths.json")["cases"]).to_vec();
  let differences: HashMap<&str, &str> = WINDOWS_DRIVE_KEY_DIFFERENCES.into_iter().collect();
  for path in differences.keys() {
    assert!(
      cases.iter().any(|case| text(&case["path"]) == *path),
      "{path:?} is not a recorded path"
    );
  }
  for case in &cases {
    let path = text(&case["path"]);
    let expected = if cfg!(windows) {
      differences
        .get(path)
        .copied()
        .unwrap_or_else(|| text(&case["driveKey"]))
    } else {
      // Linux and macOS have no drives: one queue, as in 0.3.0's backend.
      "default"
    };
    assert_eq!(get_drive_key(path), expected, "drive of {path:?}");
  }
}

#[test]
fn extract_arguments_match_the_recorded_ones() {
  // Recorded with `/` as the separator; Windows has its own hand-written
  // cases in commands.json.
  if MAIN_SEPARATOR != '/' {
    return;
  }
  let cases = items(&golden("extract_args.json")["cases"]).to_vec();
  assert!(cases.len() >= 10, "cases missing");
  for case in cases {
    let args = build_extract_args(
      text(&case["file"]),
      text(&case["outputDir"]),
      &tracks(&case["tracks"]),
      &profile(&case["profile"]),
    );
    assert_eq!(args, texts(&case["expected"]), "{}", text(&case["description"]));
  }
}

#[test]
fn commands_match_the_hand_written_ones() {
  let golden = golden("commands.json");
  let rows = tracks(&golden_rows());
  let cases = items(&golden[if cfg!(windows) { "windows" } else { "posix" }]).to_vec();
  assert!(!cases.is_empty());
  let profile = ConfigProfile::default();
  for case in cases {
    let selected: Vec<MkvTrack> = texts(&case["tracks"])
      .iter()
      .map(|key| {
        rows
          .iter()
          .find(|row| row.key() == *key)
          .unwrap_or_else(|| panic!("no row {key}"))
          .clone()
      })
      .collect();
    let file = text(&case["file"]);
    let output_dir = text(&case["outputDir"]);
    assert_eq!(
      build_extract_args(file, output_dir, &selected, &profile),
      texts(&case["args"]),
      "args of {file:?}"
    );
    assert_eq!(
      build_command_string(file, output_dir, text(&case["mkvToolNixPath"]), &selected, &profile),
      text(&case["command"]),
      "command of {file:?}"
    );
  }
}

/// The rows the command cases name by key.
fn golden_rows() -> Value {
  golden("track_selector.json")["tracks"].clone()
}
