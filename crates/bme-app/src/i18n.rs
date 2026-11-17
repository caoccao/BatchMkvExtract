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

//! UI language: the nine locale files under `locales/` (nested keys, `%{name}`
//! placeholders) and the mapping from the configured [`Language`] to a locale
//! code.
//!
//! The `gpui_component` part of every locale file holds gpui-kit's own
//! strings that the app can show (the settings search and Reset All, the
//! select placeholder and search box, the input menu, dialog buttons): gpui-kit
//! 0.10.0 ships none for `de`, `es` and `fr`, and no input menu for `it`.

use bme_core::config::Language;

/// Every shipped locale, as file names under `locales/` and as locale codes.
pub const LOCALES: [&str; 9] = ["de", "en-US", "es", "fr", "it", "ja", "zh-CN", "zh-HK", "zh-TW"];

/// The locale code of a configured language; the config spells it the same
/// way (`"en-US"`, `"zh-TW"`, …).
pub fn locale_code(language: Language) -> &'static str {
  match language {
    Language::De => "de",
    Language::EnUS => "en-US",
    Language::Es => "es",
    Language::Fr => "fr",
    Language::It => "it",
    Language::Ja => "ja",
    Language::ZhCN => "zh-CN",
    Language::ZhHK => "zh-HK",
    Language::ZhTW => "zh-TW",
  }
}

/// Layers the `gpui_component` part of the locale files over gpui-kit's own
/// strings, for every locale. Call once before `gpui_kit::init`; later calls
/// do nothing (rust-i18n accepts one extension per process).
pub fn install_component_strings() {
  static INSTALLED: std::sync::Once = std::sync::Once::new();
  INSTALLED.call_once(|| {
    use gpui_kit::component as gpui_component;
    rust_i18n::extend!(gpui_component);
  });
}

