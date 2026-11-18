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

//! Results the core hands to the application: a file's tracks, the reason
//! they could not be listed, the tool detection status, and the outcome of
//! an update check.

/// One row of a file's track table, as `mkvmerge -J` reports it: a track,
/// the `chapters` row of a file with chapter entries, or an attachment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MkvTrack {
  /// The track or attachment id `mkvextract` takes; 0 for chapters.
  pub id: i64,
  /// The track number; 0 for chapters and attachments.
  pub number: i64,
  /// `video`, `audio`, `subtitles`, `buttons`, `chapters` or `attachment`.
  pub track_type: String,
  pub codec: String,
  pub codec_id: String,
  /// The track's name; an attachment's file name.
  pub track_name: String,
  /// The track's language (`und` when `mkvmerge` reports none); empty for
  /// chapters and attachments.
  pub language: String,
}

impl MkvTrack {
  pub const TYPE_VIDEO: &'static str = "video";
  pub const TYPE_AUDIO: &'static str = "audio";
  pub const TYPE_SUBTITLES: &'static str = "subtitles";
  pub const TYPE_BUTTONS: &'static str = "buttons";
  pub const TYPE_CHAPTERS: &'static str = "chapters";
  pub const TYPE_ATTACHMENT: &'static str = "attachment";

  /// The row's key in a file's selection, `"{type}:{id}"` (0.3.0's key):
  /// the type keeps a track, the chapters row and an attachment
  /// with the same id apart.
  pub fn key(&self) -> String {
    format!("{}:{}", self.track_type, self.id)
  }
}

/// How many rows of each type a file has: what a card's summary shows and
/// what Group Mode compares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TrackCounts {
  pub video: usize,
  pub audio: usize,
  pub subtitles: usize,
  pub chapters: usize,
  pub attachments: usize,
}

impl TrackCounts {
  /// The counts of `tracks`. Buttons and rows of an unknown type are not
  /// counted.
  pub fn of(tracks: &[MkvTrack]) -> Self {
    let mut counts = Self::default();
    for track in tracks {
      match track.track_type.as_str() {
        MkvTrack::TYPE_VIDEO => counts.video += 1,
        MkvTrack::TYPE_AUDIO => counts.audio += 1,
        MkvTrack::TYPE_SUBTITLES => counts.subtitles += 1,
        MkvTrack::TYPE_CHAPTERS => counts.chapters += 1,
        MkvTrack::TYPE_ATTACHMENT => counts.attachments += 1,
        _ => {}
      }
    }
    counts
  }
}

/// Why a file's tracks could not be listed. The application keeps it per
/// file and picks the message from the variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrackListError {
  /// `mkvmerge` could not be started from `path`; `detail` is the OS error.
  NotAvailable { path: String, detail: String },
  /// `mkvmerge` exited with an error; `messages` are the errors it reported
  /// (its JSON `errors`, else what it wrote to stderr).
  Failed { messages: Vec<String> },
  /// `mkvmerge`'s output was not the JSON it documents.
  Parse { detail: String },
  /// `mkvmerge` was still running after `seconds` and was stopped.
  TimedOut { seconds: u64 },
  /// The listing was cancelled (the file was removed); never shown.
  Cancelled,
  /// Any other failure (the file is missing, …), shown as it is.
  Other(String),
}

impl std::fmt::Display for TrackListError {
  /// An English description, for logs.
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::NotAvailable { path, detail } => write!(f, "mkvmerge could not be started from {path}: {detail}"),
      Self::Failed { messages } if messages.is_empty() => write!(f, "mkvmerge failed"),
      Self::Failed { messages } => write!(f, "mkvmerge failed: {}", messages.join(" ")),
      Self::Parse { detail } => write!(f, "the output of mkvmerge could not be parsed: {detail}"),
      Self::TimedOut { seconds } => write!(f, "mkvmerge did not finish within {seconds} seconds"),
      Self::Cancelled => f.write_str("the listing was cancelled"),
      Self::Other(message) => f.write_str(message),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MkvToolNixStatus {
  pub found: bool,
  pub mkv_toolnix_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BetterMediaInfoStatus {
  pub found: bool,
  pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheckResult {
  pub has_update: bool,
  pub latest_version: Option<String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  fn row(track_type: &str, id: i64) -> MkvTrack {
    MkvTrack {
      id,
      number: 0,
      track_type: track_type.into(),
      codec: String::new(),
      codec_id: String::new(),
      track_name: String::new(),
      language: String::new(),
    }
  }

  #[test]
  fn keys_rows_by_type_and_id() {
    assert_eq!(row(MkvTrack::TYPE_SUBTITLES, 3).key(), "subtitles:3");
    assert_eq!(row(MkvTrack::TYPE_CHAPTERS, 0).key(), "chapters:0");
    assert_ne!(
      row(MkvTrack::TYPE_ATTACHMENT, 1).key(),
      row(MkvTrack::TYPE_AUDIO, 1).key()
    );
  }

  #[test]
  fn counts_rows_per_type() {
    assert_eq!(TrackCounts::of(&[]), TrackCounts::default());
    let tracks = [
      row(MkvTrack::TYPE_VIDEO, 0),
      row(MkvTrack::TYPE_AUDIO, 1),
      row(MkvTrack::TYPE_AUDIO, 2),
      row(MkvTrack::TYPE_SUBTITLES, 3),
      row(MkvTrack::TYPE_BUTTONS, 4),
      row("images", 5),
      row(MkvTrack::TYPE_CHAPTERS, 0),
      row(MkvTrack::TYPE_ATTACHMENT, 1),
      row(MkvTrack::TYPE_ATTACHMENT, 2),
      row(MkvTrack::TYPE_ATTACHMENT, 3),
    ];
    assert_eq!(
      TrackCounts::of(&tracks),
      TrackCounts {
        video: 1,
        audio: 2,
        subtitles: 1,
        chapters: 1,
        attachments: 3,
      }
    );
  }

  #[test]
  fn track_list_errors_describe_themselves_for_logs() {
    let not_available = TrackListError::NotAvailable {
      path: "/opt/mkvmerge".into(),
      detail: "not found".into(),
    };
    assert_eq!(
      not_available.to_string(),
      "mkvmerge could not be started from /opt/mkvmerge: not found"
    );
    assert_eq!(
      TrackListError::Failed { messages: Vec::new() }.to_string(),
      "mkvmerge failed"
    );
    let failed = TrackListError::Failed {
      messages: vec!["one.".into(), "two.".into()],
    };
    assert_eq!(failed.to_string(), "mkvmerge failed: one. two.");
    assert_eq!(
      TrackListError::Parse { detail: "EOF".into() }.to_string(),
      "the output of mkvmerge could not be parsed: EOF"
    );
    assert_eq!(TrackListError::Other("gone".into()).to_string(), "gone");
    assert_eq!(
      TrackListError::TimedOut { seconds: 60 }.to_string(),
      "mkvmerge did not finish within 60 seconds"
    );
    assert_eq!(TrackListError::Cancelled.to_string(), "the listing was cancelled");
  }
}
