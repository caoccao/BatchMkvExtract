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

//! The extract queue's item state machine, driven by `bme_core::extract`'s
//! ordered [`ExtractEvent`]s, and the elapsed / ETA rules of the queue and the cards.
//!
//! `Waiting → Extracting → Completed | Cancelled | Failed`. Every enqueued file
//! gets exactly one `Finished`, in order after its `Started` and `Progress`,
//! so no state has to be guessed:
//!
//! - `Started` makes an item `Extracting`. On a terminal item it starts a new run
//!   and clears the previous run's times, cancel request and error.
//! - `Progress` updates a waiting or extracting item; a terminal item ignores it.
//! - `Finished` is authoritative and always sets the outcome.

use std::time::{Duration, SystemTime};

use bme_core::extract::{ExtractError, ExtractEvent, ExtractOutcome, get_drive_key};

/// A queue item's status. The UI shows [`as_str`](Self::as_str); the i18n key
/// is [`label_key`](Self::label_key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QueueItemStatus {
  Waiting,
  Extracting,
  Completed,
  Cancelled,
  Failed,
}

impl QueueItemStatus {
  /// The PascalCase value (`Waiting`, …), as the queue's wire format spells it.
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Waiting => "Waiting",
      Self::Extracting => "Extracting",
      Self::Completed => "Completed",
      Self::Cancelled => "Cancelled",
      Self::Failed => "Failed",
    }
  }

  /// The i18n key of the label, `queue.status.<lowercase>`.
  pub fn label_key(self) -> &'static str {
    match self {
      Self::Waiting => "queue.status.waiting",
      Self::Extracting => "queue.status.extracting",
      Self::Completed => "queue.status.completed",
      Self::Cancelled => "queue.status.cancelled",
      Self::Failed => "queue.status.failed",
    }
  }

  /// Completed, cancelled or failed.
  pub fn is_terminal(self) -> bool {
    matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
  }

  /// Waiting or extracting: the file is in `bme_core`'s queue.
  pub fn is_active(self) -> bool {
    !self.is_terminal()
  }

  /// Cancelled or failed jobs can be enqueued again.
  pub fn is_resumable(self) -> bool {
    matches!(self, Self::Cancelled | Self::Failed)
  }
}

impl From<ExtractOutcome> for QueueItemStatus {
  fn from(outcome: ExtractOutcome) -> Self {
    match outcome {
      ExtractOutcome::Completed => Self::Completed,
      ExtractOutcome::Cancelled => Self::Cancelled,
      ExtractOutcome::Failed => Self::Failed,
    }
  }
}

/// One row of the extract queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueItem {
  pub file: String,
  /// The per-drive queue the file is extracted in ([`get_drive_key`]).
  pub drive: String,
  pub status: QueueItemStatus,
  /// 0–100.
  pub progress: u32,
  pub extract_started_at: Option<SystemTime>,
  pub extract_ended_at: Option<SystemTime>,
  /// The user asked to cancel; the outcome still comes from `Finished`.
  pub cancel_requested: bool,
  /// Set with [`QueueItemStatus::Failed`].
  pub error: Option<ExtractError>,
  /// What `mkvextract` warned about; a completed extraction with warnings
  /// exited with 1.
  pub warnings: Vec<String>,
}

/// Whether a file may be (re-)enqueued: not while it is waiting or
/// extracting. Both the queue and the extract actions check it.
pub fn can_enqueue(existing: Option<&QueueItem>) -> bool {
  existing.is_none_or(|item| !item.status.is_active())
}

impl QueueItem {
  /// A fresh waiting item, as a file is enqueued. Replaces a terminal item of
  /// an earlier run.
  pub fn waiting(file: &str) -> Self {
    Self {
      file: file.to_string(),
      drive: get_drive_key(file),
      status: QueueItemStatus::Waiting,
      progress: 0,
      extract_started_at: None,
      extract_ended_at: None,
      cancel_requested: false,
      error: None,
      warnings: Vec::new(),
    }
  }

