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

//! The listed files and what is known about them: their tracks (from
//! `mkvmerge -J`), the track selection, the output directories, Group Mode
//! and where BetterMediaInfo is.
//!
//! The file list's entries ([`build_render_entries`]) are derived after every
//! change. A change notifies observers only when it is structural: files
//! added or removed, a group formed or split, a file loaded or failed, Group
//! Mode switched, the first track selected or the last one cleared. Every
//! change also emits [`FileEvent::Changed`] naming the files, so a card can
//! repaint alone.
//!
//! Loading and the profile's selection live in [`load`]; the track tables'
//! checkboxes, which a group card applies to all of its files, in [`table`].

pub mod load;
pub mod table;

use std::collections::{HashMap, HashSet};

use bme_core::config::ConfigProfile;
use bme_core::extract_args::TrackSelector;
use bme_core::protocol::{MkvTrack, TrackCounts, TrackListError};
use gpui_kit::{App, AppContext, Context, Entity, EventEmitter, Subscription};

pub use load::{LoadState, MAX_CONCURRENT_LOADS, TrackSource};
pub use table::TableScope;

use super::config::{ConfigEvent, ConfigStore, active_profile};
use super::ui::{NotificationKind, UiState};
use crate::domain::grouping::{RenderEntry, build_render_entries};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileEvent {
  /// Data of these files changed: tracks, selection, output directory or the
  /// load state.
  Changed { files: Vec<String> },
}

pub struct FileStore {
  /// The files in the order they were added.
  files: Vec<String>,
  /// What makes each listed file unique ([`file_identity`] when it came
  /// through [`add_paths`], else its path).
  file_identities: HashMap<String, String>,
  /// The table rows of each loaded file.
  file_tracks: HashMap<String, Vec<MkvTrack>>,
  file_track_counts: HashMap<String, TrackCounts>,
  /// Selected row keys (`"{type}:{id}"`) per file; absent until the profile's
  /// selection ran.
  file_selected_ids: HashMap<String, Vec<String>>,
  /// Output directory overrides of a card or a group card.
  file_output_dirs: HashMap<String, String>,
  /// The session's default output directory (not persisted).
  global_output_dir: Option<String>,
  /// Group Mode: session state, seeded by the active profile's
  /// `defaultGroupMode` and set by every profile operation that sets it.
  group_mode: bool,
  /// Where BetterMediaInfo was detected; `None` hides the cards' button.
  better_media_info: Option<String>,
  /// Why a file's tracks could not be listed; it is not tried again until
  /// removed.
  load_errors: HashMap<String, TrackListError>,
  /// Files whose tracks are being listed on the background executor.
  in_flight: HashSet<String>,
  /// Listed files whose refresh is still to come: results that arrive
  /// together are refreshed once (see `finish_load`).
  loaded: Vec<String>,
  entries: Vec<RenderEntry>,
  has_selection: bool,
  /// The active profile's selection; `None` without a profile.
  selector: Option<TrackSelector>,
  source: TrackSource,
  _config: Vec<Subscription>,
}

impl EventEmitter<FileEvent> for FileStore {}

impl FileStore {
  /// A store that follows `config` (the active profile and its Group Mode
  /// events) and lists tracks with `source`.
  pub fn new(config: &Entity<ConfigStore>, source: TrackSource, cx: &mut Context<Self>) -> Self {
    let mut store = Self::with_profile(active_profile(config.read(cx).config()), source);
    store._config = vec![
      cx.observe(config, |store, config, cx| {
        let selector = active_profile(config.read(cx).config()).map(TrackSelector::new);
        store.set_selector(selector, cx);
      }),
      cx.subscribe(config, |store, _, event: &ConfigEvent, cx| {
        let ConfigEvent::GroupMode(on) = event;
        store.set_group_mode(*on, cx);
      }),
    ];
    store
  }

  /// A store with a fixed profile, following no config. Group Mode starts as
  /// the profile's `defaultGroupMode` (0.3.0's `initConfig`).
  pub fn with_profile(profile: Option<&ConfigProfile>, source: TrackSource) -> Self {
    Self {
      files: Vec::new(),
      file_identities: HashMap::new(),
      file_tracks: HashMap::new(),
      file_track_counts: HashMap::new(),
      file_selected_ids: HashMap::new(),
      file_output_dirs: HashMap::new(),
      global_output_dir: None,
      group_mode: profile.is_some_and(|profile| profile.default_group_mode),
      better_media_info: None,
      load_errors: HashMap::new(),
      in_flight: HashSet::new(),
      loaded: Vec::new(),
      entries: Vec::new(),
      has_selection: false,
      selector: profile.map(TrackSelector::new),
      source,
      _config: Vec::new(),
    }
  }

  pub fn files(&self) -> &[String] {
    &self.files
  }

  pub fn has_files(&self) -> bool {
    !self.files.is_empty()
  }

  pub fn tracks(&self, file: &str) -> Option<&[MkvTrack]> {
    self.file_tracks.get(file).map(Vec::as_slice)
  }

