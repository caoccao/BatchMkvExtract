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

//! The config file: its schema (unchanged since 0.3.0 apart from
//! `window.display`), where it lives, and how it is read and written.
//!
//! A file that cannot be read or holds invalid values never stops the app:
//! the original is kept under a backup name, every value that still reads is
//! kept, and the problem is reported through [`unreadable_config`]. Saves
//! are atomic (a temporary file renamed over the config).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, PoisonError, RwLock};

use crate::constants::APP_NAME;

static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();
static UNREADABLE_CONFIG: OnceLock<UnreadableConfig> = OnceLock::new();

/// How many backup names [`keep_aside`] tries before it gives up.
const MAX_BACKUPS: u32 = 100;
/// How many times a load reads the config file again when another instance
/// of the app replaced it meanwhile (see [`Config::reload_if_replaced`]).
const MAX_RELOADS: u32 = 3;

/// A config file that did not load as it was. The app started with every
/// value that could still be read from it and the defaults for the rest, and
/// the original was kept under a backup name so that no save overwrites it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadableConfig {
  /// What was wrong with the file.
  pub reason: String,
  /// Where the original file was kept; `None` when it could not be moved.
  pub backup: Option<PathBuf>,
}

/// The problem with the config file read at startup, if it had one.
pub fn unreadable_config() -> Option<&'static UnreadableConfig> {
  UNREADABLE_CONFIG.get()
}

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

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum UpdateCheckInterval {
  Daily,
  #[default]
  Weekly,
  Monthly,
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
  pub const DEFAULT_SUBTITLE_LANGUAGES: &'static str = "eng, chi, spa, ger, fre, jpn";

  fn default_template() -> String {
    Self::DEFAULT_TEMPLATE.to_owned()
  }

  fn default_template_no_language() -> String {
    Self::DEFAULT_TEMPLATE_NO_LANGUAGE.to_owned()
  }

  fn default_select_subtitle() -> bool {
    true
  }

  fn default_subtitle_languages() -> String {
    Self::DEFAULT_SUBTITLE_LANGUAGES.to_owned()
  }

  fn default_default_group_mode() -> bool {
    true
  }

  /// A fresh profile called `name`, as 0.3.0 made one: the
  /// Default profile's templates and language filters, auto-selecting
  /// subtitle tracks and starting in Group Mode only when it is the Default
  /// profile itself.
  pub fn named(name: &str) -> Self {
    let is_default = name == Self::DEFAULT_NAME;
    Self {
      name: name.to_owned(),
      select_subtitle: is_default,
      default_group_mode: is_default,
      ..Self::default()
    }
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

/// The external tools (0.3.0 calls the block `integration`).
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum DisplayMode {
  #[default]
  Auto,
  Light,
  Dark,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Theme {
  #[default]
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub enum Language {
  #[serde(rename = "de")]
  De,
  #[default]
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

impl Language {
  fn detect_system() -> Self {
    let locales: Vec<String> = sys_locale::get_locales().collect();
    Self::from_locales(&locales)
  }

  /// The first supported app language among the system's `locales`, in
  /// their order of preference; en-US when none is supported.
  fn from_locales(locales: &[String]) -> Self {
    for locale in locales {
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

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct ConfigWindow {
  /// The display the window was on, as the platform names it (a display
  /// UUID); empty when unknown. Displays at different scales can overlap in
  /// logical coordinates, so the position alone may not say which one it is.
  /// The only field 1.0.0 adds to 0.3.0's config.
  #[serde(default)]
  pub display: String,
  #[serde(default)]
  pub position: ConfigWindowPosition,
  #[serde(default)]
  pub size: ConfigWindowSize,
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
    Self::new_at(Self::get_path_buf())
  }

  /// The config of the file at `path`, created with the defaults when it
  /// does not exist. A problem with the file is kept for
  /// [`unreadable_config`].
  fn new_at(config_path_buf: PathBuf) -> Self {
    if config_path_buf.exists() {
      let (config, problem) = Self::load(config_path_buf);
      if let Some(problem) = problem {
        let _ = UNREADABLE_CONFIG.set(problem);
      }
      config
    } else {
      log::debug!("Loading default config.");
      let config = Self::default();
      if let Err(err) = config.save(&config_path_buf) {
        log::error!("Couldn't save the default config because {:#}", err);
      }
      config
    }
  }

  fn get_path_buf() -> PathBuf {
    config_dir().join(format!("{}.json", APP_NAME))
  }

  /// The directory of the executable; the current directory when the
  /// executable's path is unknown.
  fn get_exe_dir() -> PathBuf {
    std::env::current_exe()
      .ok()
      .and_then(|exe| exe.parent().map(Path::to_path_buf))
      .or_else(|| std::env::current_dir().ok())
      .unwrap_or_else(|| PathBuf::from("."))
  }

  #[cfg(target_os = "linux")]
  fn get_config_dir() -> PathBuf {
    Self::linux_config_dir(
      std::env::var("XDG_CONFIG_HOME").ok(),
      std::env::var("HOME").ok(),
      Self::get_exe_dir,
    )
  }

  /// `$XDG_CONFIG_HOME/BatchMkvExtract`, else `$HOME/.config/BatchMkvExtract`,
  /// else the executable's directory.
  #[cfg(any(target_os = "linux", test))]
  fn linux_config_dir(xdg_config_home: Option<String>, home: Option<String>, exe_dir: fn() -> PathBuf) -> PathBuf {
    if let Some(xdg) = xdg_config_home.filter(|xdg| !xdg.is_empty()) {
      return PathBuf::from(xdg).join(APP_NAME);
    }
    if let Some(home) = home {
      return PathBuf::from(home).join(".config").join(APP_NAME);
    }
    exe_dir()
  }

  #[cfg(target_os = "macos")]
  fn get_config_dir() -> PathBuf {
    Self::macos_config_dir(std::env::var("HOME").ok(), Self::get_exe_dir)
  }

  /// `$HOME/Library/Application Support/BatchMkvExtract`, else the
  /// executable's directory.
  #[cfg(any(target_os = "macos", test))]
  fn macos_config_dir(home: Option<String>, exe_dir: fn() -> PathBuf) -> PathBuf {
    match home {
      Some(home) => PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join(APP_NAME),
      None => exe_dir(),
    }
  }

  #[cfg(target_os = "windows")]
  fn get_config_dir() -> PathBuf {
    let exe_dir = Self::get_exe_dir();
    let exe_dir_string = exe_dir.to_string_lossy();
    let is_installed = ["LOCALAPPDATA", "ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
      .iter()
      .filter_map(|name| std::env::var(name).ok())
      .any(|root| is_within_directory(&exe_dir_string, &root));
    if is_installed {
      if let Ok(appdata) = std::env::var("APPDATA") {
        if !appdata.is_empty() {
          return PathBuf::from(appdata).join(APP_NAME);
        }
      }
    }
    exe_dir
  }

  /// Reads the config file at `path`. A file that cannot be read or holds
  /// invalid values never stops the app: it is kept as `*.json.bad` (see
  /// [`keep_aside`]), the config salvaged from it is saved in its place, and
  /// the problem is returned for the app to show.
  fn load(path: PathBuf) -> (Self, Option<UnreadableConfig>) {
    log::debug!("Loading config from {}.", path.display());
    let read = std::fs::read(&path);
    Self::load_read(path, read)
  }

  /// [`Self::load`] with the result of reading the file. Another instance
  /// of the app may have replaced the file since it was read.
  fn load_read(path: PathBuf, read: std::io::Result<Vec<u8>>) -> (Self, Option<UnreadableConfig>) {
    Self::load_from(path, read, MAX_RELOADS)
  }

  /// [`Self::load_read`], which may read the file again up to `reloads`
  /// times.
  fn load_from(path: PathBuf, read: std::io::Result<Vec<u8>>, reloads: u32) -> (Self, Option<UnreadableConfig>) {
    let parsed = match &read {
      Ok(bytes) => Self::parse(bytes),
      Err(error) => Parsed::Salvaged(Self::default(), format!("it could not be read: {error}")),
    };
    match parsed {
      Parsed::Loaded(mut config, has_language) => {
        let normalized = config.normalize_profiles();
        if !has_language || normalized {
          if let Some(newer) = Self::reload_if_replaced(&path, &read, reloads) {
            return newer;
          }
          log::debug!("Saving the completed config {}.", path.display());
          if let Err(err) = config.save(&path) {
            log::error!("Couldn't save the completed config because {:#}", err);
          }
        }
        (config, None)
      }
      Parsed::Salvaged(mut config, reason) => {
        // A file that could not be read is moved aside, and the move takes
        // whatever is there: settings another instance saved since the read
        // would go to the backup. A file that can be read now is loaded as
        // it is instead.
        if read.is_err()
          && let Some(newer) = Self::reload_if_replaced(&path, &read, reloads)
        {
          return newer;
        }
        config.normalize_profiles();
        log::error!("Config file {} is unusable: {}.", path.display(), reason);
        let backup = match keep_aside(&path, read.as_deref().ok()) {
          Ok(backup) => {
            log::warn!("Kept the unusable config file as {}.", backup.display());
            if let Some(newer) = Self::reload_if_replaced(&path, &read, reloads) {
              return newer;
            }
            if let Err(err) = config.save(&path) {
              log::error!("Couldn't save the repaired config because {:#}", err);
            }
            Some(backup)
          }
          Err(err) => {
            log::error!("Couldn't keep the unusable config file {}: {}", path.display(), err);
            None
          }
        };
        (config, Some(UnreadableConfig { reason, backup }))
      }
    }
  }

  /// Another instance of the app may have saved the config file since this
  /// one read it: newer settings, or its own repair or completion of the
  /// same file. A save made from what this instance read would replace
  /// them, so a load reads the file again just before it saves, and an
  /// unreadable file before it is moved aside. When the file no longer holds
  /// what was read, or can be read after a read that failed, it is loaded
  /// again as it is now, and that load's result is returned. `None` when the
  /// file still holds what was read, when it is gone or cannot be read
  /// (nothing newer to keep), or when no reload is left.
  fn reload_if_replaced(
    path: &Path,
    read: &std::io::Result<Vec<u8>>,
    reloads: u32,
  ) -> Option<(Self, Option<UnreadableConfig>)> {
    if reloads == 0 {
      return None;
    }
    let now = std::fs::read(path).ok()?;
    if read.as_ref().is_ok_and(|bytes| *bytes == now) {
      return None;
    }
    log::warn!(
      "Config file {} was replaced while it was being loaded; loading it again.",
      path.display()
    );
    Some(Self::load_from(path.to_path_buf(), Ok(now), reloads - 1))
  }

  /// Makes the profile list what the app relies on; `true` when it changed.
  /// A hand-edited file can break it: a profile name used twice is made
  /// unique (`Anime (2)`), a missing `Default` profile is added first, and an
  /// `activeProfile` that names no profile becomes `Default`.
  fn normalize_profiles(&mut self) -> bool {
    let mut changed = false;
    // A new name avoids every name in the file, so no later profile's own
    // name is taken by a rename.
    let taken: Vec<String> = self.profiles.iter().map(|profile| profile.name.clone()).collect();
    let mut names: Vec<String> = Vec::new();
    for profile in &mut self.profiles {
      if names.contains(&profile.name) {
        let base = profile.name.clone();
        let unique = (2..)
          .map(|n| format!("{base} ({n})"))
          .find(|name| !names.contains(name) && !taken.contains(name))
          .expect("an unused name");
        log::warn!("Renamed the second profile called {base:?} to {unique:?}.");
        profile.name = unique;
        changed = true;
      }
      names.push(profile.name.clone());
    }
    if !names.iter().any(|name| name == ConfigProfile::DEFAULT_NAME) {
      log::warn!("Added the missing {:?} profile.", ConfigProfile::DEFAULT_NAME);
      self.profiles.insert(0, ConfigProfile::default());
      names.push(ConfigProfile::DEFAULT_NAME.to_owned());
      changed = true;
    }
    if !names.contains(&self.active_profile) {
      log::warn!(
        "The active profile {:?} does not exist; using {:?}.",
        self.active_profile,
        ConfigProfile::DEFAULT_NAME
      );
      self.active_profile = ConfigProfile::DEFAULT_NAME.to_owned();
      changed = true;
    }
    changed
  }

  /// Parses a config file's contents.
  fn parse(bytes: &[u8]) -> Parsed {
    let value: Value = match serde_json::from_slice(bytes) {
      Ok(value) => value,
      Err(error) => return Parsed::Salvaged(Self::default(), format!("it is not valid JSON ({error})")),
    };
    let has_language = value.as_object().is_some_and(|object| object.contains_key("language"));
    match serde_json::from_value::<Self>(value.clone()) {
      Ok(config) => Parsed::Loaded(config, has_language),
      Err(error) => Parsed::Salvaged(Self::salvage(&value), format!("it holds an invalid value ({error})")),
    }
  }

  /// Builds a config from the defaults and every value of `value` that
  /// still deserialises: an unknown enum value written by a newer version, a
  /// wrong type from a hand edit or a broken profile costs only that value,
  /// not the whole file.
  fn salvage(value: &Value) -> Self {
    let Ok(mut document) = serde_json::to_value(Self::default()) else {
      return Self::default();
    };
    keep_valid_values(&mut document, &mut Vec::new(), value);
    serde_json::from_value(document).unwrap_or_default()
  }

  /// Writes the config to a new temporary file of its own next to `path`
  /// (`BatchMkvExtract.json.<process id>-<counter>.tmp`), flushes it to disk
  /// and renames it over `path`, so a crash or a full disk never leaves a
  /// truncated config file behind, and two instances of the app saving at
  /// once never write into the same file: the last rename wins with a whole
  /// config.
  fn save(&self, path: &Path) -> Result<()> {
    log::debug!("Saving config to {}.", path.display());
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let (temp, file) = crate::unique_file::create(|tag| path.with_file_name(format!("{name}.{tag}.tmp")))
      .with_context(|| format!("Couldn't write config file {}", path.display()))?;
    let write = || -> Result<()> {
      let mut writer = BufWriter::new(file);
      serde_json::to_writer_pretty(&mut writer, self)?;
      writer.flush()?;
      writer.get_ref().sync_all()?;
      Ok(())
    };
    if let Err(error) = write() {
      let _ = std::fs::remove_file(&temp);
      return Err(error.context(format!("Couldn't write config file {}", temp.display())));
    }
    if let Err(error) = std::fs::rename(&temp, path) {
      let _ = std::fs::remove_file(&temp);
      return Err(anyhow::Error::new(error).context(format!("Couldn't replace config file {}", path.display())));
    }
    Ok(())
  }
}

/// Keeps an unusable config file under a backup name next to it that no
/// file has: `BatchMkvExtract.json.bad`, else `BatchMkvExtract.json.2.bad`,
/// `.3.bad` and so on. A backup is never replaced, also not by another
/// instance of the app repairing the same file at the same time. The backup
/// holds `contents`, the bytes this instance read and could not use, even
/// when another instance has replaced the file since. A file that could not
/// be read at all, and still cannot be read just before, is moved there
/// instead. Returns the backup's path.
fn keep_aside(path: &Path, contents: Option<&[u8]>) -> std::io::Result<PathBuf> {
  let name = path.file_name().unwrap_or_default().to_string_lossy();
  let mut number = 1;
  let (backup, mut file) = loop {
    let backup = match number {
      1 => path.with_file_name(format!("{name}.bad")),
      n => path.with_file_name(format!("{name}.{n}.bad")),
    };
    match std::fs::File::options().write(true).create_new(true).open(&backup) {
      Ok(file) => break (backup, file),
      Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists && number < MAX_BACKUPS => number += 1,
      Err(err) => return Err(err),
    }
  };
  let kept = match contents {
    Some(contents) => file.write_all(contents).and_then(|()| file.sync_all()),
    None => {
      // Replaces only the empty file just created for it.
      drop(file);
      std::fs::rename(path, &backup)
    }
  };
  if let Err(err) = kept {
    let _ = std::fs::remove_file(&backup);
    return Err(err);
  }
  Ok(backup)
}

/// What [`Config::parse`] made of a config file.
enum Parsed {
  /// Every value loaded; whether the file names a language.
  Loaded(Config, bool),
  /// What could be salvaged (see [`Config::salvage`]) and why the file did
  /// not load as it was.
  Salvaged(Config, String),
}

/// Copies into `document` (a serialised default [`Config`]) each value of
/// the object `value` at `path` with which the document still deserialises.
/// A rejected value that is an object or an array where the document has one
/// is salvaged member by member instead.
fn keep_valid_values(document: &mut Value, path: &mut Vec<String>, value: &Value) {
  let Some(fields) = value.as_object() else {
    return;
  };
  for (key, field) in fields {
    let Some(previous) = object_at(document, path).map(|object| object.insert(key.clone(), field.clone())) else {
      return;
    };
    if serde_json::from_value::<Config>(document.clone()).is_ok() {
      continue;
    }
    if let Some(object) = object_at(document, path) {
      match &previous {
        Some(previous) => object.insert(key.clone(), previous.clone()),
        None => object.remove(key),
      };
    }
    path.push(key.clone());
    match (&previous, field) {
      (Some(Value::Object(_)), Value::Object(_)) => keep_valid_values(document, path, field),
      (Some(Value::Array(_)), Value::Array(items)) => keep_valid_items(document, path, items),
      _ => {}
    }
    path.pop();
  }
}

/// Replaces the array at `path` with the items of `items` that keep the
/// document valid; an empty array when none does. An object item that does
/// not keep it valid as it is, such as a profile with one mistyped value, is
/// salvaged member by member: it starts empty (every value its default) and
/// takes each of its members that keeps the document valid, so one bad value
/// costs only that value.
fn keep_valid_items(document: &mut Value, path: &[String], items: &[Value]) {
  let Some((key, parent)) = path.split_last() else {
    return;
  };
  let Some(object) = object_at(document, parent) else {
    return;
  };
  let previous = object.insert(key.clone(), Value::Array(Vec::new()));
  if serde_json::from_value::<Config>(document.clone()).is_err() {
    if let (Some(object), Some(previous)) = (object_at(document, parent), previous) {
      object.insert(key.clone(), previous);
    }
    return;
  }
  for item in items {
    let push = |document: &mut Value, item: Option<&Value>| {
      if let Some(Value::Array(array)) = object_at(document, parent).and_then(|object| object.get_mut(key)) {
        match item {
          Some(item) => array.push(item.clone()),
          None => {
            array.pop();
          }
        }
      }
    };
    push(document, Some(item));
    if serde_json::from_value::<Config>(document.clone()).is_ok() {
      continue;
    }
    push(document, None);
    if let Value::Object(members) = item {
      push(document, Some(&Value::Object(serde_json::Map::new())));
      if serde_json::from_value::<Config>(document.clone()).is_err() {
        push(document, None);
        continue;
      }
      for (member, value) in members {
        if let Some(salvaged) = last_item(document, parent, key) {
          salvaged.insert(member.clone(), value.clone());
        }
        if serde_json::from_value::<Config>(document.clone()).is_err()
          && let Some(salvaged) = last_item(document, parent, key)
        {
          salvaged.remove(member);
        }
      }
    }
  }
}

/// The last item of the array `key` of the object at `parent`, when it is an
/// object.
fn last_item<'a>(
  document: &'a mut Value,
  parent: &[String],
  key: &str,
) -> Option<&'a mut serde_json::Map<String, Value>> {
  object_at(document, parent)?
    .get_mut(key)?
    .as_array_mut()?
    .last_mut()?
    .as_object_mut()
}

/// The object at `path` (object keys from the root) inside `document`.
fn object_at<'a>(document: &'a mut Value, path: &[String]) -> Option<&'a mut serde_json::Map<String, Value>> {
  path
    .iter()
    .try_fold(document, |value, key| value.get_mut(key))
    .and_then(Value::as_object_mut)
}

/// Whether the directory `dir` is `root` or inside it, comparing whole path
/// components case-insensitively, as Windows paths compare; `\` and `/`
/// both separate components.
#[cfg(any(target_os = "windows", test))]
fn is_within_directory(dir: &str, root: &str) -> bool {
  let components = |path: &str| -> Vec<String> {
    path
      .split(['\\', '/'])
      .filter(|component| !component.is_empty())
      .map(str::to_lowercase)
      .collect()
  };
  let (dir, root) = (components(dir), components(root));
  !root.is_empty() && dir.len() >= root.len() && dir[..root.len()] == root[..]
}

/// The directory that holds the config file (and the app's log), created if
/// needed. Computed without reading the config.
pub fn config_dir() -> PathBuf {
  let dir = Config::get_config_dir();
  if let Err(err) = std::fs::create_dir_all(&dir) {
    log::warn!("Couldn't create config dir {}: {}", dir.display(), err);
  }
  dir
}

pub fn get_config() -> Config {
  CONFIG
    .get_or_init(|| RwLock::new(Config::new()))
    .read()
    .unwrap_or_else(PoisonError::into_inner)
    .clone()
}

/// Saves `config` and makes it the config [`get_config`] returns. The save
/// keeps what another instance of the app saved meanwhile
/// ([`save_merged`]): only the values `config` changed since this process's
/// last save, or its load, replace those in the file. The lock is held
/// across the save and the update, so concurrent calls leave the in-memory
/// config with the value last saved; a failed save leaves it as it was, so
/// the next save carries its changes too.
pub fn set_config(config: Config) -> Result<()> {
  let mut current = CONFIG
    .get_or_init(|| RwLock::new(Config::new()))
    .write()
    .unwrap_or_else(PoisonError::into_inner);
  save_merged(&Config::get_path_buf(), &current, &config)?;
  *current = config;
  Ok(())
}

/// Saves `ours` over the config file at `path`, which other instances of the
/// app may have saved since this one last saved `base` (or loaded it). Under
/// an interprocess lock on `<path>.lock`, the file is read again and each
/// value `ours` changed from `base` replaces the file's: an object member by
/// member, anything else whole (the profile list is one value). Every other
/// value stays as the file has it, so a window size saved here does not put
/// back a profile list another instance changed. A file that cannot be read
/// as a config is replaced by `ours`. Returns what was written.
fn save_merged(path: &Path, base: &Config, ours: &Config) -> Result<Config> {
  let _lock = lock_config_file(path);
  let theirs = std::fs::read(path).ok().and_then(|bytes| match Config::parse(&bytes) {
    Parsed::Loaded(theirs, _) => Some(theirs),
    Parsed::Salvaged(..) => None,
  });
  let merged = match theirs {
    Some(theirs) => merge_configs(base, ours, &theirs),
    None => ours.clone(),
  };
  merged.save(path)?;
  Ok(merged)
}

/// Takes the interprocess lock of the config file at `path`; released when
/// the returned file is dropped. `None` (a save without the lock) when the
/// lock file cannot be opened or locked.
fn lock_config_file(path: &Path) -> Option<std::fs::File> {
  let name = path.file_name().unwrap_or_default().to_string_lossy();
  let lock = std::fs::File::options()
    .read(true)
    .write(true)
    .create(true)
    .truncate(false)
    .open(path.with_file_name(format!("{name}.lock")))
    .inspect_err(|error| log::warn!("Couldn't open the config lock file: {error}"))
    .ok()?;
  match lock.lock() {
    Ok(()) => Some(lock),
    Err(error) => {
      log::warn!("Couldn't lock the config file: {error}");
      None
    }
  }
}

/// `theirs` with what `ours` changed from `base` ([`merge_values`]), its
/// profile list made valid again.
fn merge_configs(base: &Config, ours: &Config, theirs: &Config) -> Config {
  let as_value = |config: &Config| serde_json::to_value(config).unwrap_or(Value::Null);
  let (base_value, ours_value, mut merged) = (as_value(base), as_value(ours), as_value(theirs));
  merge_values(&base_value, &ours_value, &mut merged);
  let mut merged: Config = serde_json::from_value(merged).unwrap_or_else(|_| ours.clone());
  merged.normalize_profiles();
  merged
}

/// Applies onto `theirs` what `ours` changed from `base`: objects member by
/// member, any other value whole.
fn merge_values(base: &Value, ours: &Value, theirs: &mut Value) {
  if ours == base {
    return;
  }
  match (base, ours, theirs) {
    (Value::Object(base), Value::Object(ours), Value::Object(theirs)) => {
      for (key, value) in ours {
        match (base.get(key), theirs.get_mut(key)) {
          (Some(base), Some(theirs)) => merge_values(base, value, theirs),
          _ => {
            theirs.insert(key.clone(), value.clone());
          }
        }
      }
    }
    (_, ours, theirs) => *theirs = ours.clone(),
  }
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
    assert_eq!(config.window.display, "");
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

  #[test]
  fn locale_tags_without_a_supported_language_fall_back() {
    assert_eq!(Language::from_locale_tag("zh"), Some(Language::ZhCN));
    assert_eq!(Language::from_locale_tag("zh_TW"), Some(Language::ZhTW));
    assert_eq!(Language::from_locale_tag("zh-Hant"), Some(Language::ZhTW));
    assert_eq!(Language::from_locale_tag("de_DE@euro"), Some(Language::De));
    assert_eq!(Language::from_locale_tag(":fr_FR:"), Some(Language::Fr));
    assert_eq!(Language::from_locale_tag("POSIX"), None);
    assert_eq!(Language::from_locale_tag("ko-KR"), None);
    assert_eq!(Language::from_locale_tag("."), None);
    let locales = |tags: &[&str]| tags.iter().map(|tag| tag.to_string()).collect::<Vec<_>>();
    assert_eq!(Language::from_locales(&locales(&["ko-KR", "it-IT"])), Language::It);
    assert_eq!(Language::from_locales(&locales(&["ko-KR"])), Language::EnUS);
    assert_eq!(Language::from_locales(&[]), Language::EnUS);
  }

  /// A config file as 0.3.0 writes it (`serde_json::to_writer_pretty` of
  /// its `Config`): every key, in its order, with values other than the
  /// defaults.
  const CONFIG_0_3_0: &str = r#"{
  "displayMode": "Dark",
  "theme": "Forest",
  "language": "ja",
  "integration": {
    "mkvToolNixPath": "/opt/mkvtoolnix",
    "betterMediaInfoPath": "/opt/bmi"
  },
  "profiles": [
    {
      "name": "Default",
      "videoTemplate": "{file_name}.{track_id}.{language}",
      "audioTemplate": "{file_name}.{track_id}.{language}",
      "subtitleTemplate": "{file_name}.{language}",
      "chaptersTemplate": "{file_name}.{track_id}",
      "attachmentsTemplate": "{track_name}",
      "selectVideo": false,
      "selectAudio": false,
      "selectSubtitle": true,
      "selectChapters": false,
      "selectAttachments": false,
      "videoLanguages": "",
      "audioLanguages": "",
      "subtitleLanguages": "eng, chi, spa, ger, fre, jpn",
      "defaultGroupMode": true
    },
    {
      "name": "Anime",
      "videoTemplate": "{file_name}.video",
      "audioTemplate": "{file_name}.{track_number}.{codec_name}",
      "subtitleTemplate": "{file_name}.{track_name}",
      "chaptersTemplate": "{file_name}.chapters",
      "attachmentsTemplate": "fonts/{track_name}",
      "selectVideo": true,
      "selectAudio": true,
      "selectSubtitle": false,
      "selectChapters": true,
      "selectAttachments": true,
      "videoLanguages": "jpn",
      "audioLanguages": "jpn, eng",
      "subtitleLanguages": "chi",
      "defaultGroupMode": false
    }
  ],
  "activeProfile": "Anime",
  "window": {
    "position": {
      "x": 120,
      "y": 80
    },
    "size": {
      "width": 1440,
      "height": 960
    }
  },
  "update": {
    "checkInterval": "Monthly",
    "lastChecked": 1800000000,
    "lastVersion": "0.3.1",
    "ignoreVersion": "0.3.1"
  }
}"#;

  /// The object keys at every depth, as `a.b.c` paths; arrays are not
  /// entered.
  fn key_paths(value: &serde_json::Value, prefix: &str, paths: &mut Vec<String>) {
    if let serde_json::Value::Object(map) = value {
      for (key, value) in map {
        let path = format!("{prefix}{key}");
        paths.push(path.clone());
        key_paths(value, &format!("{path}."), paths);
      }
    }
  }

  /// A 0.3.0 file loads as it is, and is written back with only
  /// `window.display` added: no value changes and no key goes.
  #[test]
  fn a_0_3_0_config_loads_and_round_trips_with_only_the_display_added() {
    let dir = ScratchDir::new("v030");
    std::fs::write(dir.file(), CONFIG_0_3_0).unwrap();
    let (config, problem) = Config::load(dir.file());
    assert_eq!(problem, None);
    assert_eq!(
      std::fs::read_to_string(dir.file()).unwrap(),
      CONFIG_0_3_0,
      "a complete file is not saved on load"
    );
    assert_eq!(config.active_profile, "Anime");
    assert_eq!(config.profiles[1].attachments_template, "fonts/{track_name}");
    let mut expected: Value = serde_json::from_str(CONFIG_0_3_0).unwrap();
    expected["window"]["display"] = Value::String(String::new());
    assert_eq!(serde_json::to_value(&config).unwrap(), expected);
    // The fixture covers every field of the config but the new one.
    let mut every = Vec::new();
    key_paths(&serde_json::to_value(Config::default()).unwrap(), "", &mut every);
    key_paths(
      &serde_json::to_value(ConfigProfile::default()).unwrap(),
      "profiles.",
      &mut every,
    );
    let mut written = Vec::new();
    let original: Value = serde_json::from_str(CONFIG_0_3_0).unwrap();
    key_paths(&original, "", &mut written);
    key_paths(&original["profiles"][0], "profiles.", &mut written);
    written.push("window.display".to_owned());
    every.sort();
    written.sort();
    assert_eq!(written, every);
  }

  #[test]
  fn a_named_profile_selects_subtitles_and_groups_only_as_default() {
    let default = ConfigProfile::named(ConfigProfile::DEFAULT_NAME);
    assert_eq!(
      serde_json::to_value(&default).unwrap(),
      serde_json::to_value(ConfigProfile::default()).unwrap()
    );
    let other = ConfigProfile::named("Anime");
    assert_eq!(other.name, "Anime");
    assert!(!other.select_video && !other.select_audio && !other.select_subtitle);
    assert!(!other.select_chapters && !other.select_attachments);
    assert!(!other.default_group_mode);
    assert_eq!(other.video_template, ConfigProfile::DEFAULT_TEMPLATE);
    assert_eq!(other.attachments_template, ConfigProfile::DEFAULT_TEMPLATE_NO_LANGUAGE);
    assert_eq!(other.subtitle_languages, ConfigProfile::DEFAULT_SUBTITLE_LANGUAGES);
  }

  /// A directory under the system temp dir, removed on drop.
  struct ScratchDir(PathBuf);

  impl ScratchDir {
    fn new(name: &str) -> Self {
      let path = std::env::temp_dir().join(format!("bme_config_{name}_{}", std::process::id()));
      let _ = std::fs::remove_dir_all(&path);
      std::fs::create_dir_all(&path).unwrap();
      Self(path)
    }

    fn file(&self) -> PathBuf {
      self.0.join("BatchMkvExtract.json")
    }
  }

  impl Drop for ScratchDir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  fn salvaged(json: &str) -> Config {
    match Config::parse(json.as_bytes()) {
      Parsed::Loaded(..) => panic!("{json} should not load as it is"),
      Parsed::Salvaged(config, _) => config,
    }
  }

  #[test]
  fn a_missing_config_file_is_created_with_the_defaults() {
    let dir = ScratchDir::new("missing_file");
    let config = Config::new_at(dir.file());
    assert_eq!(config.theme, Theme::default());
    let (saved, problem) = Config::load(dir.file());
    assert_eq!((saved.theme, problem), (Theme::default(), None), "saved");
    let unsaved = Config::new_at(dir.0.join("gone").join("BatchMkvExtract.json"));
    assert_eq!(unsaved.profiles.len(), 1, "the defaults even when it cannot be saved");
  }

  #[test]
  fn a_broken_file_read_at_startup_is_reported() {
    let dir = ScratchDir::new("startup");
    std::fs::write(dir.file(), "not json").unwrap();
    let config = Config::new_at(dir.file());
    assert_eq!(config.theme, Theme::default());
    let problem = unreadable_config().expect("the problem is kept");
    assert!(problem.reason.contains("not valid JSON"), "{}", problem.reason);
  }

  #[test]
  fn a_truncated_config_file_is_kept_aside_and_replaced_by_defaults() {
    let dir = ScratchDir::new("truncated");
    std::fs::write(dir.file(), "{\"theme\": \"Rose\", \"wind").unwrap();
    let (config, problem) = Config::load(dir.file());
    assert_eq!(config.theme, Theme::default());
    let problem = problem.expect("the problem is reported");
    assert!(problem.reason.contains("not valid JSON"), "{}", problem.reason);
    assert_eq!(
      problem.backup.as_deref(),
      Some(dir.0.join("BatchMkvExtract.json.bad").as_path())
    );
    assert_eq!(
      std::fs::read_to_string(dir.0.join("BatchMkvExtract.json.bad")).unwrap(),
      "{\"theme\": \"Rose\", \"wind"
    );
    let rewritten: Value = serde_json::from_slice(&std::fs::read(dir.file()).unwrap()).unwrap();
    assert_eq!(rewritten["theme"], "Ocean");
  }

  /// Two instances of the app read the same broken file. The first keeps it
  /// as a backup, saves the repaired config, and then the user's newer
  /// settings. The second, holding the bytes it read before all that, keeps
  /// them under a new backup name, never replaces the first backup, and
  /// finds the file replaced: it loads the newer settings instead of saving
  /// its own repair over them.
  #[test]
  fn a_late_repair_replaces_neither_a_backup_nor_newer_settings() {
    let dir = ScratchDir::new("backups");
    let broken = b"{\"theme\": \"Rose\", \"wind".to_vec();
    std::fs::write(dir.file(), &broken).unwrap();
    let (repaired, first) = Config::load(dir.file());
    assert_eq!(repaired.theme, Theme::default());
    let newer = Config {
      theme: Theme::Forest,
      ..repaired
    };
    newer.save(&dir.file()).unwrap();
    let (config, second) = Config::load_read(dir.file(), Ok(broken.clone()));
    assert_eq!((config.theme, second), (Theme::Forest, None), "the newer settings");
    let backups = [
      dir.0.join("BatchMkvExtract.json.bad"),
      dir.0.join("BatchMkvExtract.json.2.bad"),
    ];
    assert_eq!(first.unwrap().backup.as_ref(), Some(&backups[0]));
    for backup in &backups {
      assert_eq!(std::fs::read(backup).unwrap(), broken, "{}", backup.display());
    }
    let (config, problem) = Config::load(dir.file());
    assert_eq!((config.theme, problem), (Theme::Forest, None), "still in the file");
  }

  /// The same for a file that loads but is completed and saved at once (no
  /// `language` yet, as versions before 0.3.0 wrote it): a second instance
  /// that read it before the first saved newer settings loads those instead
  /// of saving its own completion over them.
  #[test]
  fn a_late_completion_does_not_replace_newer_settings() {
    let dir = ScratchDir::new("completion");
    let old = br#"{"theme": "Rose"}"#.to_vec();
    std::fs::write(dir.file(), &old).unwrap();
    let (completed, problem) = Config::load(dir.file());
    assert_eq!((completed.theme, problem), (Theme::Rose, None));
    let newer = Config {
      theme: Theme::Forest,
      ..completed
    };
    newer.save(&dir.file()).unwrap();
    let (config, problem) = Config::load_read(dir.file(), Ok(old));
    assert_eq!((config.theme, problem), (Theme::Forest, None), "the newer settings");
    let (config, _) = Config::load(dir.file());
    assert_eq!(config.theme, Theme::Forest, "still in the file");
  }

  /// A file that cannot be read is moved to its backup name.
  #[cfg(unix)]
  #[test]
  fn an_unreadable_config_file_is_moved_to_its_backup() {
    use std::os::unix::fs::PermissionsExt;
    let dir = ScratchDir::new("unreadable");
    std::fs::write(dir.file(), "held by another program").unwrap();
    std::fs::write(dir.0.join("BatchMkvExtract.json.bad"), "an older backup").unwrap();
    let mode = |path: &Path, mode: u32| std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
    mode(&dir.file(), 0o000);
    if std::fs::read(dir.file()).is_ok() {
      // Root reads every file.
      return;
    }
    let (_, problem) = Config::load(dir.file());
    let problem = problem.expect("the problem is reported");
    assert!(problem.reason.contains("could not be read"), "{}", problem.reason);
    let backup = dir.0.join("BatchMkvExtract.json.2.bad");
    assert_eq!(problem.backup.as_deref(), Some(backup.as_path()));
    mode(&backup, 0o600);
    assert_eq!(std::fs::read_to_string(backup).unwrap(), "held by another program");
    assert_eq!(
      std::fs::read_to_string(dir.0.join("BatchMkvExtract.json.bad")).unwrap(),
      "an older backup"
    );
    let (_, problem) = Config::load(dir.file());
    assert_eq!(problem, None, "the defaults were saved in its place");
  }

  /// No backup name is free: the problem is still reported, without a
  /// backup, and the file is left where it is.
  #[test]
  fn a_broken_file_without_a_free_backup_name_is_left_in_place() {
    let dir = ScratchDir::new("no_backup");
    std::fs::write(dir.file(), "broken").unwrap();
    std::fs::write(dir.0.join("BatchMkvExtract.json.bad"), "").unwrap();
    for n in 2..=MAX_BACKUPS {
      std::fs::write(dir.0.join(format!("BatchMkvExtract.json.{n}.bad")), "").unwrap();
    }
    let (config, problem) = Config::load(dir.file());
    assert_eq!(config.theme, Theme::default());
    assert_eq!(problem.expect("the problem is reported").backup, None);
    assert_eq!(std::fs::read_to_string(dir.file()).unwrap(), "broken");
  }

  /// A read that failed, and the file can be read now: another instance has
  /// saved its settings there since. They are loaded where they are, not
  /// moved aside and replaced by the defaults.
  #[test]
  fn settings_saved_after_a_failed_read_are_loaded_where_they_are() {
    let dir = ScratchDir::new("read-again");
    let newer = Config {
      theme: Theme::Forest,
      ..Config::default()
    };
    newer.save(&dir.file()).unwrap();
    let saved = std::fs::read(dir.file()).unwrap();
    let unreadable = Err(std::io::Error::other("unreadable"));
    let (config, problem) = Config::load_read(dir.file(), unreadable);
    assert_eq!((config.theme, problem), (Theme::Forest, None), "the newer settings");
    assert_eq!(std::fs::read(dir.file()).unwrap(), saved, "left where they are");
    assert!(!dir.0.join("BatchMkvExtract.json.bad").exists(), "nothing moved aside");
  }

  /// A read that failed of a file that is gone by now: there is nothing to
  /// keep aside, and the defaults are used.
  #[test]
  fn a_failed_read_of_a_file_that_is_gone_uses_the_defaults() {
    let dir = ScratchDir::new("gone");
    let unreadable = Err(std::io::Error::other("unreadable"));
    let (config, problem) = Config::load_read(dir.file(), unreadable);
    assert_eq!(config.theme, Theme::default());
    let problem = problem.expect("the problem is reported");
    assert!(problem.reason.contains("could not be read"), "{}", problem.reason);
    assert_eq!(problem.backup, None);
  }

  #[test]
  fn an_unknown_or_mistyped_value_costs_only_that_value() {
    let config = salvaged(
      r#"{"theme": "Rose", "displayMode": "Sepia", "language": "ko",
          "window": {"size": {"width": "1200", "height": 700}, "position": {"x": 5, "y": 6}},
          "update": {"checkInterval": "Hourly", "ignoreVersion": "2.0.0"},
          "integration": {"mkvToolNixPath": 7, "betterMediaInfoPath": "/opt/bmi"}}"#,
    );
    assert_eq!(config.theme, Theme::Rose);
    assert_eq!(config.display_mode, DisplayMode::Auto);
    assert_eq!((config.window.position.x, config.window.position.y), (5, 6));
    assert_eq!(config.window.size.height, 700);
    assert_eq!(config.window.size.width, ConfigWindowSize::default().width);
    assert_eq!(config.update.check_interval, UpdateCheckInterval::Weekly);
    assert_eq!(config.update.ignore_version, "2.0.0");
    assert_eq!(
      config.integration.mkv_toolnix_path,
      ConfigIntegration::default().mkv_toolnix_path
    );
    assert_eq!(config.integration.better_media_info_path, "/opt/bmi");
  }

  #[test]
  fn a_broken_profile_is_dropped_and_the_others_are_kept() {
    let config = salvaged(
      r#"{"profiles": [{"name": "Default"}, "not a profile", {"name": "Anime"}],
          "activeProfile": "Anime"}"#,
    );
    let names: Vec<&str> = config.profiles.iter().map(|profile| profile.name.as_str()).collect();
    assert_eq!(names, ["Default", "Anime"]);
    assert_eq!(config.active_profile, "Anime");
  }

  /// One invalid value in a profile costs only that value: the profile keeps
  /// its name, its other values and its place as the active profile.
  #[test]
  fn an_invalid_profile_value_costs_only_that_value() {
    let config = salvaged(
      r#"{"profiles": [{"name": "Default"},
            {"name": "Anime", "selectVideo": "yes", "audioTemplate": "{file_name}.a",
             "audioLanguages": "jpn", "selectAudio": true, "defaultGroupMode": false}],
          "activeProfile": "Anime"}"#,
    );
    let names: Vec<&str> = config.profiles.iter().map(|profile| profile.name.as_str()).collect();
    assert_eq!(names, ["Default", "Anime"]);
    assert_eq!(config.active_profile, "Anime");
    let anime = &config.profiles[1];
    assert!(!anime.select_video, "the invalid value went back to its default");
    assert_eq!(anime.audio_template, "{file_name}.a");
    assert_eq!(anime.audio_languages, "jpn");
    assert!(anime.select_audio);
    assert!(!anime.default_group_mode);
  }

  #[test]
  fn a_profile_list_that_is_not_a_list_keeps_the_default_profile() {
    let config = salvaged(r#"{"profiles": {"name": "Anime"}, "theme": "Sand"}"#);
    let names: Vec<&str> = config.profiles.iter().map(|profile| profile.name.as_str()).collect();
    assert_eq!(names, ["Default"]);
    assert_eq!(config.theme, Theme::Sand);
  }

  #[test]
  fn a_config_file_that_is_not_an_object_loads_the_defaults() {
    let config = salvaged("[1, 2]");
    assert_eq!(config.profiles.len(), 1);
    assert_eq!(config.theme, Theme::default());
  }

  #[test]
  fn a_valid_config_file_loads_without_a_problem_and_without_a_backup() {
    let dir = ScratchDir::new("valid");
    std::fs::write(dir.file(), r#"{"theme": "Forest", "language": "fr"}"#).unwrap();
    let (config, problem) = Config::load(dir.file());
    assert_eq!(
      (config.theme, config.language, problem),
      (Theme::Forest, Language::Fr, None)
    );
    assert!(!dir.0.join("BatchMkvExtract.json.bad").exists());
  }

  #[test]
  fn saving_replaces_the_file_and_leaves_no_temporary_file() {
    let dir = ScratchDir::new("save");
    std::fs::write(dir.file(), "old contents").unwrap();
    let config = Config {
      theme: Theme::Amber,
      ..Config::default()
    };
    config.save(&dir.file()).unwrap();
    let (loaded, problem) = Config::load(dir.file());
    assert_eq!((loaded.theme, problem), (Theme::Amber, None));
    let names: Vec<String> = std::fs::read_dir(&dir.0)
      .unwrap()
      .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
      .collect();
    assert_eq!(names, ["BatchMkvExtract.json"]);
  }

  /// Two instances of the app may save at the same time; every save must
  /// succeed and leave a whole config.
  #[test]
  fn concurrent_saves_each_write_a_temporary_file_of_their_own() {
    let dir = ScratchDir::new("concurrent");
    let themes = [Theme::Amber, Theme::Rose, Theme::Forest, Theme::Indigo];
    std::thread::scope(|scope| {
      for theme in themes {
        let file = dir.file();
        scope.spawn(move || {
          let config = Config {
            theme,
            ..Config::default()
          };
          for _ in 0..25 {
            config.save(&file).unwrap();
          }
        });
      }
    });
    let (loaded, problem) = Config::load(dir.file());
    assert_eq!(problem, None);
    assert!(themes.contains(&loaded.theme));
    let names: Vec<String> = std::fs::read_dir(&dir.0)
      .unwrap()
      .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
      .collect();
    assert_eq!(names, ["BatchMkvExtract.json"]);
  }

  /// Two instances that loaded one file and save their own changes keep
  /// each other's: a profile edited in one survives the window size the
  /// other saves later. The same value changed in both is the later save's.
  #[test]
  fn two_instances_keep_each_others_settings() {
    let dir = ScratchDir::new("merged");
    let path = dir.file();
    let loaded = Config::default();
    loaded.save(&path).unwrap();
    let mut first = loaded.clone();
    first.profiles[0].audio_template = "{file_name}.first".into();
    first.window.size.width = 700;
    save_merged(&path, &loaded, &first).unwrap();
    let mut second = loaded.clone();
    second.window.size.width = 800;
    second.update.last_checked = 42;
    let written = save_merged(&path, &loaded, &second).unwrap();
    let on_disk = Config::load(path.clone()).0;
    for config in [&written, &on_disk] {
      assert_eq!(
        config.profiles[0].audio_template, "{file_name}.first",
        "the first's profile"
      );
      assert_eq!(config.window.size.width, 800, "the later window size");
      assert_eq!(config.update.last_checked, 42);
    }
    // The second saves again from what it saved: the profile stays.
    let mut again = second.clone();
    again.window.size.height = 600;
    save_merged(&path, &second, &again).unwrap();
    let on_disk = Config::load(path.clone()).0;
    assert_eq!(on_disk.profiles[0].audio_template, "{file_name}.first");
    assert_eq!((on_disk.window.size.width, on_disk.window.size.height), (800, 600));
    // A file that is no config is replaced by what this instance has.
    std::fs::write(&path, b"{ broken").unwrap();
    let written = save_merged(&path, &again, &again).unwrap();
    assert_eq!(
      written.profiles[0].audio_template,
      ConfigProfile::default().audio_template
    );
    assert!(path.with_file_name("BatchMkvExtract.json.lock").exists());
  }

  #[test]
  fn merging_applies_only_what_changed() {
    let base = serde_json::json!({"a": 1, "b": {"c": 1, "d": [1]}, "e": "x"});
    let ours = serde_json::json!({"a": 1, "b": {"c": 2, "d": [1]}, "e": "x", "f": true});
    let mut theirs = serde_json::json!({"a": 5, "b": {"c": 1, "d": [7]}, "e": "y"});
    merge_values(&base, &ours, &mut theirs);
    assert_eq!(
      theirs,
      serde_json::json!({"a": 5, "b": {"c": 2, "d": [7]}, "e": "y", "f": true})
    );
    // A list changed here replaces theirs whole.
    let mut theirs = serde_json::json!([3]);
    merge_values(&serde_json::json!([1]), &serde_json::json!([2]), &mut theirs);
    assert_eq!(theirs, serde_json::json!([2]));
  }

  #[test]
  fn saving_into_a_missing_directory_returns_the_error() {
    let dir = ScratchDir::new("missing");
    let error = Config::default().save(&dir.0.join("gone").join("BatchMkvExtract.json"));
    assert!(error.is_err());
  }

  /// The temporary file is there, but it cannot replace the config: a
  /// directory has the config's name.
  #[test]
  fn a_save_that_cannot_replace_the_file_removes_its_temporary_file() {
    let dir = ScratchDir::new("rename");
    std::fs::create_dir_all(dir.file().join("in the way")).unwrap();
    let error = Config::default().save(&dir.file()).unwrap_err();
    assert!(format!("{error:#}").contains("Couldn't replace"), "{error:#}");
    let names: Vec<String> = std::fs::read_dir(&dir.0)
      .unwrap()
      .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
      .collect();
    assert_eq!(names, ["BatchMkvExtract.json"], "only the directory in the way");
  }

  #[test]
  fn install_roots_match_whole_path_components_case_insensitively() {
    assert!(is_within_directory(
      r"C:\Program Files\BatchMkvExtract",
      r"c:\program files"
    ));
    assert!(is_within_directory(r"C:\Program Files", r"C:\Program Files\"));
    assert!(is_within_directory(
      r"C:\Users\me\AppData\Local\BatchMkvExtract",
      r"C:/Users/me/AppData/Local"
    ));
    assert!(!is_within_directory(
      r"C:\Program Files Portable\BatchMkvExtract",
      r"C:\Program Files"
    ));
    assert!(!is_within_directory(r"D:\Tools", r"C:\Program Files"));
    assert!(!is_within_directory(r"C:\Program Files", ""));
  }

  #[test]
  fn profiles_are_made_unique_with_a_default_and_a_valid_active_one() {
    let dir = ScratchDir::new("profiles");
    std::fs::write(
      dir.file(),
      r#"{"language": "en-US", "activeProfile": "Gone",
          "profiles": [{"name": "Anime"}, {"name": "Anime"}, {"name": "Anime (2)"}, {"name": "Anime"}]}"#,
    )
    .unwrap();
    let (config, problem) = Config::load(dir.file());
    assert_eq!(problem, None);
    let names: Vec<&str> = config.profiles.iter().map(|profile| profile.name.as_str()).collect();
    assert_eq!(names, ["Default", "Anime", "Anime (3)", "Anime (2)", "Anime (4)"]);
    assert_eq!(config.active_profile, "Default");
    let (saved, _) = Config::load(dir.file());
    assert_eq!(saved.profiles.len(), 5, "the completed list was saved");
    let mut fine = Config::default();
    assert!(!fine.normalize_profiles(), "a default config needs nothing");
  }

  #[test]
  fn the_config_directory_follows_the_platform_rules() {
    fn exe_dir() -> PathBuf {
      PathBuf::from("/opt/app")
    }
    let linux = |xdg: Option<&str>, home: Option<&str>| {
      Config::linux_config_dir(xdg.map(str::to_owned), home.map(str::to_owned), exe_dir)
    };
    assert_eq!(linux(Some("/xdg"), Some("/home/me")), Path::new("/xdg").join(APP_NAME));
    assert_eq!(
      linux(Some(""), Some("/home/me")),
      Path::new("/home/me").join(".config").join(APP_NAME)
    );
    assert_eq!(linux(None, None), exe_dir());
    assert_eq!(
      Config::macos_config_dir(Some("/Users/me".to_owned()), exe_dir),
      Path::new("/Users/me")
        .join("Library")
        .join("Application Support")
        .join(APP_NAME)
    );
    assert_eq!(Config::macos_config_dir(None, exe_dir), exe_dir());
    assert!(Config::get_exe_dir().is_dir());
    let dir = config_dir();
    assert!(dir.is_dir());
    assert!(Config::get_path_buf().ends_with(format!("{APP_NAME}.json")));
  }
}
