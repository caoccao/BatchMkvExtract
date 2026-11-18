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

//! The persisted configuration (`bme_core::config::Config`) and the profile
//! edits: add, delete, switch, reset and edit the active profile.
//!
//! Group Mode is session state seeded by the active profile's
//! `defaultGroupMode`. Every profile operation that sets it in 0.3.0
//! (`store.ts`) emits [`ConfigEvent::GroupMode`] with the value, also when the
//! value did not change, so it overrides a toggle made during the session.

use std::sync::{Arc, Mutex};

use bme_core::config::{Config, ConfigProfile};
use gpui_kit::{AppContext, Context, EventEmitter, Subscription};

/// The active profile, else the first one.
pub fn active_profile(config: &Config) -> Option<&ConfigProfile> {
  active_profile_index(config).map(|index| &config.profiles[index])
}

/// The index of [`active_profile`].
fn active_profile_index(config: &Config) -> Option<usize> {
  config
    .profiles
    .iter()
    .position(|profile| profile.name == config.active_profile)
    .or_else(|| (!config.profiles.is_empty()).then_some(0))
}

type WriteFn = dyn Fn(Config) -> anyhow::Result<()> + Send + Sync;

/// Where configuration writes go: the config file in the app, a recorder in
/// tests. Writes carry a revision and run one at a time, and a write older
/// than the last one written is dropped, so background writes that finish out
/// of order never put an older config on disk. Only a write that succeeded
/// counts as written: after a failure (a full disk, a folder that is not
/// writable for a moment) the next write, or the flush at quit, tries again.
#[derive(Clone)]
pub struct Persistence {
  write: Arc<WriteFn>,
  last_written: Arc<Mutex<u64>>,
}

impl Persistence {
  /// A write that cannot fail (a recorder in tests).
  pub fn new(write: impl Fn(Config) + Send + Sync + 'static) -> Self {
    Self::fallible(move |config| {
      write(config);
      Ok(())
    })
  }

  /// A write that reports its failure.
  pub fn fallible(write: impl Fn(Config) -> anyhow::Result<()> + Send + Sync + 'static) -> Self {
    Self {
      write: Arc::new(write),
      last_written: Arc::new(Mutex::new(0)),
    }
  }

  /// Writes through `bme_core::config::set_config`, which also updates the
  /// in-memory config the core reads.
  pub fn disk() -> Self {
    Self::fallible(bme_core::config::set_config)
  }

  /// Writes `config` unless a newer revision has been written already; the
  /// error of a write that failed.
  fn write(&self, revision: u64, config: Config) -> Result<(), String> {
    let mut last_written = self
      .last_written
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if revision <= *last_written {
      return Ok(());
    }
    match (self.write)(config) {
      Ok(()) => {
        *last_written = revision;
        Ok(())
      }
      Err(error) => {
        log::error!("Failed to save the config: {error}");
        Err(error.to_string())
      }
    }
  }
}

/// What a [`ConfigStore`] edit means beyond the new config.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigEvent {
  /// A profile operation set Group Mode to this value (the profile's
  /// `defaultGroupMode`).
  GroupMode(bool),
  /// Saving the config failed, after the saves before it succeeded; the
  /// error. The next edit and the quit try again.
  SaveFailed(String),
}

/// Holds the configuration the UI shows and edits.
pub struct ConfigStore {
  config: Config,
  revision: u64,
  persistence: Persistence,
  /// The revision of the last save whose outcome arrived, and whether it
  /// failed.
  last_saved: (u64, bool),
  _flush_on_quit: Subscription,
}

impl EventEmitter<ConfigEvent> for ConfigStore {}

impl ConfigStore {
  pub fn new(config: Config, persistence: Persistence, cx: &mut Context<Self>) -> Self {
    Self {
      config,
      revision: 0,
      persistence,
      last_saved: (0, false),
      // A write still pending at quit, or one that failed, is finished
      // synchronously; a failure is in the log.
      _flush_on_quit: cx.on_app_quit(|this, _| {
        let _ = this.flush();
        async {}
      }),
    }
  }

  pub fn config(&self) -> &Config {
    &self.config
  }