  pub fn track_counts(&self, file: &str) -> Option<TrackCounts> {
    self.file_track_counts.get(file).copied()
  }

  pub fn selected_ids(&self, file: &str) -> Option<&[String]> {
    self.file_selected_ids.get(file).map(Vec::as_slice)
  }

  /// The selected rows of `file` in table order (`getSelectedTracksForFile`).
  pub fn selected_tracks(&self, file: &str) -> Vec<MkvTrack> {
    let (Some(tracks), Some(selected)) = (self.file_tracks.get(file), self.file_selected_ids.get(file)) else {
      return Vec::new();
    };
    let selected: HashSet<&str> = selected.iter().map(String::as_str).collect();
    tracks
      .iter()
      .filter(|track| selected.contains(track.key().as_str()))
      .cloned()
      .collect()
  }

  /// Whether a listed file has a selected row (Extract All's enabled state).
  pub fn has_selection(&self) -> bool {
    self.has_selection
  }

  /// The output directory override of `file`.
  pub fn output_dir(&self, file: &str) -> Option<&str> {
    self.file_output_dirs.get(file).map(String::as_str)
  }

  pub fn global_output_dir(&self) -> Option<&str> {
    self.global_output_dir.as_deref()
  }

  pub fn group_mode(&self) -> bool {
    self.group_mode
  }

  pub fn better_media_info_available(&self) -> bool {
    self.better_media_info.is_some()
  }

  /// The directory BetterMediaInfo was detected in.
  pub fn better_media_info_dir(&self) -> Option<&str> {
    self.better_media_info.as_deref()
  }

  /// The file list's cards: single files and group cards, in their order on
  /// screen.
  pub fn entries(&self) -> &[RenderEntry] {
    &self.entries
  }

  /// Every listed file in the order the file list shows them: a group
  /// card's files where the group is.
  pub fn files_on_screen(&self) -> Vec<String> {
    self
      .entries
      .iter()
      .flat_map(|entry| match entry {
        RenderEntry::Single { file } => std::slice::from_ref(file),
        RenderEntry::Group { files, .. } => files.as_slice(),
      })
      .cloned()
      .collect()
  }

  /// Lists `file` as loaded with `tracks` and the `selected` keys, without
  /// a context (tests).
  #[cfg(test)]
  pub(crate) fn load_for_tests(&mut self, file: &str, tracks: Vec<MkvTrack>, selected: &[&str]) {
    self.files.push(file.to_string());
    self
      .file_track_counts
      .insert(file.to_string(), TrackCounts::of(&tracks));
    self.file_tracks.insert(file.to_string(), tracks);
    let selected = selected.iter().map(|key| (*key).to_string()).collect();
    self.file_selected_ids.insert(file.to_string(), selected);
  }

  /// Appends the paths that are not listed yet, in order, and starts listing
  /// their tracks. A path is its own identity here; [`add_paths`] passes the
  /// identity of each file ([`file_identity`]) through
  /// [`Self::add_identified_files`] instead.
  pub fn add_files(&mut self, paths: impl IntoIterator<Item = String>, cx: &mut Context<Self>) {
    self.add_identified_files(paths.into_iter().map(|path| (path.clone(), path)), cx);
  }

  /// [`Self::add_files`] for `(path, identity)` pairs: a path whose identity
  /// a listed file (or an earlier pair) has is the same file under another
  /// spelling (`/m/./a.mkv`, another letter case, a link) and is left out;
  /// the first spelling stays.
  pub fn add_identified_files(&mut self, files: impl IntoIterator<Item = (String, String)>, cx: &mut Context<Self>) {
    let mut listed: HashSet<String> = self.files.iter().cloned().collect();
    let mut identities: HashSet<String> = self.file_identities.values().cloned().collect();
    let mut added = Vec::new();
    for (path, identity) in files {
      if listed.contains(&path) || identities.contains(&identity) {
        continue;
      }
      listed.insert(path.clone());
      identities.insert(identity.clone());
      self.file_identities.insert(path.clone(), identity);
      added.push(path);
    }
    if added.is_empty() {
      return;
    }
    self.files.extend(added.iter().cloned());
    self.refresh(added, true, cx);
  }

  /// Removes one file and everything about it. Its queue item goes with
  /// [`crate::actions::remove_file`].
  pub fn remove_file(&mut self, path: &str, cx: &mut Context<Self>) {
    let Some(index) = self.files.iter().position(|file| file == path) else {
      return;
    };
    self.files.remove(index);
    self.forget(path);
    self.refresh(vec![path.to_string()], true, cx);
  }

  /// Removes every file; the global output directory and Group Mode stay.
  /// The queue is cleared by [`crate::actions::clear_files`].
  pub fn clear_files(&mut self, cx: &mut Context<Self>) {
    if self.files.is_empty() {
      return;
    }
    let removed = std::mem::take(&mut self.files);
    for file in &removed {
      self.forget(file);
    }
    self.refresh(removed, true, cx);
  }

