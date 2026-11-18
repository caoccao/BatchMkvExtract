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

//! Group Mode: files of one directory whose tracks line up share one group
//! card, whose track selection applies to all of them (0.3.0's file list).
//!
//! The card's table shows its first file's rows and writes their keys
//! (`"{type}:{id}"`) to every file, so files share a card only when each key
//! names the same track in all of them: the same type, and for a video,
//! audio or subtitle track the same language and codec. 0.3.0 grouped by the
//! number of tracks of each type alone, which put a file whose English and
//! Japanese tracks swap ids on the card of the other, and extracted its
//! Japanese track for the English one checked.

use std::collections::HashMap;

use bme_core::extract_args::parent_dir;
use bme_core::protocol::{MkvTrack, TrackCounts};

/// One entry of the file list: the card of one file, or the group card of two
/// or more alike files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderEntry {
  Single { file: String },
  Group { key: String, files: Vec<String> },
}

/// The group key of a file whose tracks are loaded:
/// `{directory}|v={video}|a={audio}|s={subtitles}|c={chapters}|t={attachments}|{layout}`.
/// The directory is the text before the last separator ([`parent_dir`]);
/// the counts are 0.3.0's key. The layout lists every row's key, sorted,
/// with `={language}/{codec id}` after a video, audio or subtitle track's.
/// Track names are left out: a video track's name is often the episode's
/// title.
pub fn grouping_key(file: &str, tracks: &[MkvTrack]) -> String {
  let counts = TrackCounts::of(tracks);
  let mut layout: Vec<String> = tracks
    .iter()
    .map(|track| match track.track_type.as_str() {
      MkvTrack::TYPE_VIDEO | MkvTrack::TYPE_AUDIO | MkvTrack::TYPE_SUBTITLES => {
        format!("{}={}/{}", track.key(), track.language, track.codec_id)
      }
      _ => track.key(),
    })
    .collect();
  layout.sort();
  format!(
    "{}|v={}|a={}|s={}|c={}|t={}|{}",
    parent_dir(file),
    counts.video,
    counts.audio,
    counts.subtitles,
    counts.chapters,
    counts.attachments,
    layout.join(",")
  )
}

/// The file list's entries for `files`, in their order. Without Group Mode
/// every file is a card of its own. With it, files whose tracks are loaded
/// (they have a key in `group_keys`, [`grouping_key`]) are grouped by it: a
/// key shared by two or more files is a group card where the key first
/// appeared, a key of one file is that file's card. Files still loading, and
/// files whose tracks could not be listed, follow as cards of their own.
pub fn build_render_entries(
  files: &[String],
  group_mode: bool,
  group_keys: &HashMap<String, String>,
) -> Vec<RenderEntry> {
  let single = |file: &String| RenderEntry::Single { file: file.clone() };
  if !group_mode {
    return files.iter().map(single).collect();
  }
  let mut buckets: Vec<(String, Vec<String>)> = Vec::new();
  let mut bucket_of: HashMap<&str, usize> = HashMap::new();
  let mut ungroupable = Vec::new();
  for file in files {
    let Some(key) = group_keys.get(file) else {
      ungroupable.push(file);
      continue;
    };
    match bucket_of.get(key.as_str()) {
      Some(index) => buckets[*index].1.push(file.clone()),
      None => {
        bucket_of.insert(key, buckets.len());
        buckets.push((key.clone(), vec![file.clone()]));
      }
    }
  }
  let mut entries = Vec::with_capacity(files.len());
  for (key, members) in buckets {
    if members.len() >= 2 {
      entries.push(RenderEntry::Group { key, files: members });
    } else {
      entries.extend(members.iter().map(single));
    }
  }
  entries.extend(ungroupable.into_iter().map(single));
  entries
}

#[cfg(test)]
mod tests {
  use super::*;

  fn track(id: i64, track_type: &str, codec_id: &str, language: &str) -> MkvTrack {
    MkvTrack {
      id,
      number: id + 1,
      track_type: track_type.to_string(),
      codec: String::new(),
      codec_id: codec_id.to_string(),
      track_name: format!("name of {id}"),
      language: language.to_string(),
    }
  }

  /// Video, English and Japanese audio, English subtitles, chapters.
  fn episode() -> Vec<MkvTrack> {
    vec![
      track(0, "video", "V_MPEG4/ISO/AVC", "und"),
      track(1, "audio", "A_FLAC", "eng"),
      track(2, "audio", "A_FLAC", "jpn"),
      track(3, "subtitles", "S_TEXT/UTF8", "eng"),
      track(0, "chapters", "", ""),
    ]
  }