  /// The item for an event about a file the queue does not list yet: a
  /// extracting item for `Started` / `Progress` (started `at`, or `now` for
  /// progress), the outcome for `Finished`.
  pub fn from_event(event: &ExtractEvent, now: SystemTime) -> Self {
    let mut item = Self::waiting(event.file());
    match event {
      ExtractEvent::Started { at, .. } => {
        item.status = QueueItemStatus::Extracting;
        item.extract_started_at = Some(*at);
      }
      ExtractEvent::Progress { percent, .. } => {
        item.status = QueueItemStatus::Extracting;
        item.progress = *percent;
        item.extract_started_at = Some(now);
      }
      ExtractEvent::Finished { .. } => {
        item.apply(event, now);
      }
    }
    item
  }

  /// Apply one event about this item's file. Returns `false` when the event
  /// is ignored (progress on a terminal item).
  pub fn apply(&mut self, event: &ExtractEvent, now: SystemTime) -> bool {
    match event {
      ExtractEvent::Started { at, .. } => {
        if self.status.is_terminal() {
          self.extract_started_at = None;
          self.extract_ended_at = None;
          self.cancel_requested = false;
          self.error = None;
          self.warnings.clear();
          self.progress = 0;
        }
        self.status = QueueItemStatus::Extracting;
        self.extract_started_at = self.extract_started_at.or(Some(*at));
        true
      }
      ExtractEvent::Progress { percent, .. } => {
        if self.status.is_terminal() {
          return false;
        }
        self.status = QueueItemStatus::Extracting;
        self.progress = *percent;
        self.extract_started_at = self.extract_started_at.or(Some(now));
        true
      }
      ExtractEvent::Finished {
        outcome,
        error,
        warnings,
        at,
        ..
      } => {
        self.status = (*outcome).into();
        self.extract_ended_at = self.extract_ended_at.or(Some(*at));
        if *outcome == ExtractOutcome::Completed {
          self.progress = 100;
        }
        self.error = error.clone();
        self.warnings = warnings.clone();
        true
      }
    }
  }

  /// Flag a cancel request. Terminal items are left alone (returns `false`).
  pub fn request_cancel(&mut self) -> bool {
    if self.status.is_terminal() {
      return false;
    }
    self.cancel_requested = true;
    true
  }

  /// The queue table's Elapsed: from the start to the end, or to `now` while
  /// extracting. `None` while waiting, before a start, or for a negative span.
  pub fn elapsed(&self, now: SystemTime) -> Option<Duration> {
    if self.status == QueueItemStatus::Waiting {
      return None;
    }
    let started = self.extract_started_at?;
    self.extract_ended_at.unwrap_or(now).duration_since(started).ok()
  }

  /// A card's elapsed time: only while extracting, zero before a start.
  pub fn card_elapsed(&self, now: SystemTime) -> Option<Duration> {
    if self.status != QueueItemStatus::Extracting {
      return None;
    }
    match self.extract_started_at {
      Some(started) => now.duration_since(started).ok(),
      None => Some(Duration::ZERO),
    }
  }