/// Switches the application's and gpui-kit's strings to `language`. The
/// locale is process-wide state outside GPUI's change tracking: callers
/// refresh the windows afterwards.
pub fn set_language(language: Language) {
  gpui_kit::component::set_locale(locale_code(language));
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;
  use std::collections::BTreeMap;
  use std::sync::Mutex;

  /// Tests that switch or read the process-wide locale take this lock.
  pub(crate) static LOCALE_LOCK: Mutex<()> = Mutex::new(());

  fn flatten(prefix: &str, value: &serde_json::Value, out: &mut BTreeMap<String, String>) {
    match value {
      serde_json::Value::Object(map) => {
        for (key, child) in map {
          let path = if prefix.is_empty() {
            key.clone()
          } else {
            format!("{prefix}.{key}")
          };
          flatten(&path, child, out);
        }
      }
      serde_json::Value::String(text) => {
        out.insert(prefix.to_owned(), text.clone());
      }
      other => panic!("{prefix}: unexpected value {other}"),
    }
  }

  fn load(locale: &str) -> BTreeMap<String, String> {
    let path = format!("{}/locales/{locale}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    let value: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|error| panic!("{path}: {error}"));
    let mut keys = BTreeMap::new();
    flatten("", &value, &mut keys);
    keys
  }

  /// The placeholder names of a message, sorted: `%{name}` → `name`.
  fn placeholders(text: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("%{") {
      let tail = &rest[start + 2..];
      let end = tail.find('}').expect("unclosed placeholder");
      names.push(tail[..end].to_owned());
      rest = &tail[end + 1..];
    }
    names.sort();
    names
  }

  #[test]
  fn every_locale_has_the_same_keys_and_placeholders_as_en_us() {
    let reference = load("en-US");
    assert!(reference.len() > 150, "en-US has {} keys", reference.len());
    for locale in LOCALES {
      let messages = load(locale);
      let missing: Vec<_> = reference.keys().filter(|key| !messages.contains_key(*key)).collect();
      let extra: Vec<_> = messages.keys().filter(|key| !reference.contains_key(*key)).collect();
      assert!(
        missing.is_empty() && extra.is_empty(),
        "{locale}: missing {missing:?}, extra {extra:?}"
      );
      for (key, text) in &messages {
        assert!(
          !text.contains("{{"),
          "{locale} {key}: `{{{{name}}}}` placeholder instead of `%{{name}}`: {text}"
        );
        assert_eq!(placeholders(text), placeholders(&reference[key]), "{locale} {key}");
      }
    }
  }

  #[test]
  fn every_configured_language_has_a_locale_file_and_translations() {
    let languages = [
      Language::De,
      Language::EnUS,
      Language::Es,
      Language::Fr,
      Language::It,
      Language::Ja,
      Language::ZhCN,
      Language::ZhHK,
      Language::ZhTW,
    ];
    let available = crate::_rust_i18n_available_locales();
    for language in languages {
      let code = locale_code(language);
      assert!(LOCALES.contains(&code), "{code}");
      // The config file spells the language as its locale code.
      assert_eq!(serde_json::to_value(language).unwrap(), serde_json::json!(code));
      assert!(available.iter().any(|locale| locale == code), "{code} not embedded");
      let close = rust_i18n::t!("tabs.close", locale = code);
      assert!(!close.ends_with("tabs.close"), "{code}: {close}");
    }
    assert_eq!(rust_i18n::t!("tabs.fileList", locale = "de"), "Dateiliste");
    assert_eq!(
      rust_i18n::t!("about.tagline", locale = "en-US"),
      "Batch GUI for mkvextract"
    );
  }

  #[test]
  fn placeholders_are_interpolated() {
    assert_eq!(
      rust_i18n::t!("notification.completedIn", locale = "en-US", elapsed = "00:01:02"),
      "Completed in 00:01:02"
    );
    assert_eq!(
      rust_i18n::t!("extract.error.mkvextractExitCode", locale = "de", code = 2),
      "mkvextract wurde mit dem Code 2 beendet."
    );
    // 0.3.0's single braces are literal text, not placeholders.
    assert!(rust_i18n::t!("settings.templatePlaceholdersHint", locale = "en-US").contains("{file_name}"));
  }

  /// Every key passed as a string literal to `tr!` or `rust_i18n::t!` in
  /// `src/` exists in en-US (the plan's scripted `rg 't!\("'` check).
  #[test]
  fn every_literal_key_in_the_sources_exists_in_en_us() {
    fn sources(directory: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
      for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
          sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
          out.push(path);
        }
      }
    }
    let reference = load("en-US");
    let mut files = Vec::new();
    sources(
      &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
      &mut files,
    );
    let mut checked = 0;
    let mut missing = Vec::new();
    for file in files {
      let text = std::fs::read_to_string(&file).unwrap();
      for marker in ["tr!(\"", "rust_i18n::t!(\""] {
        for (start, _) in text.match_indices(marker) {
          // The marker can end a longer name (`include_str!`).
          if text[..start]
            .chars()
            .next_back()
            .is_some_and(|before| before.is_alphanumeric() || before == '_')
          {
            continue;
          }
          let rest = &text[start + marker.len()..];
          let key = &rest[..rest.find('"').expect("unclosed key")];
          checked += 1;
          if !reference.contains_key(key) {
            missing.push(format!("{}: {key}", file.display()));
          }
        }
      }
    }
    // Keys built at run time: the theme names of the theme select.
    for spec in &crate::theme::THEMES {
      let key = format!("settings.theme{}", spec.name);
      checked += 1;
      if !reference.contains_key(&key) {
        missing.push(key);
      }
    }
    assert!(checked > 75, "only {checked} keys found");
    assert!(missing.is_empty(), "keys missing from en-US: {missing:#?}");
  }

  /// gpui-kit's strings come from the `gpui_component` part of the locale
  /// files, also where gpui-kit ships no translation of its own.
  #[test]
  fn gpui_kit_strings_are_translated_in_every_locale() {
    install_component_strings();
    install_component_strings();
    let reference = load("en-US");
    let keys: Vec<&str> = reference
      .keys()
      .filter_map(|key| key.strip_prefix("gpui_component."))
      .collect();
    assert_eq!(keys.len(), 13, "{keys:?}");
    for locale in LOCALES {
      let messages = load(locale);
      for key in &keys {
        let expected = &messages[&format!("gpui_component.{key}")];
        assert_eq!(
          gpui_kit::component::_rust_i18n_translate(locale, key),
          expected.as_str(),
          "{locale} {key}"
        );
      }
    }
    // gpui-kit's English is kept, so the en-US screens read as before.
    assert_eq!(
      gpui_kit::component::_rust_i18n_translate("en-US", "Settings.Reset All"),
      "Reset All"
    );
    assert_eq!(
      gpui_kit::component::_rust_i18n_translate("de", "Settings.Reset All"),
      "Alle zurücksetzen"
    );
    assert_eq!(
      gpui_kit::component::_rust_i18n_translate("ja", "Input.Paste"),
      "貼り付け"
    );
  }

  #[test]
  fn set_language_switches_the_process_locale() {
    let _guard = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    set_language(Language::Fr);
    assert_eq!(&*gpui_kit::component::locale(), "fr");
    set_language(Language::EnUS);
    assert_eq!(&*rust_i18n::locale(), "en-US");
  }
}
