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
//! one process per file. Each listing runs with the MKVToolNix directory of
//! the settings and has a generation of its own: removing the file, or a new
//! MKVToolNix directory, cancels it (its `mkvmerge` is stopped) and frees its
//! place at once, and its late result is dropped. `mkvmerge` is also stopped
//! after [`LIST_TRACKS_TIMEOUT`](bme_core::mkvtoolnix::LIST_TRACKS_TIMEOUT),
//! so a stalled drive cannot hold the places for ever. A failure is kept
//! until the file is removed, Retry is pressed, or the MKVToolNix directory
//! changes.
//!
//! Once a file's tracks arrive, the profile selects its tracks if it has no
//! selection yet (`MkvFileCard.tsx`); the files of a group card share one
//! selection (`GroupCard.tsx`), which names the same tracks in each of them
//! ([`grouping_key`]). An existing selection, also an empty one, is
//! never replaced, except a group member's by its group's.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use bme_core::extract_args::TrackSelector;
use bme_core::protocol::{MkvTrack, TrackCounts, TrackListError};
use gpui_kit::{AppContext, Context};

use super::FileStore;
use crate::domain::grouping::{RenderEntry, grouping_key};

/// How many track listings (`mkvmerge -J`) run at once.
pub const MAX_CONCURRENT_LOADS: usize = 4;

type ListFn = dyn Fn(&str, &str, &AtomicBool) -> Result<Vec<MkvTrack>, TrackListError> + Send + Sync;

/// Lists a file's tracks: `mkvmerge -J` in the app, a stand-in in tests.
/// Called on the background executor with the MKVToolNix directory of the
/// settings and a flag that is set when the listing is no longer wanted. A
/// native parser could replace it without touching the UI.
#[derive(Clone)]
pub struct TrackSource(Arc<ListFn>);

impl TrackSource {
  /// A source that needs only the file (tests).
  pub fn new(list: impl Fn(&str) -> Result<Vec<MkvTrack>, TrackListError> + Send + Sync + 'static) -> Self {
    Self::with_tools(move |file, _, _| list(file))
  }

  /// A source given the MKVToolNix directory and the cancel flag too.
  pub fn with_tools(
    list: impl Fn(&str, &str, &AtomicBool) -> Result<Vec<MkvTrack>, TrackListError> + Send + Sync + 'static,
  ) -> Self {
    Self(Arc::new(list))
  }

  /// `mkvmerge -J` ([`bme_core::mkvtoolnix::list_tracks`]).
  pub fn mkvmerge() -> Self {
    Self::with_tools(bme_core::mkvtoolnix::list_tracks)
  }

  pub(crate) fn list(
    &self,
    file: &str,
    mkv_toolnix_path: &str,
    cancelled: &AtomicBool,
  ) -> Result<Vec<MkvTrack>, TrackListError> {
    (self.0)(file, mkv_toolnix_path, cancelled)
  }
}

/// A listing on the background executor.
pub(super) struct Listing {
  /// Its own number: a result of an earlier listing of the file is dropped.
  generation: u64,
  /// Set when the listing is no longer wanted; `mkvmerge` is then stopped.
  cancelled: Arc<AtomicBool>,
}

impl Listing {
  pub(super) fn cancel(&self) {
    self.cancelled.store(true, Ordering::Relaxed);
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
          && !self.in_flight.contains_key(*file)
          && !self.load_errors.contains_key(*file)
      })
      .take(free)
      .cloned()
      .collect();
    for file in pending {
      self.last_listing += 1;
      let generation = self.last_listing;
      let cancelled = Arc::new(AtomicBool::new(false));
      self.in_flight.insert(
        file.clone(),
        Listing {
          generation,
          cancelled: cancelled.clone(),
        },
      );
      let (source, tools) = (self.source.clone(), self.mkv_toolnix_path.clone());
      cx.spawn(async move |this, cx| {
        let listed = {
          let file = file.clone();
          cx.background_spawn(async move { source.list(&file, &tools, &cancelled) })
            .await
        };
        let _ = this.update(cx, |store, cx| store.finish_load(file, generation, listed, cx));
      })
      .detach();
    }
  }

  /// Cancels the listings in flight: `mkvmerge` is stopped, their places are
  /// free and their results will be dropped. `files` limits it to those
  /// files; `None` cancels every listing.
  pub(super) fn cancel_listings(&mut self, files: Option<&[String]>) {
    let cancelled: Vec<String> = match files {
      Some(files) => files
        .iter()
        .filter(|file| self.in_flight.contains_key(*file))
        .cloned()
        .collect(),
      None => self.in_flight.keys().cloned().collect(),
    };
    for file in cancelled {
      if let Some(listing) = self.in_flight.remove(&file) {
        listing.cancel();
      }
    }
  }

  /// Lists `file` again after its listing failed (its card's Retry).
  pub fn retry_load(&mut self, file: &str, cx: &mut Context<Self>) {
    if self.load_errors.remove(file).is_some() {
      self.refresh(vec![file.to_string()], true, cx);
    }
  }

  /// The MKVToolNix directory of the settings changed: listings still
  /// running with the former one are cancelled and every failed file is
  /// listed again with the new one.
  pub(super) fn set_mkv_toolnix_path(&mut self, path: String, cx: &mut Context<Self>) {
    if self.mkv_toolnix_path == path {
      return;
    }
    self.mkv_toolnix_path = path;
    self.cancel_listings(None);
    let failed: Vec<String> = self.load_errors.drain().map(|(file, _)| file).collect();
    self.refresh(failed, true, cx);
  }

  /// Stores a finished listing of `generation`. A listing that was cancelled
  /// meanwhile (its file removed, the MKVToolNix directory changed) is
  /// dropped. The refresh comes in a task of its own, so every result that
  /// arrived before it runs is refreshed at once: a folder of files is
  /// derived again once per batch of results, not once per file.
  pub(super) fn finish_load(
    &mut self,
    file: String,
    generation: u64,
    listed: Result<Vec<MkvTrack>, TrackListError>,
    cx: &mut Context<Self>,
  ) {
    if self.in_flight.get(&file).map(|listing| listing.generation) != Some(generation) {
      return;
    }
    self.in_flight.remove(&file);
    if !self.files.contains(&file) {
      // Its place goes to the next file.
      self.load_pending(cx);
      return;
    }
    match listed {
      Ok(tracks) => self.store_tracks(&file, tracks),
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

  /// Keeps the listed `tracks` of `file` with their counts and group key.
  pub(super) fn store_tracks(&mut self, file: &str, tracks: Vec<MkvTrack>) {
    self
      .file_track_counts
      .insert(file.to_string(), TrackCounts::of(&tracks));
    self
      .file_group_keys
      .insert(file.to_string(), grouping_key(file, &tracks));
    self.file_tracks.insert(file.to_string(), tracks);
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
