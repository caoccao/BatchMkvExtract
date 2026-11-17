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

//! What a file whose tracks could not be listed shows in its table: each
//! [`TrackListError`] in the app's language, through 0.3.0's keys, with what
//! `mkvmerge` reported.

use bme_core::protocol::TrackListError;

/// The message of a failed track listing, translated now.
pub fn track_error_message(error: &TrackListError) -> String {
  match error {
    TrackListError::NotAvailable { path, detail } => rust_i18n::t!(
      "extract.error.mkvmergeNotAvailable",
      detail = format!("{path}: {detail}")
    )
    .into_owned(),
    TrackListError::Failed { messages } => {
      // `mkvmerge` reported nothing (no JSON `errors`, an empty stderr).
      let detail = if messages.is_empty() {
        rust_i18n::t!("notification.unknownError").into_owned()
      } else {
        messages.join("\n")
      };
      rust_i18n::t!("extract.error.mkvmergeFailed", detail = detail).into_owned()
    }
    TrackListError::Parse { detail } => rust_i18n::t!("extract.error.parseError", detail = detail).into_owned(),
    TrackListError::Other(message) => message.clone(),
  }
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
    let failed = |messages: &[&str]| TrackListError::Failed {
      messages: messages.iter().map(|message| message.to_string()).collect(),
    };
    assert_eq!(
      track_error_message(&TrackListError::NotAvailable {
        path: "/usr/bin/mkvmerge".into(),
        detail: "No such file or directory (os error 2)".into(),
      }),
      "mkvmerge is not available. Please check the MKVToolNix path in Settings. \
       /usr/bin/mkvmerge: No such file or directory (os error 2)"
    );
    assert_eq!(
      track_error_message(&failed(&["The file 'a.mkv' could not be opened for reading."])),
      "mkvmerge failed: The file 'a.mkv' could not be opened for reading."
    );
    assert_eq!(
      track_error_message(&failed(&["one.", "two."])),
      "mkvmerge failed: one.\ntwo."
    );
    assert_eq!(track_error_message(&failed(&[])), "mkvmerge failed: Unknown error");
    assert_eq!(
      track_error_message(&TrackListError::Parse {
        detail: "EOF while parsing".into()
      }),
      "Failed to parse mkvmerge output: EOF while parsing"
    );
    assert_eq!(track_error_message(&TrackListError::Other("as is".into())), "as is");
    crate::i18n::set_language(Language::De);
    assert_eq!(
      track_error_message(&TrackListError::Parse { detail: "EOF".into() }),
      "Konnte mkvmerge-Ausgabe nicht auswerten: EOF"
    );
    assert_eq!(
      track_error_message(&failed(&[])),
      "mkvmerge fehlgeschlagen: Unbekannter Fehler"
    );
    crate::i18n::set_language(Language::EnUS);
  }
}
