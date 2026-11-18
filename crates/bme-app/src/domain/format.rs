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

//! Duration, clock-time and track-count labels.

use std::time::{Duration, SystemTime};

use bme_core::protocol::{MkvTrack, TrackCounts};

/// The emoji the track table and the card summary show for a row type
/// (0.3.0's emoji); `❓` for any other type.
pub fn track_type_emoji(track_type: &str) -> &'static str {
  match track_type {
    MkvTrack::TYPE_VIDEO => "🎞️",
    MkvTrack::TYPE_AUDIO => "🔊",
    MkvTrack::TYPE_SUBTITLES => "💬",
    MkvTrack::TYPE_CHAPTERS => "📑",
    MkvTrack::TYPE_ATTACHMENT => "📎",
    MkvTrack::TYPE_BUTTONS => "🔘",
    "images" => "🖼️",
    _ => "❓",
  }
}

/// A file's track counts as a card summary shows them: count and emoji per
/// type that has any, e.g. `1 🎞️ 2 🔊 3 💬`; empty without tracks.
pub fn format_track_counts(counts: &TrackCounts) -> String {
  [
    (counts.video, track_type_emoji(MkvTrack::TYPE_VIDEO)),
    (counts.audio, track_type_emoji(MkvTrack::TYPE_AUDIO)),
    (counts.subtitles, track_type_emoji(MkvTrack::TYPE_SUBTITLES)),
    (counts.chapters, track_type_emoji(MkvTrack::TYPE_CHAPTERS)),
    (counts.attachments, track_type_emoji(MkvTrack::TYPE_ATTACHMENT)),
  ]
  .into_iter()
  .filter(|(count, _)| *count > 0)
  .map(|(count, emoji)| format!("{count} {emoji}"))
  .collect::<Vec<_>>()
  .join(" ")
}

/// A queue row's Start or End: the local time of day as `HH:MM:SS`; `--:--:--`
/// without one.
pub fn format_clock(time: Option<SystemTime>) -> String {
  format_clock_in(time, &chrono::Local)
}

/// [`format_clock`] in the time zone `tz`.
pub fn format_clock_in<Tz: chrono::TimeZone>(time: Option<SystemTime>, tz: &Tz) -> String
where
  Tz::Offset: std::fmt::Display,
{
  match time {
    Some(time) => chrono::DateTime::<chrono::Utc>::from(time)
      .with_timezone(tz)
      .format("%H:%M:%S")
      .to_string(),
    None => "--:--:--".to_string(),
  }
}

/// `HH:MM:SS` with whole seconds rounded down (hours keep growing past 99);
/// `--:--:--` when there is no duration (not started, or a negative span).
pub fn format_hms(duration: Option<Duration>) -> String {
  let Some(duration) = duration else {
    return "--:--:--".to_string();
  };
  let total = duration.as_secs();
  format!("{:02}:{:02}:{:02}", total / 3600, (total % 3600) / 60, total % 60)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn formats_track_counts_like_the_card_summary() {
    assert_eq!(format_track_counts(&TrackCounts::default()), "");
    let counts = TrackCounts {
      video: 1,
      audio: 2,
      subtitles: 0,
      chapters: 1,
      attachments: 3,
    };
    assert_eq!(format_track_counts(&counts), "1 🎞️ 2 🔊 1 📑 3 📎");
  }

  #[test]
  fn every_row_type_has_an_emoji() {
    let emojis: Vec<&str> = [
      "video",
      "audio",
      "subtitles",
      "buttons",
      "chapters",
      "attachment",
      "images",
      "",
      "unknown",
    ]
    .into_iter()
    .map(track_type_emoji)
    .collect();
    assert_eq!(emojis, ["🎞️", "🔊", "💬", "🔘", "📑", "📎", "🖼️", "❓", "❓"]);
  }

  #[test]
  fn shows_clock_times_in_the_time_zone() {
    let tokyo = chrono::FixedOffset::east_opt(9 * 3600).unwrap();
    // 2026-05-24T07:27:25Z.
    let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_779_607_645);
    assert_eq!(format_clock_in(Some(time), &chrono::Utc), "07:27:25");
    assert_eq!(format_clock_in(Some(time), &tokyo), "16:27:25");
    assert_eq!(format_clock_in(None, &tokyo), "--:--:--");
    assert_eq!(format_clock(None), "--:--:--");
    assert_eq!(format_clock(Some(time)).len(), "HH:MM:SS".len());
  }

  #[test]
  fn formats_durations_as_hours_minutes_seconds() {
    assert_eq!(format_hms(None), "--:--:--");
    assert_eq!(format_hms(Some(Duration::ZERO)), "00:00:00");
    assert_eq!(format_hms(Some(Duration::from_millis(3_661_999))), "01:01:01");
    assert_eq!(format_hms(Some(Duration::from_secs(100 * 3600 + 59))), "100:00:59");
  }

  /// 0.3.0's duration format took milliseconds; a negative span had no
  /// duration.
  #[test]
  fn formats_durations_like_the_recorded_format_hms() {
    let golden: serde_json::Value =
      serde_json::from_str(include_str!("../../../bme-core/tests/golden/format_hms.json")).unwrap();
    let cases = golden["cases"].as_array().unwrap();
    assert!(cases.len() >= 10, "cases missing");
    for case in cases {
      let ms = case["ms"].as_f64().unwrap();
      let duration = (ms >= 0.0).then(|| Duration::from_secs_f64(ms / 1000.0));
      assert_eq!(format_hms(duration), case["expected"].as_str().unwrap(), "{ms} ms");
    }
  }
}
