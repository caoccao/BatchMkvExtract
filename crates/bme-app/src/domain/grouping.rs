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

//! Group Mode: files of one directory with the same number of tracks of each
//! type share one group card, whose track selection applies to all of them
//! (0.3.0's `FileList.tsx`).

use std::collections::HashMap;

use bme_core::extract_args::parent_dir;
use bme_core::protocol::TrackCounts;

/// One entry of the file list: the card of one file, or the group card of two
/// or more alike files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderEntry {
  Single { file: String },
  Group { key: String, files: Vec<String> },
}

/// The group key of a file whose tracks are loaded:
/// `{directory}|v={video}|a={audio}|s={subtitles}|c={chapters}|t={attachments}`.
/// The directory is the text before the last `/` or `\` ([`parent_dir`]), so
/// the keys equal 0.3.0's.
pub fn grouping_key(file: &str, counts: &TrackCounts) -> String {
  format!(
    "{}|v={}|a={}|s={}|c={}|t={}",
    parent_dir(file),
    counts.video,
    counts.audio,
    counts.subtitles,
    counts.chapters,
    counts.attachments
  )
}

/// The file list's entries for `files`, in their order. Without Group Mode
/// every file is a card of its own. With it, files whose tracks are loaded
/// (they have counts in `track_counts`) are grouped by [`grouping_key`]: a key
/// shared by two or more files is a group card where the key first appeared,
/// a key of one file is that file's card. Files still loading, and files whose
/// tracks could not be listed, follow as cards of their own.
pub fn build_render_entries(
  files: &[String],
  group_mode: bool,
  track_counts: &HashMap<String, TrackCounts>,
) -> Vec<RenderEntry> {
  let single = |file: &String| RenderEntry::Single { file: file.clone() };
  if !group_mode {
    return files.iter().map(single).collect();
  }
  let mut buckets: Vec<(String, Vec<String>)> = Vec::new();
  let mut bucket_of: HashMap<String, usize> = HashMap::new();
  let mut ungroupable = Vec::new();
  for file in files {
    let Some(counts) = track_counts.get(file) else {
      ungroupable.push(file);
      continue;
    };
    let key = grouping_key(file, counts);
    match bucket_of.get(&key) {
      Some(index) => buckets[*index].1.push(file.clone()),
      None => {
        bucket_of.insert(key.clone(), buckets.len());
        buckets.push((key, vec![file.clone()]));
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

  fn counts(video: usize, audio: usize, subtitles: usize) -> TrackCounts {
    TrackCounts {
      video,
      audio,
      subtitles,
      chapters: 1,
      attachments: 0,
    }
  }

  fn files(paths: &[&str]) -> Vec<String> {
    paths.iter().map(|path| (*path).to_string()).collect()
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
  fn keys_files_by_directory_and_counts() {
    assert_eq!(grouping_key("/m/E01.mkv", &counts(1, 2, 3)), "/m|v=1|a=2|s=3|c=1|t=0");
    assert_eq!(
      grouping_key(r"C:\Shows\S1\E01.mkv", &counts(1, 1, 0)),
      r"C:\Shows\S1|v=1|a=1|s=0|c=1|t=0"
    );
    // Either separator ends the directory; a bare name has none.
    assert_eq!(
      grouping_key(r"C:/Shows\E01.mkv", &TrackCounts::default()),
      "C:/Shows|v=0|a=0|s=0|c=0|t=0"
    );
    assert_eq!(grouping_key("E01.mkv", &TrackCounts::default()), "|v=0|a=0|s=0|c=0|t=0");
  }

  #[test]
  fn without_group_mode_every_file_is_a_card() {
    let list = files(&["/m/E01.mkv", "/m/E02.mkv"]);
    let loaded = HashMap::from([(list[0].clone(), counts(1, 2, 3)), (list[1].clone(), counts(1, 2, 3))]);
    assert_eq!(
      build_render_entries(&list, false, &loaded),
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
    let loaded = HashMap::from([
      ("/m/E01.mkv".to_string(), counts(1, 2, 3)),
      ("/n/E01.mkv".to_string(), counts(1, 2, 3)),
      ("/m/Extra.mkv".to_string(), counts(1, 1, 0)),
      ("/m/E02.mkv".to_string(), counts(1, 2, 3)),
      ("/n/E02.mkv".to_string(), counts(1, 2, 3)),
      ("/m/E03.mkv".to_string(), counts(1, 2, 3)),
    ]);
    assert_eq!(
      build_render_entries(&list, true, &loaded),
      [
        group("/m|v=1|a=2|s=3|c=1|t=0", &["/m/E01.mkv", "/m/E02.mkv", "/m/E03.mkv"]),
        group("/n|v=1|a=2|s=3|c=1|t=0", &["/n/E01.mkv", "/n/E02.mkv"]),
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
    let loaded = HashMap::from([("/m/A.mkv".to_string(), counts(1, 1, 1))]);
    assert_eq!(
      build_render_entries(&list, true, &loaded),
      [single("/m/A.mkv"), single("/m/B.mkv"), single("/m/C.mkv")]
    );
    assert!(build_render_entries(&[], true, &loaded).is_empty());
  }
}