  fn files(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_string()).collect()
  }

  fn keys(loaded: &[(&str, Vec<MkvTrack>)]) -> HashMap<String, String> {
    loaded
      .iter()
      .map(|(file, tracks)| (file.to_string(), grouping_key(file, tracks)))
      .collect()
  }

  fn single(file: &str) -> RenderEntry {
    RenderEntry::Single { file: file.to_string() }
  }

  fn group(key: &str, members: &[&str]) -> RenderEntry {
    RenderEntry::Group {
      key: key.to_string(),
      files: files(members),
    }
  }

  #[test]
  fn keys_files_by_directory_counts_and_layout() {
    assert_eq!(
      grouping_key("/m/E01.mkv", &episode()),
      "/m|v=1|a=2|s=1|c=1|t=0|audio:1=eng/A_FLAC,audio:2=jpn/A_FLAC,chapters:0,\
       subtitles:3=eng/S_TEXT/UTF8,video:0=und/V_MPEG4/ISO/AVC"
    );
    let attachment = [track(1, "attachment", "ttf", "")];
    // The directory as the OS splits the path.
    let windows = r"C:\Shows\S1\E01.mkv";
    let expected = if cfg!(windows) { r"C:\Shows\S1" } else { "" };
    assert_eq!(
      grouping_key(windows, &attachment),
      format!("{expected}|v=0|a=0|s=0|c=0|t=1|attachment:1")
    );
    assert_eq!(grouping_key("E01.mkv", &[]), "|v=0|a=0|s=0|c=0|t=0|");
    // The order of the listing and the track names do not count.
    let mut reordered = episode();
    reordered.reverse();
    reordered[0].track_name = "Episode 1".into();
    assert_eq!(
      grouping_key("/m/E01.mkv", &reordered),
      grouping_key("/m/E02.mkv", &episode())
    );
  }

  /// Files with the same counts whose tracks do not line up get cards of
  /// their own: swapped languages, other ids, another codec.
  #[test]
  fn files_whose_tracks_do_not_line_up_are_not_grouped() {
    let mut swapped = episode();
    swapped[1].language = "jpn".into();
    swapped[2].language = "eng".into();
    let mut other_ids = episode();
    other_ids[3].id = 4;
    let mut other_codec = episode();
    other_codec[1].codec_id = "A_AAC".into();
    let loaded = [
      ("/m/E01.mkv", episode()),
      ("/m/E02.mkv", swapped),
      ("/m/E03.mkv", other_ids),
      ("/m/E04.mkv", other_codec),
      ("/m/E05.mkv", episode()),
    ];
    let list: Vec<String> = loaded.iter().map(|(file, _)| file.to_string()).collect();
    let key = grouping_key("/m/E01.mkv", &episode());
    assert_eq!(
      build_render_entries(&list, true, &keys(&loaded)),
      [
        group(&key, &["/m/E01.mkv", "/m/E05.mkv"]),
        single("/m/E02.mkv"),
        single("/m/E03.mkv"),
        single("/m/E04.mkv"),
      ]
    );
  }

  #[test]
  fn without_group_mode_every_file_is_a_card() {
    let loaded = [("/m/E01.mkv", episode()), ("/m/E02.mkv", episode())];
    assert_eq!(
      build_render_entries(&files(&["/m/E01.mkv", "/m/E02.mkv"]), false, &keys(&loaded)),
      [single("/m/E01.mkv"), single("/m/E02.mkv")]
    );
  }

  #[test]
  fn groups_alike_files_where_their_key_first_appears() {
    let list = files(&[
      "/m/E01.mkv",
      "/n/E01.mkv",
      "/m/Extra.mkv",
      "/m/Loading.mkv",
      "/m/E02.mkv",
      "/n/E02.mkv",
      "/m/E03.mkv",
    ]);
    let extra = vec![track(0, "video", "V_MPEG4/ISO/AVC", "und")];
    let loaded = [
      ("/m/E01.mkv", episode()),
      ("/n/E01.mkv", episode()),
      ("/m/Extra.mkv", extra),
      ("/m/E02.mkv", episode()),
      ("/n/E02.mkv", episode()),
      ("/m/E03.mkv", episode()),
    ];
    let (m, n) = (
      grouping_key("/m/E01.mkv", &episode()),
      grouping_key("/n/E01.mkv", &episode()),
    );
    assert_eq!(
      build_render_entries(&list, true, &keys(&loaded)),
      [
        group(&m, &["/m/E01.mkv", "/m/E02.mkv", "/m/E03.mkv"]),
        group(&n, &["/n/E01.mkv", "/n/E02.mkv"]),
        // A key of one file is a card of its own, in the key's place.
        single("/m/Extra.mkv"),
        // Not loaded yet: last.
        single("/m/Loading.mkv"),
      ]
    );
  }

  #[test]
  fn unloaded_files_keep_their_order_after_the_groups() {
    let list = files(&["/m/B.mkv", "/m/A.mkv", "/m/C.mkv"]);
    let loaded = keys(&[("/m/A.mkv", episode())]);
    assert_eq!(
      build_render_entries(&list, true, &loaded),
      [single("/m/A.mkv"), single("/m/B.mkv"), single("/m/C.mkv")]
    );
    assert!(build_render_entries(&[], true, &loaded).is_empty());
  }
}
