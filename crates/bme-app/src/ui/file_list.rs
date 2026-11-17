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

//! The File List tab (`FileList.tsx`): the Welcome page while it is empty,
//! otherwise one card per entry: a [`FileCard`] per file, a [`GroupCard`]
//! per group of files with the same folder and track counts.
//!
//! The cards are kept by their file or their group key, as 0.3.0's React
//! keys were, so a card keeps its state (a group's splitter) while files
//! come and go around it. Each card is drawn as a cached view at the height
//! it measured, so a card that changes repaints alone. Files dragged in from
//! the system outline the list.

use std::collections::{HashMap, HashSet};

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme, v_flex};
use gpui_kit::*;

use super::cards::{FileCard, GroupCard};
use super::track_table::TrackTable;
use super::welcome::Welcome;
use crate::domain::grouping::RenderEntry;
use crate::state::Stores;
use crate::state::files::TableScope;

/// What a card is kept by: its file, or its group's key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CardKey {
  File(String),
  Group(String),
}

impl CardKey {
  pub fn of(entry: &RenderEntry) -> Self {
    match entry {
      RenderEntry::Single { file } => Self::File(file.clone()),
      RenderEntry::Group { key, .. } => Self::Group(key.clone()),
    }
  }
}

/// A card view of either kind.
#[derive(Clone)]
pub enum CardView {
  File(Entity<FileCard>),
  Group(Entity<GroupCard>),
}

impl CardView {
  pub fn file(&self) -> Option<&Entity<FileCard>> {
    match self {
      Self::File(card) => Some(card),
      Self::Group(_) => None,
    }
  }

  pub fn group(&self) -> Option<&Entity<GroupCard>> {
    match self {
      Self::File(_) => None,
      Self::Group(card) => Some(card),
    }
  }

  pub fn table(&self, cx: &App) -> Entity<TrackTable> {
    match self {
      Self::File(card) => card.read(cx).table().clone(),
      Self::Group(card) => card.read(cx).table().clone(),
    }
  }

  pub fn render_count(&self, cx: &App) -> usize {
    match self {
      Self::File(card) => card.read(cx).render_count(),
      Self::Group(card) => card.read(cx).render_count(),
    }
  }

  /// The card as a cached view at its measured height
  /// ([`CardHeight::view`](super::cards::CardHeight::view)).
  fn element(&self, cache: bool, cx: &App) -> AnyElement {
    match self {
      Self::File(card) => card.read(cx).height().view(card, cache),
      Self::Group(card) => card.read(cx).height().view(card, cache),
    }
  }
}

pub struct FileList {
  stores: Stores,
  cards: HashMap<CardKey, CardView>,
  scroll: ScrollHandle,
  _subscriptions: Vec<Subscription>,
}

impl FileList {
  pub fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
    // Cards come and go only with structural changes; each card repaints its
    // own data.
    let subscriptions = vec![cx.observe(&stores.files, |_, _, cx| cx.notify())];
    Self {
      stores,
      cards: HashMap::new(),
      scroll: ScrollHandle::new(),
      _subscriptions: subscriptions,
    }
  }

  /// The card of a file (its own card or its group's).
  pub fn card_of(&self, file: &str, cx: &App) -> Option<&CardView> {
    let entry = self.stores.files.read(cx).entries().iter().find(|entry| match entry {
      RenderEntry::Single { file: own } => own == file,
      RenderEntry::Group { files, .. } => files.iter().any(|own| own == file),
    })?;
    self.cards.get(&CardKey::of(entry))
  }

  /// Creates the cards of new entries, hands a group card its current files
  /// and drops the cards of entries that are gone.
  fn sync_cards(&mut self, entries: &[RenderEntry], window: &mut Window, cx: &mut Context<Self>) {
    let mut live = HashSet::new();
    for entry in entries {
      let key = CardKey::of(entry);
      live.insert(key.clone());
      if let (Some(CardView::Group(card)), RenderEntry::Group { files, .. }) = (self.cards.get(&key), entry) {
        card.update(cx, |card, cx| card.set_files(files.clone(), cx));
        continue;
      }
      if self.cards.contains_key(&key) {
        continue;
      }
      let stores = self.stores.clone();
      let scope = TableScope::from_entry(entry);
      let table = cx.new(|cx| TrackTable::new(stores.clone(), scope, cx));
      let card = match entry {
        RenderEntry::Single { file } => {
          let file = file.clone();
          CardView::File(cx.new(|cx| FileCard::new(stores, file, table, window, cx)))
        }
        RenderEntry::Group { key, files } => {
          let (key, files) = (key.clone(), files.clone());
          CardView::Group(cx.new(|cx| GroupCard::new(stores, key, files, table, cx)))
        }
      };
      self.cards.insert(key, card);
    }
    self.cards.retain(|key, _| live.contains(key));
  }
}

impl Render for FileList {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let primary = cx.theme().primary;
    // Paint the external-file drop outline without reserving space for it.
    let drop_zone = v_flex()
      .id("file-drop-zone")
      .test_support()
      .relative()
      .size_full()
      .drag_over::<ExternalPaths>(move |style, _, _, _| style.border_2().border_dashed().border_color(primary));
    let files = self.stores.files.read(cx);
    if !files.has_files() {
      self.cards.clear();
      return drop_zone.child(Welcome::new(self.stores.clone())).into_any_element();
    }
    let entries = files.entries().to_vec();
    self.sync_cards(&entries, window, cx);
    // A screen reader sees cached cards only on the frames that render them.
    let cache = !window.is_a11y_active();
    let elements: Vec<AnyElement> = entries
      .iter()
      .enumerate()
      .map(|(index, entry)| {
        div()
          .id(("file-entry", index))
          .test_support()
          .w_full()
          .child(self.cards[&CardKey::of(entry)].element(cache, cx))
          .into_any_element()
      })
      .collect();
    drop_zone
      .child(
        div()
          .id("file-list")
          .test_support()
          .size_full()
          .overflow_y_scroll()
          .restrict_scroll_to_axis()
          .track_scroll(&self.scroll)
          .text_size(super::text::body2())
          .child(
            v_flex()
              .w_full()
              .pb(super::SHELL_MARGIN)
              .gap(super::space(1.0))
              .children(elements),
          ),
      )
      .vertical_scrollbar(&self.scroll)
      .into_any_element()
  }
}
