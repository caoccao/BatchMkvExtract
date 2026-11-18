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

//! What the app tells the user outside its views: the toast of every
//! finished extraction, the OS notification and topmost window once the
//! queue drains, the paths a drop or a dialog could not use, and the notice
//! of a config file that could not be read as it was.
//!
//! The OS notification and the topmost window are sent once per batch (the
//! queue ran until nothing waited or was extracted), not per file, and only
//! when an extraction of the batch completed or failed.

use std::time::Duration;

use bme_core::config::UnreadableConfig;
use bme_core::extract::{ExtractError, ExtractOutcome};
use gpui_kit::App;

use crate::domain::extract_error::{extract_error_message, warnings_text};
use crate::domain::format::format_hms;
use crate::state::Stores;
use crate::state::config::ConfigEvent;
use crate::state::queue::{QueueBatch, QueueEvent};
use crate::state::ui::{CompletionNotice, NotificationKind};

/// The notice of a drained batch; `None` when it does not notify (every
/// extraction was cancelled). A batch of one completed file names the file
/// and its own time. Any other gives the counts and the time from the first
/// start to the last end.
pub fn completion_notice(batch: &QueueBatch) -> Option<CompletionNotice> {
  if !batch.notifies() {
    return None;
  }
  let title = rust_i18n::t!("extract.extractionComplete").into_owned();
  let failed = batch.failed > 0;
  if let (1, 1, Some((file, elapsed))) = (batch.finished(), batch.completed, batch.last.as_ref()) {
    return Some(CompletionNotice {
      title,
      line: file.clone(),
      detail: rust_i18n::t!("notification.completedIn", elapsed = format_hms(Some(*elapsed))).into_owned(),
      failed,
    });
  }
  Some(CompletionNotice {
    title,
    line: rust_i18n::t!(
      "notification.summary",
      completed = batch.completed,
      failed = batch.failed,
      cancelled = batch.cancelled
    )
    .into_owned(),
    detail: rust_i18n::t!("notification.finishedIn", elapsed = format_hms(Some(batch.elapsed()))).into_owned(),
    failed,
  })
}

/// Shows the toast of every finished extraction (a success with its time, a
/// failure with its error; nothing for a cancelled one), and hands the
/// notice of a drained batch to the UI state, which delivers it through the
/// enabled channels.
pub fn watch(stores: &Stores, cx: &mut App) {
  let ui = stores.ui.clone();
  cx.subscribe(&stores.queue, move |_, event: &QueueEvent, cx| match event {
    QueueEvent::Finished {
      file,
      outcome,
      error,
      warnings,
      elapsed,
    } => {
      let toast = finished_toast(*outcome, error.as_ref(), warnings, *elapsed);
      if let Some((kind, detail)) = toast {
        ui.update(cx, |ui, cx| ui.show_toast(kind, file.clone(), detail, cx));
      }
    }
    QueueEvent::Drained(batch) => {
      if let Some(notice) = completion_notice(batch) {
        ui.update(cx, |ui, cx| ui.show_completion(notice, cx));
      }
    }
    QueueEvent::ItemChanged { .. } | QueueEvent::Tick => {}
  })
  .detach();
  let ui = stores.ui.clone();
  cx.subscribe(&stores.config, move |_, event: &ConfigEvent, cx| {
    if let ConfigEvent::SaveFailed(error) = event {
      let (title, detail) = save_failed_message(error);
      ui.update(cx, |ui, cx| ui.show_toast(NotificationKind::Error, title, detail, cx));
    }
  })
  .detach();
}

/// The toast's title and text for settings that could not be saved.
pub fn save_failed_message(error: &str) -> (String, String) {
  (
    rust_i18n::t!("notification.configSaveFailedTitle").into_owned(),
    rust_i18n::t!("notification.configSaveFailed", detail = error).into_owned(),
  )
}

