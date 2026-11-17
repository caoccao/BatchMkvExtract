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

//! What a track table shows and where its checkboxes write, in a file card
//! or a group card.
//!
//! A group card's table shows its first file's tracks, and its selection is
//! every member's: a checkbox writes the first file's selection, changed, to
//! all of them, as 0.3.0's group card did. Rows are keyed by
//! [`MkvTrack::key`], `"{type}:{id}"`.

use bme_core::protocol::MkvTrack;
use gpui_kit::Context;

use super::FileStore;
use crate::domain::grouping::RenderEntry;

/// The files a track table shows and selects for: one file's card, or a
/// group card's files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableScope {
  File(String),
  Group(Vec<String>),
}

impl TableScope {
  pub fn from_entry(entry: &RenderEntry) -> Self {
    match entry {
      RenderEntry::Single { file } => Self::File(file.clone()),
      RenderEntry::Group { files, .. } => Self::Group(files.clone()),
    }
  }

  /// Every file the table selects for.
  pub fn files(&self) -> &[String] {
    match self {
      Self::File(file) => std::slice::from_ref(file),
      Self::Group(files) => files,
    }
  }

  /// The file whose tracks the table shows: a group card's first.
  pub fn shown(&self) -> &str {
    &self.files()[0]
  }
}

impl FileStore {
  /// The table's rows: the shown file's tracks; empty while it loads.
  pub fn table_rows(&self, scope: &TableScope) -> &[MkvTrack] {
    self.tracks(scope.shown()).unwrap_or_default()
  }

  /// The checked row keys: the shown file's selection.
  pub fn checked_keys(&self, scope: &TableScope) -> &[String] {
    self.selected_ids(scope.shown()).unwrap_or_default()
  }

  /// The select-all checkbox: every row of the shown file, or none, for every
  /// file of the scope. Nothing happens before the shown file is loaded.
  pub fn set_all_checked(&mut self, scope: &TableScope, checked: bool, cx: &mut Context<Self>) {
    let Some(tracks) = self.tracks(scope.shown()) else {
      return;
    };
    let ids = if checked {
      tracks.iter().map(MkvTrack::key).collect()
    } else {
      Vec::new()
    };
    self.write_selection(scope, ids, cx);
  }

  /// Ticks or clears one row's checkbox (a click on the row flips it).
  pub fn set_checked(&mut self, scope: &TableScope, key: &str, checked: bool, cx: &mut Context<Self>) {
    if self.tracks(scope.shown()).is_none() {
      return;
    }
    let mut ids = self.checked_keys(scope).to_vec();
    let present = ids.iter().any(|id| id == key);
    if checked && !present {
      ids.push(key.to_string());
    } else if !checked && present {
      ids.retain(|id| id != key);
    } else {
      return;
    }
    self.write_selection(scope, ids, cx);
  }

  fn write_selection(&mut self, scope: &TableScope, ids: Vec<String>, cx: &mut Context<Self>) {
    for file in scope.files() {
      self.file_selected_ids.insert(file.clone(), ids.clone());
    }
    self.refresh(scope.files().to_vec(), false, cx);
  }
}
