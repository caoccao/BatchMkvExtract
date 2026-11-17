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

//! Track loading and the profile's selection.
//!
//! The tracks of every listed file that is not loaded, not being listed and
//! has not failed are listed once on the background executor, in list order
//! and at most [`MAX_CONCURRENT_LOADS`] at a time: 0.3.0 ran `mkvmerge -J`
//! on a four-thread runtime, and GPUI's background pool would otherwise start
//! one process per file. A failure is kept and not retried until the file is
//! removed.
//!
//! Once a file's tracks arrive, the profile selects its tracks if it has no
//! selection yet (`MkvFileCard.tsx`); the files of a group card share one
//! selection (`GroupCard.tsx`). An existing selection, also an empty one, is
//! never replaced, except a group member's by its group's.

use std::sync::Arc;

use bme_core::extract_args::TrackSelector;
use bme_core::protocol::{MkvTrack, TrackCounts, TrackListError};
use gpui_kit::{AppContext, Context};

use super::FileStore;
use crate::domain::grouping::RenderEntry;

/// How many track listings (`mkvmerge -J`) run at once.
pub const MAX_CONCURRENT_LOADS: usize = 4;

type ListFn = dyn Fn(&str) -> Result<Vec<MkvTrack>, TrackListError> + Send + Sync;

/// Lists a file's tracks: `mkvmerge -J` in the app, a stand-in in tests.
/// Called on the background executor. A native parser could replace it
/// without touching the UI.
#[derive(Clone)]
pub struct TrackSource(Arc<ListFn>);

impl TrackSource {
  pub fn new(list: impl Fn(&str) -> Result<Vec<MkvTrack>, TrackListError> + Send + Sync + 'static) -> Self {
    Self(Arc::new(list))
  }

  /// `mkvmerge -J` ([`bme_core::mkvtoolnix::list_tracks`]).
  pub fn mkvmerge() -> Self {
    Self::new(bme_core::mkvtoolnix::list_tracks)
  }

  pub(crate) fn list(&self, file: &str) -> Result<Vec<MkvTrack>, TrackListError> {
    (self.0)(file)
  }
}

/// Where a file stands in loading.
#[derive(Clone, Copy, Debug)]
pub enum LoadState<'a> {
  Loading,
  Loaded,
  Failed(&'a TrackListError),
}

/// The keys of the tracks `selector` picks, in table order.
fn profile_selection(selector: &TrackSelector, tracks: &[MkvTrack]) -> Vec<String> {
  tracks
    .iter()
    .filter(|track| selector.selects(track))
    .map(MkvTrack::key)
    .collect()
}

impl FileStore {
  pub fn load_state(&self, file: &str) -> LoadState<'_> {
    if self.file_tracks.contains_key(file) {
      LoadState::Loaded
    } else if let Some(error) = self.load_errors.get(file) {
      LoadState::Failed(error)
    } else {
      LoadState::Loading
    }
  }

  /// How many listings run now (tests).
  #[cfg(test)]
  pub(crate) fn listings_in_flight(&self) -> usize {
    self.in_flight.len()
  }

  /// Starts listing the tracks of the next listed files that are not loaded,
  /// in flight or failed, while fewer than [`MAX_CONCURRENT_LOADS`] run.
  pub(super) fn load_pending(&mut self, cx: &mut Context<Self>) {
    let free = MAX_CONCURRENT_LOADS.saturating_sub(self.in_flight.len());
    if free == 0 {
      return;
    }
    let pending: Vec<String> = self
      .files
      .iter()
      .filter(|file| {
        !self.file_tracks.contains_key(*file)
          && !self.in_flight.contains(*file)
          && !self.load_errors.contains_key(*file)
      })
      .take(free)
      .cloned()
      .collect();
    for file in pending {
      self.in_flight.insert(file.clone());
      let source = self.source.clone();
      cx.spawn(async move |this, cx| {
        let listed = {
          let file = file.clone();
          cx.background_spawn(async move { source.list(&file) }).await
        };
        let _ = this.update(cx, |store, cx| store.finish_load(file, listed, cx));
      })
      .detach();
    }
  }

  /// Stores a finished listing. A file removed meanwhile is dropped; one
  /// removed and added again keeps this result. The refresh comes in a task
  /// of its own, so every result that arrived before it runs is refreshed at
  /// once: a folder of files is derived again once per batch of results, not
  /// once per file.
  pub(super) fn finish_load(
    &mut self,
    file: String,
    listed: Result<Vec<MkvTrack>, TrackListError>,
    cx: &mut Context<Self>,
  ) {
    if !self.in_flight.remove(&file) {
      return;
    }
    if !self.files.contains(&file) {
      // Its place goes to the next file.
      self.load_pending(cx);
      return;
    }
    match listed {
      Ok(tracks) => {
        self.file_track_counts.insert(file.clone(), TrackCounts::of(&tracks));
        self.file_tracks.insert(file.clone(), tracks);
      }
      Err(error) => {
        log::warn!("Failed to list the tracks of {file}: {error}");
        self.load_errors.insert(file.clone(), error);
      }
    }
    self.loaded.push(file);
    if self.loaded.len() == 1 {
      cx.spawn(async move |this, cx| {
        let _ = this.update(cx, |store, cx| store.refresh_loaded(cx));
      })
      .detach();
    }
  }

  /// The refresh of the listings that finished since the last one.
  fn refresh_loaded(&mut self, cx: &mut Context<Self>) {
    let mut loaded = std::mem::take(&mut self.loaded);
    if loaded.is_empty() {
      return;
    }
    loaded.retain(|file| self.files.contains(file));
    self.refresh(loaded, true, cx);
  }

  /// The profile's selection where it is due; the files whose selection it
  /// set. A single card's file with tracks and no selection gets the tracks
  /// the profile selects. A group card's files all get the group's
  /// selection: that of the first member that has one, else the profile's
  /// over the first member's tracks. Nothing is selected without a profile
  /// or for a file without tracks.
  pub(super) fn auto_select(&mut self) -> Vec<String> {
    let Some(selector) = self.selector.as_ref() else {
      return Vec::new();
    };
    let mut changed = Vec::new();
    for entry in &self.entries {
      match entry {
        RenderEntry::Single { file } => {
          if self.file_selected_ids.contains_key(file) {
            continue;
          }
          let Some(tracks) = self.file_tracks.get(file).filter(|tracks| !tracks.is_empty()) else {
            continue;
          };
          self
            .file_selected_ids
            .insert(file.clone(), profile_selection(selector, tracks));
          changed.push(file.clone());
        }
        RenderEntry::Group { files, .. } => {
          let shared = files.iter().find_map(|file| self.file_selected_ids.get(file)).cloned();
          let ids = match shared {
            Some(ids) => ids,
            None => match self.file_tracks.get(&files[0]).filter(|tracks| !tracks.is_empty()) {
              Some(tracks) => profile_selection(selector, tracks),
              None => continue,
            },
          };
          for file in files {
            if self.file_selected_ids.get(file) != Some(&ids) {
              self.file_selected_ids.insert(file.clone(), ids.clone());
              changed.push(file.clone());
            }
          }
        }
      }
    }
    changed
  }
}