/// At most this many warnings are shown in a finished extraction's toast.
const TOAST_WARNINGS: usize = 3;

/// The toast of a finished extraction: its time (and warnings) on success,
/// its error on failure, nothing when it was cancelled.
pub fn finished_toast(
  outcome: ExtractOutcome,
  error: Option<&ExtractError>,
  warnings: &[String],
  elapsed: Duration,
) -> Option<(NotificationKind, String)> {
  let elapsed = format_hms(Some(elapsed));
  match outcome {
    ExtractOutcome::Completed if warnings.is_empty() => Some((
      NotificationKind::Success,
      rust_i18n::t!("notification.completedIn", elapsed = elapsed).into_owned(),
    )),
    ExtractOutcome::Completed => Some((
      NotificationKind::Success,
      rust_i18n::t!(
        "notification.completedWithWarnings",
        elapsed = elapsed,
        warnings = warnings_text(warnings, TOAST_WARNINGS)
      )
      .into_owned(),
    )),
    ExtractOutcome::Failed => Some((
      NotificationKind::Error,
      error
        .map(extract_error_message)
        .unwrap_or_else(|| rust_i18n::t!("notification.unknownError").into_owned()),
    )),
    ExtractOutcome::Cancelled => None,
  }
}

/// At most this many of the paths a drop or a dialog could not use are named.
const SKIPPED_NAMED: usize = 3;

/// The snackbar for paths that could not be added: how many, and the first
/// few of them.
pub fn skipped_paths_message(skipped: &[String]) -> String {
  let mut named: Vec<&str> = skipped.iter().take(SKIPPED_NAMED).map(String::as_str).collect();
  if skipped.len() > SKIPPED_NAMED {
    named.push("…");
  }
  rust_i18n::t!(
    "notification.pathsSkipped",
    count = skipped.len(),
    paths = named.join(", ")
  )
  .into_owned()
}

/// The toast's title and text for a config file that did not load as it was.
pub fn unreadable_config_message(problem: &UnreadableConfig) -> (String, String) {
  let detail = match &problem.backup {
    Some(backup) => rust_i18n::t!("notification.configReset", path = backup.display()).into_owned(),
    None => rust_i18n::t!("notification.configResetNoBackup").into_owned(),
  };
  (rust_i18n::t!("notification.configResetTitle").into_owned(), detail)
}

