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

//! What the app tells the user outside its views: for now, the notice of a
//! config file that could not be read as it was.

use bme_core::config::UnreadableConfig;
use gpui_kit::App;

use crate::state::Stores;
use crate::state::ui::NotificationKind;

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
