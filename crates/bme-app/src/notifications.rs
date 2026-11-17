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
//! finished extraction, the paths a drop or a dialog could not use, and the
//! notice of a config file that could not be read as it was.

use std::time::Duration;

use bme_core::config::UnreadableConfig;
use bme_core::extract::{ExtractError, ExtractOutcome};
use gpui_kit::App;

use crate::domain::extract_error::{extract_error_message, warnings_text};
use crate::domain::format::format_hms;
use crate::state::Stores;
use crate::state::queue::QueueEvent;
use crate::state::ui::NotificationKind;

/// Shows the toast of every finished extraction (a success with its time, a
/// failure with its error; nothing for a cancelled one).
pub fn watch(stores: &Stores, cx: &mut App) {
  let ui = stores.ui.clone();
  cx.subscribe(&stores.queue, move |_, event: &QueueEvent, cx| {
    if let QueueEvent::Finished {
      file,
      outcome,
      error,
      warnings,
      elapsed,
    } = event
      && let Some((kind, detail)) = finished_toast(*outcome, error.as_ref(), warnings, *elapsed)
    {
      ui.update(cx, |ui, cx| ui.show_toast(kind, file.clone(), detail, cx));
    }
  })
  .detach();
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
}