  /// Applies `edit` at once and persists the result in the background. The
  /// result is never read back: an older write finishing late must not revert
  /// newer edits.
  pub fn update(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut Config)) {
    edit(&mut self.config);
    self.revision += 1;
    let (revision, config, persistence) = (self.revision, self.config.clone(), self.persistence.clone());
    let saved = cx.background_spawn(async move { persistence.write(revision, config) });
    cx.spawn(async move |this, cx| {
      let saved = saved.await;
      let _ = this.update(cx, |store, cx| store.saved(revision, saved, cx));
    })
    .detach();
    cx.notify();
  }

  /// The outcome of the save of `revision`. A failure is emitted once, when
  /// the save before it succeeded, so a field typed into while the disk is
  /// full says so once; an outcome older than one already seen is dropped.
  fn saved(&mut self, revision: u64, saved: Result<(), String>, cx: &mut Context<Self>) {
    if revision < self.last_saved.0 {
      return;
    }
    let failed_before = self.last_saved.1;
    self.last_saved = (revision, saved.is_err());
    if let Err(error) = saved
      && !failed_before
    {
      cx.emit(ConfigEvent::SaveFailed(error));
    }
  }

  /// Edits the profile the Settings show: the active one, else the first. An
  /// edit of its `defaultGroupMode` also sets Group Mode, as an edit of that
  /// field did in 0.3.0.
  pub fn update_active_profile(&mut self, cx: &mut Context<Self>, edit: impl FnOnce(&mut ConfigProfile)) {
    let Some(index) = active_profile_index(&self.config) else {
      return;
    };
    let before = self.config.profiles[index].default_group_mode;
    self.update(cx, |config| edit(&mut config.profiles[index]));
    let after = self.config.profiles[index].default_group_mode;
    if after != before {
      cx.emit(ConfigEvent::GroupMode(after));
    }
  }

  /// Whether `name` (trimmed) can become a new profile: not empty and not
  /// taken.
  pub fn can_add_profile(&self, name: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && !self.config.profiles.iter().any(|profile| profile.name == name)
  }

  /// Adds a fresh profile called `name` (trimmed) and makes it active; `false`
  /// when [`Self::can_add_profile`] refuses it.
  pub fn add_profile(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
    if !self.can_add_profile(name) {
      return false;
    }
    let profile = ConfigProfile::named(name.trim());
    let group_mode = profile.default_group_mode;
    self.update(cx, |config| {
      config.active_profile = profile.name.clone();
      config.profiles.push(profile);
    });
    cx.emit(ConfigEvent::GroupMode(group_mode));
    true
  }

  /// Whether the active profile can be deleted: every one but Default.
  pub fn can_delete_active_profile(&self) -> bool {
    self.config.active_profile != ConfigProfile::DEFAULT_NAME
  }

  /// Deletes the active profile; Default becomes active and sets Group Mode
  /// (off if there is no Default profile). The Default profile stays.
  pub fn delete_active_profile(&mut self, cx: &mut Context<Self>) {
    if !self.can_delete_active_profile() {
      return;
    }
    self.update(cx, |config| {
      let name = std::mem::replace(&mut config.active_profile, ConfigProfile::DEFAULT_NAME.to_string());
      config.profiles.retain(|profile| profile.name != name);
    });
    let group_mode = self
      .config
      .profiles
      .iter()
      .find(|profile| profile.name == ConfigProfile::DEFAULT_NAME)
      .is_some_and(|profile| profile.default_group_mode);
    cx.emit(ConfigEvent::GroupMode(group_mode));
  }

  /// Makes the profile called `name` active, if there is one, and sets Group
  /// Mode from it, also when it was active already.
  pub fn set_active_profile(&mut self, name: &str, cx: &mut Context<Self>) {
    let Some(group_mode) = self
      .config
      .profiles
      .iter()
      .find(|profile| profile.name == name)
      .map(|profile| profile.default_group_mode)
    else {
      return;
    };
    if self.config.active_profile != name {
      let name = name.to_string();
      self.update(cx, |config| config.active_profile = name);
    }
    cx.emit(ConfigEvent::GroupMode(group_mode));
  }

  /// Puts the active profile back to a fresh profile of the same name (0.3.0's
  /// Reset to Defaults) and sets Group Mode from it.
  pub fn reset_active_profile(&mut self, cx: &mut Context<Self>) {
    let Some(index) = active_profile_index(&self.config) else {
      return;
    };
    let profile = ConfigProfile::named(&self.config.profiles[index].name);
    let group_mode = profile.default_group_mode;
    self.update(cx, |config| config.profiles[index] = profile);
    cx.emit(ConfigEvent::GroupMode(group_mode));
  }

  /// Writes the current config now unless that revision is on disk already,
  /// also after its background write failed; the error of a failed write.
  pub fn flush(&self) -> Result<(), String> {
    self.persistence.write(self.revision, self.config.clone())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bme_core::config::{DisplayMode, Language};
  use core::prelude::v1::test;
  use gpui_kit::{Entity, TestAppContext};

  fn recorder() -> (Persistence, Arc<Mutex<Vec<Config>>>) {
    let written = Arc::new(Mutex::new(Vec::new()));
    let sink = written.clone();
    (
      Persistence::new(move |config| sink.lock().unwrap().push(config)),
      written,
    )
  }

  #[gpui_kit::test]
  fn update_applies_at_once_and_persists_in_the_background(cx: &mut TestAppContext) {
    let (persistence, written) = recorder();
    let store = cx.new(|cx| ConfigStore::new(Config::default(), persistence, cx));
    store.update(cx, |store, cx| {
      store.update(cx, |config| config.display_mode = DisplayMode::Dark)
    });
    assert_eq!(
      store.read_with(cx, |store, _| store.config().display_mode),
      DisplayMode::Dark
    );
    cx.run_until_parked();
    let written = written.lock().unwrap();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].display_mode, DisplayMode::Dark);
  }

  #[gpui_kit::test]
  fn rapid_updates_keep_the_newest_config(cx: &mut TestAppContext) {
    let (persistence, written) = recorder();
    let store = cx.new(|cx| ConfigStore::new(Config::default(), persistence, cx));
    store.update(cx, |store, cx| {
      store.update(cx, |config| config.language = Language::De);
      store.update(cx, |config| config.language = Language::Fr);
      store.update(cx, |config| config.display_mode = DisplayMode::Light);
    });
    cx.run_until_parked();
    let written = written.lock().unwrap();
    let last = written.last().expect("a write");
    assert_eq!(last.language, Language::Fr);
    assert_eq!(last.display_mode, DisplayMode::Light);
    assert_eq!(store.read_with(cx, |store, _| store.config().language), Language::Fr);
  }

  #[test]
  fn the_active_profile_falls_back_to_the_first() {
    let mut config = Config::default();
    let mut other = config.profiles[0].clone();
    other.name = "Other".to_string();
    config.profiles.push(other);
    config.active_profile = "Other".to_string();
    assert_eq!(
      active_profile(&config).map(|profile| profile.name.as_str()),
      Some("Other")
    );
    config.active_profile = "Missing".to_string();
    assert_eq!(
      active_profile(&config).map(|profile| profile.name.as_str()),
      Some(config.profiles[0].name.as_str())
    );
    config.profiles.clear();
    assert!(active_profile(&config).is_none());
  }

  #[test]
  fn a_write_older_than_the_last_one_is_dropped() {
    let (persistence, written) = recorder();
    let newer = Config {
      language: Language::Ja,
      ..Config::default()
    };
    assert_eq!(persistence.write(2, newer), Ok(()));
    assert_eq!(persistence.write(1, Config::default()), Ok(()));
    assert_eq!(persistence.write(2, Config::default()), Ok(()));
    let written = written.lock().unwrap();
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].language, Language::Ja);
  }

  #[gpui_kit::test]
  fn flush_writes_only_what_is_not_on_disk(cx: &mut TestAppContext) {
    let (persistence, written) = recorder();
    let store = cx.new(|cx| ConfigStore::new(Config::default(), persistence, cx));
    store.update(cx, |store, cx| {
      store.update(cx, |config| config.language = Language::It)
    });
    assert_eq!(store.read_with(cx, |store, _| store.flush()), Ok(()));
    assert_eq!(written.lock().unwrap().len(), 1);
    cx.run_until_parked();
    assert_eq!(store.read_with(cx, |store, _| store.flush()), Ok(()));
    assert_eq!(
      written.lock().unwrap().len(),
      1,
      "the background write of the same revision is dropped"
    );
  }

  fn store(cx: &mut TestAppContext) -> (Entity<ConfigStore>, Arc<Mutex<Vec<Config>>>) {
    let (persistence, written) = recorder();
    (
      cx.new(|cx| ConfigStore::new(Config::default(), persistence, cx)),
      written,
    )
  }

  fn names(store: &Entity<ConfigStore>, cx: &mut TestAppContext) -> (Vec<String>, String) {
    store.read_with(cx, |store, _| {
      let config = store.config();
      (
        config.profiles.iter().map(|profile| profile.name.clone()).collect(),
        config.active_profile.clone(),
      )
    })
  }

  #[gpui_kit::test]
  fn a_new_profile_is_fresh_and_becomes_active(cx: &mut TestAppContext) {
    let (store, written) = store(cx);
    assert!(store.update(cx, |store, cx| store.add_profile("  Anime ", cx)));
    assert_eq!(
      names(&store, cx),
      (vec!["Default".into(), "Anime".into()], "Anime".into())
    );
    let anime = store.read_with(cx, |store, _| store.config().profiles[1].clone());
    assert!(!anime.select_audio && !anime.select_subtitle && !anime.select_chapters);
    // Empty and taken names are refused.
    for name in ["", "   ", "Anime", "Default"] {
      assert!(!store.read_with(cx, |store, _| store.can_add_profile(name)), "{name:?}");
      assert!(!store.update(cx, |store, cx| store.add_profile(name, cx)));
    }
    cx.run_until_parked();
    assert_eq!(written.lock().unwrap().last().unwrap().active_profile, "Anime");
  }

  #[gpui_kit::test]
  fn deleting_the_active_profile_activates_default_and_default_stays(cx: &mut TestAppContext) {
    let (store, _) = store(cx);
    store.update(cx, |store, cx| {
      store.add_profile("Anime", cx);
      store.add_profile("Movies", cx);
      store.set_active_profile("Anime", cx);
    });
    assert!(store.read_with(cx, |store, _| store.can_delete_active_profile()));
    store.update(cx, |store, cx| store.delete_active_profile(cx));
    assert_eq!(
      names(&store, cx),
      (vec!["Default".into(), "Movies".into()], "Default".into())
    );
    assert!(!store.read_with(cx, |store, _| store.can_delete_active_profile()));
    store.update(cx, |store, cx| store.delete_active_profile(cx));
    assert_eq!(names(&store, cx).0, vec!["Default".to_string(), "Movies".to_string()]);
    // An unknown name is not activated.
    store.update(cx, |store, cx| store.set_active_profile("Missing", cx));
    assert_eq!(names(&store, cx).1, "Default");
  }

  #[gpui_kit::test]
  fn reset_puts_the_active_profile_back_to_its_template(cx: &mut TestAppContext) {
    let (store, _) = store(cx);
    store.update(cx, |store, cx| {
      store.add_profile("Anime", cx);
      store.update_active_profile(cx, |profile| {
        profile.select_video = true;
        profile.audio_languages = "jpn".into();
      });
    });
    let edited = store.read_with(cx, |store, _| store.config().profiles[1].clone());
    assert!(edited.select_video);
    assert_eq!(edited.audio_languages, "jpn");
    store.update(cx, |store, cx| store.reset_active_profile(cx));
    let reset = store.read_with(cx, |store, _| store.config().profiles[1].clone());
    assert_eq!(
      serde_json::to_value(&reset).unwrap(),
      serde_json::to_value(ConfigProfile::named("Anime")).unwrap()
    );
    // The Default profile was not touched.
    let default = store.read_with(cx, |store, _| store.config().profiles[0].clone());
    assert_eq!(
      serde_json::to_value(&default).unwrap(),
      serde_json::to_value(ConfigProfile::default()).unwrap()
    );
  }

  /// A save that fails is not taken as written: the next save or the flush
  /// at quit writes that config again. The failure is emitted once until a
  /// save succeeds.
  #[gpui_kit::test]
  fn a_failed_save_is_reported_once_and_tried_again(cx: &mut TestAppContext) {
    let failing = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let written = Arc::new(Mutex::new(Vec::<Config>::new()));
    let (fails, sink) = (failing.clone(), written.clone());
    let persistence = Persistence::fallible(move |config| {
      if fails.load(std::sync::atomic::Ordering::SeqCst) {
        anyhow::bail!("No space left on device");
      }
      sink.lock().unwrap().push(config);
      Ok(())
    });
    let store = cx.new(|cx| ConfigStore::new(Config::default(), persistence, cx));
    let failures = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = failures.clone();
    let _subscription = cx.update(|cx| {
      cx.subscribe(&store, move |_, event: &ConfigEvent, _| {
        if let ConfigEvent::SaveFailed(error) = event {
          seen.lock().unwrap().push(error.clone());
        }
      })
    });
    let edit = |cx: &mut TestAppContext, mode: DisplayMode| {
      store.update(cx, |store, cx| store.update(cx, |config| config.display_mode = mode));
      cx.run_until_parked();
    };
    edit(cx, DisplayMode::Dark);
    edit(cx, DisplayMode::Light);
    assert_eq!(*failures.lock().unwrap(), ["No space left on device"], "once");
    assert!(written.lock().unwrap().is_empty());
    failing.store(false, std::sync::atomic::Ordering::SeqCst);
    // The flush at quit writes the revision whose save failed.
    assert_eq!(store.read_with(cx, |store, _| store.flush()), Ok(()));
    assert_eq!(written.lock().unwrap().len(), 1);
    assert_eq!(written.lock().unwrap()[0].display_mode, DisplayMode::Light);
    assert_eq!(
      store.read_with(cx, |store, _| store.flush()),
      Ok(()),
      "nothing to write"
    );
    assert_eq!(written.lock().unwrap().len(), 1);
    // After a success, a new failure is said again.
    edit(cx, DisplayMode::Dark);
    failing.store(true, std::sync::atomic::Ordering::SeqCst);
    edit(cx, DisplayMode::Light);
    assert_eq!(failures.lock().unwrap().len(), 2);
  }

  #[gpui_kit::test]
  fn edits_go_to_the_first_profile_when_the_active_name_matches_none(cx: &mut TestAppContext) {
    let (store, _) = store(cx);
    store.update(cx, |store, cx| {
      store.update(cx, |config| config.active_profile = "Missing".into());
      store.update_active_profile(cx, |profile| profile.select_video = true);
    });
    assert!(store.read_with(cx, |store, _| store.config().profiles[0].select_video));
  }

  /// Records the Group Mode values the store emits.
  fn group_modes(store: &Entity<ConfigStore>, cx: &mut TestAppContext) -> (Arc<Mutex<Vec<bool>>>, Subscription) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let subscription = cx.update(|cx| {
      cx.subscribe(store, move |_, event: &ConfigEvent, _| {
        if let ConfigEvent::GroupMode(on) = event {
          sink.lock().unwrap().push(*on);
        }
      })
    });
    (seen, subscription)
  }

  #[gpui_kit::test]
  fn every_profile_operation_sets_group_mode_from_the_profile(cx: &mut TestAppContext) {
    let (store, _) = store(cx);
    let (seen, _subscription) = group_modes(&store, cx);
    // A new profile has Group Mode off; Default has it on.
    store.update(cx, |store, cx| store.add_profile("Anime", cx));
    assert_eq!(*seen.lock().unwrap(), [false]);
    store.update(cx, |store, cx| store.set_active_profile("Default", cx));
    // Also when the profile is active already, as 0.3.0's setActiveProfile.
    store.update(cx, |store, cx| store.set_active_profile("Default", cx));
    store.update(cx, |store, cx| store.set_active_profile("Missing", cx));
    assert_eq!(*seen.lock().unwrap(), [false, true, true]);
    store.update(cx, |store, cx| {
      store.set_active_profile("Anime", cx);
      store.update_active_profile(cx, |profile| profile.default_group_mode = true);
      // An edit that leaves the field alone does not touch Group Mode.
      store.update_active_profile(cx, |profile| profile.select_audio = true);
      store.reset_active_profile(cx);
    });
    assert_eq!(*seen.lock().unwrap(), [false, true, true, false, true, false]);
    store.update(cx, |store, cx| store.delete_active_profile(cx));
    assert_eq!(seen.lock().unwrap().last(), Some(&true), "Default's value");
    // Nothing to delete: no event.
    store.update(cx, |store, cx| store.delete_active_profile(cx));
    assert_eq!(seen.lock().unwrap().len(), 7);
    // Refused names change nothing.
    store.update(cx, |store, cx| store.add_profile("", cx));
    assert_eq!(seen.lock().unwrap().len(), 7);
  }

  #[gpui_kit::test]
  fn deleting_without_a_default_profile_turns_group_mode_off(cx: &mut TestAppContext) {
    let (store, _) = store(cx);
    let (seen, _subscription) = group_modes(&store, cx);
    store.update(cx, |store, cx| {
      store.update(cx, |config| {
        config.profiles = vec![ConfigProfile::named("Anime")];
        config.active_profile = "Anime".into();
      });
      store.delete_active_profile(cx);
    });
    assert_eq!(*seen.lock().unwrap(), [false]);
    assert!(store.read_with(cx, |store, _| store.config().profiles.is_empty()));
  }
}