  /// Remaining time extrapolated from the progress so far: only while extracting
  /// with a start and a progress strictly between 0 and 100.
  pub fn eta(&self, now: SystemTime) -> Option<Duration> {
    if self.status != QueueItemStatus::Extracting || self.progress == 0 || self.progress >= 100 {
      return None;
    }
    let elapsed = now.duration_since(self.extract_started_at?).ok()?;
    let progress = f64::from(self.progress);
    Some(elapsed.mul_f64((100.0 - progress) / progress))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const FILE: &str = "/media/Movie.mkv";

  fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + seconds)
  }

  fn error_text(item: &QueueItem) -> Option<String> {
    item.error.as_ref().map(ToString::to_string)
  }

  fn started(seconds: u64) -> ExtractEvent {
    ExtractEvent::Started {
      file: FILE.to_string(),
      at: at(seconds),
    }
  }

  fn progress(percent: u32) -> ExtractEvent {
    ExtractEvent::Progress {
      file: FILE.to_string(),
      percent,
    }
  }

  fn finished(outcome: ExtractOutcome, error: Option<&str>, seconds: u64) -> ExtractEvent {
    ExtractEvent::Finished {
      file: FILE.to_string(),
      outcome,
      error: error.map(|message| ExtractError::Other(message.to_string())),
      warnings: Vec::new(),
      at: at(seconds),
    }
  }

  #[test]
  fn names_and_classifies_statuses() {
    let all = [
      QueueItemStatus::Waiting,
      QueueItemStatus::Extracting,
      QueueItemStatus::Completed,
      QueueItemStatus::Cancelled,
      QueueItemStatus::Failed,
    ];
    let names: Vec<_> = all.iter().map(|status| status.as_str()).collect();
    assert_eq!(names, ["Waiting", "Extracting", "Completed", "Cancelled", "Failed"]);
    for status in all {
      assert_eq!(
        status.label_key(),
        format!("queue.status.{}", status.as_str().to_lowercase())
      );
      assert_ne!(status.is_terminal(), status.is_active());
    }
    assert!(QueueItemStatus::Extracting.is_active());
    assert_eq!(
      QueueItemStatus::from(ExtractOutcome::Cancelled),
      QueueItemStatus::Cancelled
    );
  }

  #[test]
  fn walks_an_extraction_through_its_states() {
    let mut item = QueueItem::waiting(FILE);
    assert_eq!(item.drive, get_drive_key(FILE));
    assert_eq!((item.status, item.progress), (QueueItemStatus::Waiting, 0));
    assert!(!can_enqueue(Some(&item)));

    assert!(item.apply(&started(10), at(10)));
    assert_eq!(item.status, QueueItemStatus::Extracting);
    assert_eq!(item.extract_started_at, Some(at(10)));
    // A repeated start keeps the first start time.
    item.apply(&started(12), at(12));
    assert_eq!(item.extract_started_at, Some(at(10)));

    for percent in [1, 50, 99] {
      assert!(item.apply(&progress(percent), at(20)));
    }
    assert_eq!(item.progress, 99);
    assert!(!can_enqueue(Some(&item)));

    assert!(item.apply(&finished(ExtractOutcome::Completed, None, 70), at(70)));
    assert_eq!(item.status, QueueItemStatus::Completed);
    assert_eq!((item.progress, item.extract_ended_at), (100, Some(at(70))));
    assert_eq!(item.error, None);
    assert!(can_enqueue(Some(&item)));
    assert!(can_enqueue(None));
  }

  #[test]
  fn keeps_progress_and_records_errors_on_failure_and_cancel() {
    let mut failed = QueueItem::waiting(FILE);
    failed.apply(&started(0), at(0));
    failed.apply(&progress(40), at(5));
    failed.apply(
      &finished(ExtractOutcome::Failed, Some("mkvextract exited with 2"), 9),
      at(9),
    );
    assert_eq!(failed.status, QueueItemStatus::Failed);
    assert_eq!(failed.progress, 40);
    assert_eq!(error_text(&failed).as_deref(), Some("mkvextract exited with 2"));

    // Cancelled while still waiting: no start, only the finish.
    let mut cancelled = QueueItem::waiting(FILE);
    assert!(cancelled.request_cancel());
    assert!(cancelled.cancel_requested);
    cancelled.apply(&finished(ExtractOutcome::Cancelled, None, 3), at(3));
    assert_eq!(cancelled.status, QueueItemStatus::Cancelled);
    assert_eq!((cancelled.progress, cancelled.extract_started_at), (0, None));
    assert_eq!(cancelled.extract_ended_at, Some(at(3)));
    assert!(!cancelled.request_cancel());
  }

  #[test]
  fn protects_terminal_items_from_progress() {
    let mut item = QueueItem::waiting(FILE);
    item.apply(&finished(ExtractOutcome::Completed, None, 5), at(5));
    assert!(!item.apply(&progress(10), at(6)));
    assert_eq!((item.status, item.progress), (QueueItemStatus::Completed, 100));
    // The finish of a run is authoritative, even over a terminal state; the
    // first end time is kept.
    item.apply(&finished(ExtractOutcome::Failed, Some("late"), 8), at(8));
    assert_eq!(item.status, QueueItemStatus::Failed);
    assert_eq!(item.extract_ended_at, Some(at(5)));
  }

  #[test]
  fn a_start_after_a_terminal_state_begins_a_new_run() {
    let mut item = QueueItem::waiting(FILE);
    item.apply(&started(0), at(0));
    item.request_cancel();
    item.apply(&finished(ExtractOutcome::Failed, Some("boom"), 4), at(4));
    item.apply(&started(10), at(10));
    assert_eq!(item.status, QueueItemStatus::Extracting);
    assert_eq!((item.extract_started_at, item.extract_ended_at), (Some(at(10)), None));
    assert_eq!(
      (item.progress, item.cancel_requested, error_text(&item).as_deref()),
      (0, false, None)
    );
  }

  #[test]
  fn creates_items_for_unknown_files() {
    let from_start = QueueItem::from_event(&started(3), at(4));
    assert_eq!(
      (from_start.status, from_start.extract_started_at),
      (QueueItemStatus::Extracting, Some(at(3)))
    );
    let from_progress = QueueItem::from_event(&progress(30), at(4));
    assert_eq!(from_progress.status, QueueItemStatus::Extracting);
    assert_eq!(
      (from_progress.progress, from_progress.extract_started_at),
      (30, Some(at(4)))
    );
    let from_finish = QueueItem::from_event(&finished(ExtractOutcome::Completed, None, 9), at(10));
    assert_eq!(
      (from_finish.status, from_finish.progress),
      (QueueItemStatus::Completed, 100)
    );
    assert_eq!(
      (from_finish.extract_started_at, from_finish.extract_ended_at),
      (None, Some(at(9)))
    );
    let failed = QueueItem::from_event(&finished(ExtractOutcome::Failed, Some("x"), 9), at(10));
    assert_eq!((failed.progress, error_text(&failed).as_deref()), (0, Some("x")));
    assert_eq!(failed.file, FILE);
  }

  #[test]
  fn computes_elapsed_and_eta() {
    let mut item = QueueItem::waiting(FILE);
    assert_eq!(
      (item.elapsed(at(5)), item.card_elapsed(at(5)), item.eta(at(5))),
      (None, None, None)
    );

    item.apply(&started(0), at(0));
    assert_eq!(item.elapsed(at(60)), Some(Duration::from_secs(60)));
    assert_eq!(item.card_elapsed(at(60)), Some(Duration::from_secs(60)));
    // No progress yet: no estimate.
    assert_eq!(item.eta(at(60)), None);
    item.apply(&progress(25), at(60));
    assert_eq!(item.eta(at(60)), Some(Duration::from_secs(180)));
    // A clock that went backwards gives no value rather than a wrong one.
    assert_eq!(item.elapsed(SystemTime::UNIX_EPOCH), None);
    assert_eq!(item.eta(SystemTime::UNIX_EPOCH), None);

    item.apply(&finished(ExtractOutcome::Completed, None, 90), at(90));
    assert_eq!(item.elapsed(at(500)), Some(Duration::from_secs(90)));
    assert_eq!((item.card_elapsed(at(500)), item.eta(at(500))), (None, None));

    // Extracting without a start time (not produced by the event flow).
    let mut unstarted = QueueItem::waiting(FILE);
    unstarted.status = QueueItemStatus::Extracting;
    unstarted.progress = 50;
    assert_eq!(unstarted.card_elapsed(at(1)), Some(Duration::ZERO));
    assert_eq!((unstarted.elapsed(at(1)), unstarted.eta(at(1))), (None, None));
  }

  #[test]
  fn a_completed_extraction_keeps_its_warnings_until_the_next_run() {
    let mut item = QueueItem::waiting(FILE);
    item.apply(&started(0), at(0));
    let warned = ExtractEvent::Finished {
      file: FILE.to_string(),
      outcome: ExtractOutcome::Completed,
      error: None,
      warnings: vec!["track 7 not found".to_string()],
      at: at(5),
    };
    item.apply(&warned, at(5));
    assert_eq!(
      (item.status, item.warnings.as_slice()),
      (QueueItemStatus::Completed, ["track 7 not found".to_string()].as_slice())
    );
    item.apply(&started(9), at(9));
    assert!(item.warnings.is_empty());
  }
}
