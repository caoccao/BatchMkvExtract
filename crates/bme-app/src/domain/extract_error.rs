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

//! What a failed extraction tells the user: each [`ExtractError`] in the
//! app's language, with `mkvextract`'s own error text when it printed one.

use bme_core::extract::ExtractError;

/// The message of a failed extraction, translated now.
pub fn extract_error_message(error: &ExtractError) -> String {
  match error {
    ExtractError::SourceMissing { path } => rust_i18n::t!("extract.error.sourceMissing", path = path).into_owned(),
    ExtractError::NotAvailable { path, detail } => rust_i18n::t!(
      "extract.error.mkvextractNotAvailable",
      detail = format!("{path}: {detail}")
    )
    .into_owned(),
    ExtractError::Exited { code, messages } if messages.is_empty() => match code {
      Some(code) => rust_i18n::t!("extract.error.mkvextractExitCode", code = code).into_owned(),
      None => rust_i18n::t!("extract.error.mkvextractTerminated").into_owned(),
    },
    ExtractError::Exited { messages, .. } => {
      rust_i18n::t!("extract.error.mkvextractFailed", detail = messages.join("\n")).into_owned()
    }
    ExtractError::Other(message) => message.clone(),
  }
}

/// The text of a completed extraction's warnings: at most `limit`, one per line,
/// with `…` when there were more.
pub fn warnings_text(warnings: &[String], limit: usize) -> String {
  let mut lines: Vec<&str> = warnings.iter().take(limit).map(String::as_str).collect();
  if warnings.len() > limit {
    lines.push("…");
  }
  lines.join("\n")
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::i18n::tests::LOCALE_LOCK;
  use bme_core::config::Language;

  #[test]
  fn every_failure_is_worded_in_the_app_language() {
    let _locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let exited = |code, messages: &[&str]| ExtractError::Exited {
      code,
      messages: messages.iter().map(|message| message.to_string()).collect(),
    };
    assert_eq!(
      extract_error_message(&ExtractError::SourceMissing {
        path: "/m/a.mkv".into()
      }),
      "The source file /m/a.mkv cannot be found."
    );
    assert_eq!(
      extract_error_message(&ExtractError::NotAvailable {
        path: "/usr/bin/mkvextract".into(),
        detail: "No such file or directory (os error 2)".into(),
      }),
      "mkvextract is not available. Please check the MKVToolNix path in Settings. \
       /usr/bin/mkvextract: No such file or directory (os error 2)"
    );
    assert_eq!(
      extract_error_message(&exited(Some(2), &["The file 'x' could not be opened for writing."])),
      "mkvextract failed: The file 'x' could not be opened for writing."
    );
    assert_eq!(
      extract_error_message(&exited(Some(2), &[])),
      "mkvextract exited with code 2."
    );
    assert_eq!(extract_error_message(&exited(None, &[])), "mkvextract was terminated.");
    assert_eq!(extract_error_message(&ExtractError::Other("as is".into())), "as is");
    crate::i18n::set_language(Language::De);
    assert_eq!(
      extract_error_message(&exited(None, &[])),
      "mkvextract wurde vorzeitig beendet."
    );
    // 0.3.0's own key, with its placeholder converted to rust-i18n's.
    assert_eq!(
      extract_error_message(&exited(Some(2), &["one.", "two."])),
      "mkvextract fehlgeschlagen: one.\ntwo."
    );
    crate::i18n::set_language(Language::EnUS);
  }

  #[test]
  fn warnings_are_listed_up_to_the_limit() {
    let warnings: Vec<String> = ["one", "two", "three"].map(String::from).to_vec();
    assert_eq!(warnings_text(&warnings, 3), "one\ntwo\nthree");
    assert_eq!(warnings_text(&warnings, 2), "one\ntwo\n…");
    assert_eq!(warnings_text(&[], 2), "");
  }
}
