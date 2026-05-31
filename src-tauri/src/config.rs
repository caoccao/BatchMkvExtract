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

use anyhow::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::constants::APP_NAME;

static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
  #[serde(rename = "displayMode", default)]
  pub display_mode: DisplayMode,
  #[serde(default)]
  pub theme: Theme,
  #[serde(default = "Language::detect_system")]
  pub language: Language,
  #[serde(default)]
  pub integration: ConfigIntegration,
  #[serde(default = "Config::default_profiles")]
  pub profiles: Vec<ConfigProfile>,
  #[serde(rename = "activeProfile", default = "Config::default_active_profile")]
  pub active_profile: String,
  #[serde(default)]
  pub window: ConfigWindow,
  #[serde(default)]
  pub update: ConfigUpdate,
}

impl Config {
  fn default_profiles() -> Vec<ConfigProfile> {
    vec![ConfigProfile::default()]
  }

  fn default_active_profile() -> String {
    ConfigProfile::DEFAULT_NAME.to_owned()
  }
}

impl Default for Config {
  fn default() -> Self {
    Self {
      display_mode: Default::default(),
      theme: Default::default(),
      language: Language::detect_system(),
      integration: Default::default(),
      profiles: Self::default_profiles(),
      active_profile: Self::default_active_profile(),
      window: Default::default(),
      update: Default::default(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigUpdate {
  #[serde(rename = "checkInterval", default)]
  pub check_interval: UpdateCheckInterval,
  #[serde(rename = "lastChecked", default)]
  pub last_checked: i64,
  #[serde(rename = "lastVersion", default)]
  pub last_version: String,
  #[serde(rename = "ignoreVersion", default)]
  pub ignore_version: String,
}

impl Default for ConfigUpdate {
  fn default() -> Self {
    Self {
      check_interval: Default::default(),
      last_checked: 0,
      last_version: String::new(),
      ignore_version: String::new(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum UpdateCheckInterval {
  Daily,
  Weekly,
  Monthly,
}

impl Default for UpdateCheckInterval {
  fn default() -> Self {
    Self::Weekly
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigProfile {
  pub name: String,
  #[serde(rename = "videoTemplate", default = "ConfigProfile::default_template")]
  pub video_template: String,
  #[serde(rename = "audioTemplate", default = "ConfigProfile::default_template")]
  pub audio_template: String,
  #[serde(rename = "subtitleTemplate", default = "ConfigProfile::default_template")]
  pub subtitle_template: String,
  #[serde(rename = "chaptersTemplate", default = "ConfigProfile::default_template_no_language")]
  pub chapters_template: String,
  #[serde(
    rename = "attachmentsTemplate",
    default = "ConfigProfile::default_template_no_language"
  )]
  pub attachments_template: String,
  #[serde(rename = "selectVideo", default)]
  pub select_video: bool,
  #[serde(rename = "selectAudio", default)]
  pub select_audio: bool,
  #[serde(rename = "selectSubtitle", default = "ConfigProfile::default_select_subtitle")]
  pub select_subtitle: bool,
  #[serde(rename = "selectChapters", default)]
  pub select_chapters: bool,
  #[serde(rename = "selectAttachments", default)]
  pub select_attachments: bool,
  #[serde(rename = "videoLanguages", default)]
  pub video_languages: String,
  #[serde(rename = "audioLanguages", default)]
  pub audio_languages: String,
  #[serde(rename = "subtitleLanguages", default = "ConfigProfile::default_subtitle_languages")]
  pub subtitle_languages: String,
  #[serde(rename = "defaultGroupMode", default = "ConfigProfile::default_default_group_mode")]
  pub default_group_mode: bool,
}

impl ConfigProfile {
  pub const DEFAULT_NAME: &'static str = "Default";
  pub const DEFAULT_TEMPLATE: &'static str = "{file_name}.{track_id}.{language}";
  pub const DEFAULT_TEMPLATE_NO_LANGUAGE: &'static str = "{file_name}.{track_id}";

  fn default_template() -> String {
    Self::DEFAULT_TEMPLATE.to_owned()
  }

  fn default_template_no_language() -> String {
    Self::DEFAULT_TEMPLATE_NO_LANGUAGE.to_owned()
  }

  fn default_select_subtitle() -> bool {
    true
  }

  pub const DEFAULT_SUBTITLE_LANGUAGES: &'static str = "eng, chi, spa, ger, fre, jpn";

  fn default_subtitle_languages() -> String {
    Self::DEFAULT_SUBTITLE_LANGUAGES.to_owned()
  }

  fn default_default_group_mode() -> bool {
    true
  }
}

impl Default for ConfigProfile {
  fn default() -> Self {
    Self {
      name: Self::DEFAULT_NAME.to_owned(),
      video_template: Self::default_template(),
      audio_template: Self::default_template(),
      subtitle_template: Self::default_template(),
      chapters_template: Self::default_template_no_language(),
      attachments_template: Self::default_template_no_language(),
      select_video: false,
      select_audio: false,
      select_subtitle: true,
      select_chapters: false,
      select_attachments: false,
      video_languages: String::new(),
      audio_languages: String::new(),
      subtitle_languages: Self::default_subtitle_languages(),
      default_group_mode: true,
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigIntegration {
  #[serde(rename = "mkvToolNixPath", default = "ConfigIntegration::default_mkv_toolnix_path")]
  pub mkv_toolnix_path: String,
  #[serde(rename = "betterMediaInfoPath", default)]
  pub better_media_info_path: String,
}

impl ConfigIntegration {
  fn default_mkv_toolnix_path() -> String {
    if cfg!(target_os = "windows") {
      r"C:\Program Files\MKVToolNix".to_owned()
    } else if cfg!(target_os = "macos") {
      "/Applications/MKVToolNix.app/Contents/MacOS".to_owned()
    } else {
      "/usr/bin".to_owned()
    }
  }
}

impl Default for ConfigIntegration {
  fn default() -> Self {
    Self {
      mkv_toolnix_path: Self::default_mkv_toolnix_path(),
      better_media_info_path: String::new(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum DisplayMode {
  Auto,
  Light,
  Dark,
}

impl Default for DisplayMode {
  fn default() -> Self {
    Self::Auto
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Theme {
  #[serde(alias = "Default")]
  Ocean,
  Aqua,
  Sky,
  Arctic,
  Glacier,
  Mist,
  Slate,
  Charcoal,
  Midnight,
  Indigo,
  Violet,
  Lavender,
  Rose,
  Blush,
  Coral,
  Sunset,
  Amber,
  Sand,
  Forest,
  Emerald,
}

impl Default for Theme {
  fn default() -> Self {
    Self::Ocean
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Language {
  #[serde(rename = "de")]
  De,
  #[serde(rename = "en-US")]
  EnUS,
  #[serde(rename = "es")]
  Es,
  #[serde(rename = "fr")]
  Fr,
  #[serde(rename = "it")]
  It,
  #[serde(rename = "ja")]
  Ja,
  #[serde(rename = "zh-CN")]
  ZhCN,
  #[serde(rename = "zh-HK")]
  ZhHK,
  #[serde(rename = "zh-TW")]
  ZhTW,
}

impl Default for Language {
  fn default() -> Self {
    Self::EnUS
  }
}

impl Language {
  fn detect_system() -> Self {
    let locales: Vec<String> = sys_locale::get_locales().collect();
    for locale in &locales {
      if let Some(language) = Self::from_locale_tag(locale) {
        log::debug!("Detected system language {:?} from locale {}.", language, locale);
        return language;
      }
    }
    if !locales.is_empty() {
      log::debug!("No supported app language found in system locales {:?}.", locales);
    }
    Self::default()
  }

  fn from_locale_tag(locale: &str) -> Option<Self> {
    let normalized = normalize_locale_tag(locale)?;
    let mut parts = normalized.split('-');
    let language = parts.next()?;
    let mut script: Option<&str> = None;
    let mut region: Option<&str> = None;
    for part in parts {
      if part.len() == 4 && script.is_none() {
        script = Some(part);
      } else if (part.len() == 2 || part.len() == 3) && region.is_none() {
        region = Some(part);
      }
    }

    match language {
      "de" => Some(Self::De),
      "en" => Some(Self::EnUS),
      "es" => Some(Self::Es),
      "fr" => Some(Self::Fr),
      "it" => Some(Self::It),
      "ja" => Some(Self::Ja),
      "zh" => Some(match (script, region) {
        (Some("hant"), Some("hk" | "mo")) => Self::ZhHK,
        (Some("hant"), _) => Self::ZhTW,
        (Some("hans"), _) => Self::ZhCN,
        (_, Some("tw")) => Self::ZhTW,
        (_, Some("hk" | "mo")) => Self::ZhHK,
        _ => Self::ZhCN,
      }),
      _ => None,
    }
  }
}

fn normalize_locale_tag(locale: &str) -> Option<String> {
  let locale = locale
    .split(':')
    .find(|part| !part.trim().is_empty())
    .unwrap_or(locale)
    .trim();
  if locale.eq_ignore_ascii_case("c") || locale.eq_ignore_ascii_case("posix") {
    return None;
  }
  let locale = locale
    .split('.')
    .next()
    .unwrap_or(locale)
    .split('@')
    .next()
    .unwrap_or(locale)
    .replace('_', "-")
    .to_ascii_lowercase();
  if locale.is_empty() || locale == "c" || locale == "posix" {
    None
  } else {
    Some(locale)
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigWindow {
  #[serde(default)]
  pub position: ConfigWindowPosition,
  #[serde(default)]
  pub size: ConfigWindowSize,
}

impl Default for ConfigWindow {
  fn default() -> Self {
    Self {
      position: Default::default(),
      size: Default::default(),
    }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigWindowPosition {
  pub x: i32,
  pub y: i32,
}

impl Default for ConfigWindowPosition {
  fn default() -> Self {
    Self { x: -1, y: -1 }
  }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigWindowSize {
  pub width: u32,
  pub height: u32,
}

impl Default for ConfigWindowSize {
  fn default() -> Self {
    Self {
      width: 1200,
      height: 900,
    }
  }
}

impl Config {
  fn new() -> Self {
    let config_path_buf = Self::get_path_buf();
    if config_path_buf.exists() {
      Self::load(config_path_buf)
    } else {
      log::debug!("Loading default config.");
      let config = Self::default();
      if let Err(err) = config.save(config_path_buf) {
        log::error!("Couldn't save the default config because {}", err);
      }
      config
    }
  }

  fn get_path_buf() -> PathBuf {
    let config_dir = Self::get_config_dir();
    if !config_dir.exists() {
      if let Err(err) = std::fs::create_dir_all(&config_dir) {
        log::warn!("Couldn't create config dir {}: {}", config_dir.display(), err);
      }
    }
    config_dir.join(format!("{}.json", APP_NAME))
  }

  fn get_exe_dir() -> PathBuf {
    std::env::current_exe().unwrap().parent().unwrap().to_path_buf()
  }

  #[cfg(target_os = "linux")]
  fn get_config_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
      if !xdg.is_empty() {
        return PathBuf::from(xdg).join(APP_NAME);
      }
    }
    if let Ok(home) = std::env::var("HOME") {
      return PathBuf::from(home).join(".config").join(APP_NAME);
    }
    Self::get_exe_dir()
  }

  #[cfg(target_os = "macos")]
  fn get_config_dir() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
      return PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join(APP_NAME);
    }
    Self::get_exe_dir()
  }

  #[cfg(target_os = "windows")]
  fn get_config_dir() -> PathBuf {
    let exe_dir = Self::get_exe_dir();
    let exe_path_lc = exe_dir.to_string_lossy().to_ascii_lowercase();
    let starts_with_env = |env_var: &str| -> bool {
      std::env::var(env_var)
        .ok()
        .map(|p| !p.is_empty() && exe_path_lc.starts_with(&p.to_ascii_lowercase()))
        .unwrap_or(false)
    };
    let is_installed =
      starts_with_env("LOCALAPPDATA") || starts_with_env("ProgramFiles") || starts_with_env("ProgramFiles(x86)");
    if is_installed {
      if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
          return PathBuf::from(appdata).join(APP_NAME);
        }
      }
    }
    exe_dir
  }

  fn load(path: PathBuf) -> Self {
    let path_string = path.to_str().unwrap();
    log::debug!("Loading config from {}.", path_string);
    let file = File::open(&path).expect(format!("Couldn't open config file {}.", path_string).as_str());
    let buf_reader = BufReader::new(file);
    let value: serde_json::Value =
      serde_json::from_reader(buf_reader).expect(format!("Couldn't parse config file {}.", path_string).as_str());
    let should_save_language = value
      .as_object()
      .map(|object| !object.contains_key("language"))
      .unwrap_or(false);
    let config: Self =
      serde_json::from_value(value).expect(format!("Couldn't parse config file {}.", path_string).as_str());
    if should_save_language {
      log::debug!("Saving detected language to config {}.", path_string);
      if let Err(err) = config.save(path) {
        log::error!("Couldn't save the detected language because {}", err);
      }
    }
    config
  }

  fn save(&self, path: PathBuf) -> Result<()> {
    let cloned_path = path.clone();
    let path_string = cloned_path.to_str().unwrap();
    log::debug!("Saving config to {}.", path_string);
    let file = File::create(path).expect(format!("Couldn't create config file {}.", path_string).as_str());
    let buf_writer = BufWriter::new(file);
    serde_json::to_writer_pretty(buf_writer, &self).map_err(Error::msg)
  }
}

pub fn get_config() -> Config {
  CONFIG
    .get_or_init(|| RwLock::new(Config::new()))
    .read()
    .unwrap()
    .clone()
}

pub fn set_config(config: Config) -> Result<()> {
  let config_path_buf = Config::get_path_buf();
  let result = config.save(config_path_buf);
  CONFIG
    .get_or_init(|| RwLock::new(Config::new()))
    .write()
    .unwrap()
    .clone_from(&config);
  result
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn config_deserialization_uses_defaults_for_missing_nodes() {
    let config: Config = serde_json::from_str("{}").unwrap();
    let default_profile = ConfigProfile::default();

    assert!(matches!(config.display_mode, DisplayMode::Auto));
    assert!(matches!(config.theme, Theme::Ocean));
    assert_eq!(
      config.integration.mkv_toolnix_path,
      ConfigIntegration::default().mkv_toolnix_path
    );
    assert_eq!(config.integration.better_media_info_path, "");
    assert_eq!(config.profiles.len(), 1);
    assert_eq!(config.profiles[0].name, default_profile.name);
    assert_eq!(config.profiles[0].video_template, default_profile.video_template);
    assert!(config.profiles[0].select_subtitle);
    assert_eq!(
      config.profiles[0].subtitle_languages,
      default_profile.subtitle_languages
    );
    assert!(config.profiles[0].default_group_mode);
    assert_eq!(config.active_profile, ConfigProfile::DEFAULT_NAME);
    assert_eq!(config.window.position.x, -1);
    assert_eq!(config.window.position.y, -1);
    assert_eq!(config.window.size.width, 1200);
    assert_eq!(config.window.size.height, 900);
    assert!(matches!(config.update.check_interval, UpdateCheckInterval::Weekly));
  }

  #[test]
  fn config_deserialization_preserves_present_nodes_while_filling_missing_children() {
    let config: Config = serde_json::from_str(
      r#"{
                "displayMode": "Dark",
                "language": "ja",
                "integration": {
                    "betterMediaInfoPath": "/Applications/BetterMediaInfo.app"
                },
                "profiles": [
                    {
                        "name": "Custom",
                        "audioTemplate": "{file_name}.audio",
                        "selectAudio": true,
                        "subtitleLanguages": "eng,jpn",
                        "defaultGroupMode": false
                    }
                ],
                "window": {
                    "position": {
                        "x": 10
                    }
                },
                "update": {}
            }"#,
    )
    .unwrap();
    let default_profile = ConfigProfile::default();

    assert!(matches!(config.display_mode, DisplayMode::Dark));
    assert!(matches!(config.language, Language::Ja));
    assert_eq!(
      config.integration.mkv_toolnix_path,
      ConfigIntegration::default().mkv_toolnix_path
    );
    assert_eq!(
      config.integration.better_media_info_path,
      "/Applications/BetterMediaInfo.app"
    );
    assert_eq!(config.profiles.len(), 1);
    assert_eq!(config.profiles[0].name, "Custom");
    assert_eq!(config.profiles[0].video_template, default_profile.video_template);
    assert_eq!(config.profiles[0].audio_template, "{file_name}.audio");
    assert!(config.profiles[0].select_audio);
    assert!(config.profiles[0].select_subtitle);
    assert_eq!(config.profiles[0].subtitle_languages, "eng,jpn");
    assert!(!config.profiles[0].default_group_mode);
    assert_eq!(config.window.position.x, 10);
    assert_eq!(config.window.position.y, -1);
    assert_eq!(config.window.size.width, 1200);
    assert_eq!(config.window.size.height, 900);
    assert!(matches!(config.update.check_interval, UpdateCheckInterval::Weekly));
  }

  #[test]
  fn language_from_locale_tag_maps_supported_locales() {
    assert!(matches!(Language::default(), Language::EnUS));
    assert!(matches!(Language::from_locale_tag("de-DE"), Some(Language::De)));
    assert!(matches!(Language::from_locale_tag("en_US.UTF-8"), Some(Language::EnUS)));
    assert!(matches!(Language::from_locale_tag("es-MX"), Some(Language::Es)));
    assert!(matches!(Language::from_locale_tag("fr-CA"), Some(Language::Fr)));
    assert!(matches!(Language::from_locale_tag("it-IT"), Some(Language::It)));
    assert!(matches!(Language::from_locale_tag("ja-JP"), Some(Language::Ja)));
    assert!(matches!(Language::from_locale_tag("zh-Hans-CN"), Some(Language::ZhCN)));
    assert!(matches!(Language::from_locale_tag("zh-Hant-TW"), Some(Language::ZhTW)));
    assert!(matches!(Language::from_locale_tag("zh-Hant-HK"), Some(Language::ZhHK)));
    assert!(matches!(Language::from_locale_tag("zh-MO"), Some(Language::ZhHK)));
    assert!(matches!(Language::from_locale_tag("C.UTF-8"), None));
  }
}