/// Tells the user once, at startup, that the config file could not be read
/// as it was, so settings that went back to their defaults are no surprise.
pub fn report_unreadable_config(stores: &Stores, cx: &mut App) {
  if let Some(problem) = bme_core::config::unreadable_config() {
    let (title, detail) = unreadable_config_message(problem);
    stores
      .ui
      .update(cx, |ui, cx| ui.show_toast(NotificationKind::Error, title, detail, cx));
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::i18n::tests::LOCALE_LOCK;
  use bme_core::config::Language;
  use std::time::SystemTime;

  fn batch(completed: usize, failed: usize, cancelled: usize) -> QueueBatch {
    let started = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    QueueBatch {
      completed,
      failed,
      cancelled,
      started: Some(started),
      ended: Some(started + Duration::from_secs(754)),
      last: Some(("/media/Last.mkv".to_string(), Duration::from_secs(62))),
    }
  }

  #[test]
  fn a_batch_of_one_completed_file_names_the_file() {
    let _locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let notice = completion_notice(&batch(1, 0, 0)).expect("notifies");
    assert_eq!(
      notice,
      CompletionNotice {
        title: "Extraction complete.".into(),
        line: "/media/Last.mkv".into(),
        detail: "Completed in 00:01:02".into(),
        failed: false,
      }
    );
  }

  #[test]
  fn other_batches_show_counts_and_the_batch_time() {
    let _locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let notice = completion_notice(&batch(3, 1, 1)).expect("notifies");
    assert_eq!(notice.title, "Extraction complete.");
    assert_eq!(notice.line, "3 completed, 1 failed, 1 cancelled");
    assert_eq!(notice.detail, "Finished in 00:12:34");
    assert!(notice.failed);
    // One file that failed does not get the single-file success text.
    let notice = completion_notice(&batch(0, 1, 0)).expect("a failure notifies");
    assert_eq!(notice.line, "0 completed, 1 failed, 0 cancelled");
    assert!(notice.failed);
    // One completed and one cancelled file are two files.
    let notice = completion_notice(&batch(1, 0, 1)).expect("notifies");
    assert_eq!(notice.line, "1 completed, 0 failed, 1 cancelled");
    assert!(!notice.failed);
  }

  #[test]
  fn a_batch_cancelled_entirely_sends_nothing() {
    assert_eq!(completion_notice(&batch(0, 0, 2)), None);
    assert_eq!(completion_notice(&QueueBatch::default()), None);
  }

  #[test]
  fn a_finished_extraction_toasts_its_time_warnings_or_error() {
    let _locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let elapsed = Duration::from_secs(62);
    assert_eq!(
      finished_toast(ExtractOutcome::Completed, None, &[], elapsed),
      Some((NotificationKind::Success, "Completed in 00:01:02".to_string()))
    );
    let warnings: Vec<String> = ["one", "two", "three", "four"].map(String::from).to_vec();
    assert_eq!(
      finished_toast(ExtractOutcome::Completed, None, &warnings, elapsed),
      Some((
        NotificationKind::Success,
        "Completed in 00:01:02 with warnings: one\ntwo\nthree\n…".to_string()
      ))
    );
    let error = ExtractError::Exited {
      code: Some(2),
      messages: vec!["The file 'x' could not be opened for writing.".to_string()],
    };
    assert_eq!(
      finished_toast(ExtractOutcome::Failed, Some(&error), &[], elapsed),
      Some((
        NotificationKind::Error,
        "mkvextract failed: The file 'x' could not be opened for writing.".to_string()
      ))
    );
    assert_eq!(
      finished_toast(ExtractOutcome::Failed, None, &[], elapsed),
      Some((NotificationKind::Error, "Unknown error".to_string()))
    );
    assert_eq!(
      finished_toast(ExtractOutcome::Cancelled, None, &warnings, elapsed),
      None
    );
  }

  #[test]
  fn skipped_paths_are_counted_and_the_first_three_named() {
    let _locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let skipped: Vec<String> = ["/a", "/b", "/c", "/d"].map(String::from).to_vec();
    assert_eq!(
      skipped_paths_message(&skipped),
      "4 item(s) could not be read and were skipped: /a, /b, /c, …"
    );
    assert_eq!(
      skipped_paths_message(&skipped[..1]),
      "1 item(s) could not be read and were skipped: /a"
    );
  }

  #[test]
  fn an_unreadable_config_names_the_kept_file() {
    let _locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let kept = UnreadableConfig {
      reason: "it is not valid JSON".into(),
      backup: Some("/home/me/.config/BatchMkvExtract/BatchMkvExtract.json.bad".into()),
    };
    assert_eq!(
      unreadable_config_message(&kept),
      (
        "Settings could not be read".to_string(),
        "Settings that could not be read were reset to their defaults. The original file was kept as \
         /home/me/.config/BatchMkvExtract/BatchMkvExtract.json.bad."
          .to_string()
      )
    );
    let lost = UnreadableConfig { backup: None, ..kept };
    assert_eq!(
      unreadable_config_message(&lost).1,
      "Settings that could not be read were reset to their defaults."
    );
  }

  #[test]
  fn settings_that_could_not_be_saved_say_when_they_are_saved_again() {
    let _locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    assert_eq!(
      save_failed_message("No space left on device (os error 28)"),
      (
        "Settings could not be saved".to_string(),
        "No space left on device (os error 28). They are saved again with the next change and when the app quits."
          .to_string()
      )
    );
  }
}