  /// Drops what is known about `file`. A listing in flight runs on: its
  /// result is kept if the file is listed again before it arrives.
  fn forget(&mut self, file: &str) {
    self.file_identities.remove(file);
    self.file_tracks.remove(file);
    self.file_track_counts.remove(file);
    self.file_selected_ids.remove(file);
    self.file_output_dirs.remove(file);
    self.load_errors.remove(file);
  }

  /// Sets the output directory of `files` (a card's file, or a group card's
  /// files); an empty one clears it.
  pub fn set_output_dir(&mut self, files: &[String], dir: &str, cx: &mut Context<Self>) {
    for file in files {
      if dir.is_empty() {
        self.file_output_dirs.remove(file);
      } else {
        self.file_output_dirs.insert(file.clone(), dir.to_string());
      }
    }
    self.refresh(files.to_vec(), false, cx);
  }

  /// Sets the session's default output directory; an empty one clears it.
  pub fn set_global_output_dir(&mut self, dir: String, cx: &mut Context<Self>) {
    let dir = (!dir.is_empty()).then_some(dir);
    if self.global_output_dir != dir {
      self.global_output_dir = dir;
      cx.notify();
    }
  }

  /// Switches Group Mode; the file list is derived again.
  pub fn set_group_mode(&mut self, on: bool, cx: &mut Context<Self>) {
    if self.group_mode != on {
      self.group_mode = on;
      self.refresh(Vec::new(), true, cx);
    }
  }

  /// The toolbar's Group Mode button and F8.
  pub fn toggle_group_mode(&mut self, cx: &mut Context<Self>) {
    self.set_group_mode(!self.group_mode, cx);
  }

  /// Where BetterMediaInfo was detected (the directory the launch needs), or
  /// `None` when it was not.
  pub fn set_better_media_info(&mut self, dir: Option<String>, cx: &mut Context<Self>) {
    if self.better_media_info != dir {
      self.better_media_info = dir;
      cx.notify();
    }
  }

  /// A changed active profile selects the tracks of files without a
  /// selection from now on; existing selections stay.
  fn set_selector(&mut self, selector: Option<TrackSelector>, cx: &mut Context<Self>) {
    if self.selector != selector {
      self.selector = selector;
      self.refresh(Vec::new(), false, cx);
    }
  }

  /// Derives the entries again, applies the profile's selection where it is
  /// due, starts listing the next files, then notifies (when `structural` or
  /// a derived value changed) and emits the changed files.
  fn refresh(&mut self, mut changed: Vec<String>, structural: bool, cx: &mut Context<Self>) {
    let before = (std::mem::take(&mut self.entries), self.has_selection);
    self.entries = build_render_entries(&self.files, self.group_mode, &self.file_track_counts);
    changed.extend(self.auto_select());
    self.has_selection = self
      .files
      .iter()
      .any(|file| self.file_selected_ids.get(file).is_some_and(|ids| !ids.is_empty()));
    let structural = structural || before.0 != self.entries || before.1 != self.has_selection;
    self.load_pending(cx);
    if !changed.is_empty() {
      let mut seen = HashSet::new();
      changed.retain(|file| seen.insert(file.clone()));
      cx.emit(FileEvent::Changed { files: changed });
    }
    if structural {
      cx.notify();
    }
  }
}

/// What makes two paths the same file: the path as the file system resolves
/// it (`.` and `..`, links, and on macOS and Windows the letter case stored
/// on disk), or the path itself when that fails. Reads the file system.
pub fn file_identity(path: &str) -> String {
  std::fs::canonicalize(path)
    .map(|real| real.to_string_lossy().into_owned())
    .unwrap_or_else(|_| path.to_string())
}

/// Resolves dropped, chosen or launch-argument paths to `.mkv` files on the
/// background executor (a folder lists the `.mkv` files directly inside it,
/// sorted) and adds them. What could not be read is skipped, and `ui`, when
/// given, says so in a snackbar.
pub fn add_paths(files: &Entity<FileStore>, ui: Option<&Entity<UiState>>, paths: Vec<String>, cx: &mut App) {
  if paths.is_empty() {
    return;
  }
  let (files, ui) = (files.clone(), ui.cloned());
  cx.spawn(async move |cx| {
    let (found, identified) = cx
      .background_spawn(async move {
        let found = bme_core::services::get_mkv_files(paths);
        let identified: Vec<(String, String)> = found
          .files
          .iter()
          .map(|file| (file.clone(), file_identity(file)))
          .collect();
        (found, identified)
      })
      .await;
    if !found.files.is_empty() {
      files.update(cx, |files, cx| files.add_identified_files(identified, cx));
    }
    if !found.skipped.is_empty()
      && let Some(ui) = ui
    {
      let message = crate::notifications::skipped_paths_message(&found.skipped);
      ui.update(cx, |ui, cx| ui.show_snackbar(NotificationKind::Error, message, cx));
    }
  })
  .detach();
}

#[cfg(test)]
pub(crate) mod tests;
