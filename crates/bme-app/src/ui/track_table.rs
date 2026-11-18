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

//! The track table of a card (`TrackSelectionTable.tsx`): a checkbox per row
//! and a select-all box, mixed while only some rows are checked, then ID,
//! Number, Type (the emoji, with the type as its tooltip), Codec, Track Name
//! and Language. The table is read-only: a click on a row toggles its
//! checkbox. While the tracks load it shows a spinner; a failed listing shows
//! its error, and a file without rows "No tracks found.".
//!
//! A group card's table shows its first file's tracks, and its checkboxes
//! write the selection of every file of the group ([`TableScope`]). While a
//! file of the card waits or is extracted, the table takes no clicks. A row
//! whose codec `mkvextract` cannot extract
//! ([`is_extractable`]) has a disabled checkbox that says so in its tooltip,
//! and select-all leaves it out.
//!
//! The table is drawn with gpui-base's table primitives, because the rows of
//! gpui-kit's composable table take no click handler.

use std::collections::HashSet;

use bme_core::extract_args::is_extractable;
use bme_core::protocol::MkvTrack;
use gpui_kit::base::{self as kit_base, TestSupportExt as _, TestTextExt as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::domain::format::track_type_emoji;
use crate::domain::track_error::track_error_message;
use crate::state::Stores;
use crate::state::files::{FileEvent, LoadState, TableScope};
use crate::state::queue::QueueEvent;

const ROW_HEIGHT: f32 = 34.0;
const CELL_PADDING: Pixels = px(6.0);
/// The least width of Track Name, which takes the rest of the row.
const TRACK_NAME_MIN_WIDTH: f32 = 140.0;

/// The table's columns, left to right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Column {
  Check,
  Id,
  Number,
  Type,
  Codec,
  TrackName,
  Language,
}

impl Column {
  const COUNT: usize = Self::Language as usize + 1;
  const TEXT_HEADERS: [(Self, &'static str); 6] = [
    (Self::Id, "extract.header.id"),
    (Self::Number, "extract.header.number"),
    (Self::Type, "extract.header.type"),
    (Self::Codec, "extract.header.codec"),
    (Self::TrackName, "extract.header.trackName"),
    (Self::Language, "extract.header.language"),
  ];

  /// One-based, for screen readers.
  fn index(self) -> usize {
    self as usize + 1
  }

  fn style<E: Styled>(self, element: E, width: Pixels) -> E {
    let element = element
      .flex()
      .items_center()
      .px(CELL_PADDING)
      .whitespace_nowrap()
      .overflow_hidden();
    let element = if self == Self::TrackName {
      element.flex_1().min_w(width)
    } else {
      element.flex_none().w(width)
    };
    if self == Self::Check {
      element.justify_center()
    } else {
      element
    }
  }
}

/// The text of a row's cells as the table shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowTexts {
  pub id: String,
  pub number: String,
  pub emoji: &'static str,
  /// The type, the emoji's tooltip.
  pub type_name: String,
  pub codec: String,
  pub track_name: String,
  pub language: String,
}

/// A row's cell texts; the numbers as `mkvmerge` reports them, 0 included.
pub fn row_texts(track: &MkvTrack) -> RowTexts {
  RowTexts {
    id: track.id.to_string(),
    number: track.number.to_string(),
    emoji: track_type_emoji(&track.track_type),
    type_name: track.track_type.clone(),
    codec: track.codec.clone(),
    track_name: track.track_name.clone(),
    language: track.language.clone(),
  }
}

/// The checked and mixed state of a select-all box over `count` of `total`
/// checked rows.
pub fn select_all_state(count: usize, total: usize) -> (bool, bool) {
  let checked = total > 0 && count == total;
  (checked, count > 0 && !checked)
}

/// The widest value or heading sets each column's width; Track Name has a
/// minimum and takes the remaining width.
struct TrackColumns {
  headers: [(Column, SharedString); Column::TEXT_HEADERS.len()],
  widths: [Pixels; Column::COUNT],
}

impl TrackColumns {
  fn measure(rows: &[RowTexts], window: &Window) -> Self {
    let mut body = window.text_style();
    body.font_size = super::text::body2().into();
    let heading = TextStyle {
      font_weight: FontWeight::MEDIUM,
      ..body.clone()
    };
    let rem_size = window.rem_size();
    let measure = |text: &str, style: &TextStyle| {
      text
        .lines()
        .map(|line| {
          let run = style.to_run(line.len());
          window
            .text_system()
            .layout_line(line, style.font_size.to_pixels(rem_size), &[run], None)
            .width
        })
        .max()
        .unwrap_or_default()
    };
    let headers = Column::TEXT_HEADERS.map(|(column, key)| (column, tr!(key)));
    let mut widths = [px(0.0); Column::COUNT];
    for (column, label) in &headers {
      widths[*column as usize] = measure(label, &heading);
    }
    widths[Column::Check as usize] = rems(0.875).to_pixels(rem_size);
    for row in rows {
      for (column, text) in [
        (Column::Id, row.id.as_str()),
        (Column::Number, row.number.as_str()),
        (Column::Type, row.emoji),
        (Column::Codec, row.codec.as_str()),
        (Column::TrackName, row.track_name.as_str()),
        (Column::Language, row.language.as_str()),
      ] {
        widths[column as usize] = widths[column as usize].max(measure(text, &body));
      }
    }
    let mut widths = widths.map(|width| width.ceil() + CELL_PADDING * 2.0);
    widths[Column::TrackName as usize] = widths[Column::TrackName as usize].max(px(TRACK_NAME_MIN_WIDTH));
    Self { headers, widths }
  }

  fn style<E: Styled>(&self, column: Column, element: E) -> E {
    column.style(element, self.widths[column as usize])
  }

  fn min_width(&self) -> Pixels {
    self.widths.iter().copied().sum()
  }
}

/// A cell's text, readable by tests.
fn cell_text(text: String) -> impl IntoElement {
  div().truncate().child(text.clone()).test_text(text)
}

pub struct TrackTable {
  stores: Stores,
  scope: TableScope,
  /// The table's own sideways scrolling, below its minimum width.
  scroll_x: ScrollHandle,
  _subscriptions: Vec<Subscription>,
}

impl TrackTable {
  pub fn new(stores: Stores, scope: TableScope, cx: &mut Context<Self>) -> Self {
    let subscriptions = vec![
      cx.subscribe(&stores.files, |this, _, event: &FileEvent, cx| {
        let FileEvent::Changed { files } = event;
        if files.iter().any(|file| this.scope.files().contains(file)) {
          cx.notify();
        }
      }),
      // A file of the card waiting or extracting disables the table.
      cx.subscribe(&stores.queue, |this, _, event: &QueueEvent, cx| {
        if let QueueEvent::ItemChanged { file } = event
          && this.scope.files().contains(file)
        {
          cx.notify();
        }
      }),
    ];
    Self {
      stores,
      scope,
      scroll_x: ScrollHandle::new(),
      _subscriptions: subscriptions,
    }
  }

  pub fn scope(&self) -> &TableScope {
    &self.scope
  }

  /// The card's files changed (a file joined or left a group).
  pub fn set_scope(&mut self, scope: TableScope, cx: &mut Context<Self>) {
    if self.scope != scope {
      self.scope = scope;
      cx.notify();
    }
  }

  /// The element id of the table of `scope`, unique in the window.
  pub fn element_id(scope: &TableScope) -> SharedString {
    format!("tracks:{}", scope.shown()).into()
  }

  /// A file of the card waits or is extracted: the table takes no clicks.
  pub fn is_disabled(&self, cx: &App) -> bool {
    let queue = self.stores.queue.read(cx);
    self
      .scope
      .files()
      .iter()
      .any(|file| queue.item(file).is_some_and(|item| item.status.is_active()))
  }

  fn set_checked(&mut self, key: &str, checked: bool, cx: &mut Context<Self>) {
    if self.is_disabled(cx) {
      return;
    }
    let scope = self.scope.clone();
    self
      .stores
      .files
      .update(cx, |files, cx| files.set_checked(&scope, key, checked, cx));
  }

  /// A click on a row flips its checkbox.
  fn toggle(&mut self, key: &str, cx: &mut Context<Self>) {
    let checked = self
      .stores
      .files
      .read(cx)
      .checked_keys(&self.scope)
      .iter()
      .any(|id| id == key);
    self.set_checked(key, !checked, cx);
  }

  /// Lists the failed files of the card again.
  fn retry(&mut self, cx: &mut Context<Self>) {
    let files = self.scope.files().to_vec();
    self.stores.files.update(cx, |store, cx| {
      for file in &files {
        store.retry_load(file, cx);
      }
    });
  }

  fn set_all_checked(&mut self, checked: bool, cx: &mut Context<Self>) {
    if self.is_disabled(cx) {
      return;
    }
    let scope = self.scope.clone();
    self
      .stores
      .files
      .update(cx, |files, cx| files.set_all_checked(&scope, checked, cx));
  }

  fn render_header(
    &self,
    columns: &TrackColumns,
    all: (bool, bool),
    disabled: bool,
    cx: &Context<Self>,
  ) -> impl IntoElement {
    let theme = cx.theme();
    let head = |column: Column| {
      columns.style(
        column,
        kit_base::TableHead::new(("head", column.index()), column.index()),
      )
    };
    let (checked, mixed) = all;
    kit_base::TableHeader::new("track-head-group").child(
      kit_base::TableRow::new("track-head", 1)
        .flex()
        .h(px(ROW_HEIGHT))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.table_head_foreground)
        .child(
          head(Column::Check).child(
            Checkbox::new("track-check-all")
              .small()
              .checked(checked)
              .indeterminate(mixed)
              .disabled(disabled)
              .accessibility_label(tr!("extract.header.selectAll"))
              .on_click(cx.listener(|this, checked: &bool, _, cx| this.set_all_checked(*checked, cx))),
          ),
        )
        .children(
          columns
            .headers
            .iter()
            .map(|(column, label)| head(*column).child(label.clone())),
        ),
    )
  }

  fn render_row(
    &self,
    index: usize,
    (track, texts): (&MkvTrack, RowTexts),
    checked: bool,
    disabled: bool,
    columns: &TrackColumns,
    cx: &Context<Self>,
  ) -> AnyElement {
    let theme = cx.theme();
    let key = track.key();
    let cell = |column: Column| {
      columns.style(
        column,
        kit_base::TableCell::new(("cell", column.index()), column.index()),
      )
    };
    let type_name = SharedString::from(texts.type_name);
    // A row `mkvextract` refuses stays unchecked; its checkbox says why.
    let refused = (!is_extractable(track)).then(|| tr!("extract.notExtractable", codec = track.codec_id));
    let clickable = !disabled && refused.is_none();
    let (row_key, check_key) = (key.clone(), key);
    let checkbox = Checkbox::new(("track-check", index))
      .small()
      .checked(checked)
      .disabled(!clickable)
      .when_some(refused.clone(), |checkbox, reason| checkbox.accessibility_label(reason))
      .on_click(cx.listener(move |this, checked: &bool, _, cx| this.set_checked(&check_key, *checked, cx)));
    kit_base::TableRow::new(("track-row", index), index + 2)
      .flex()
      .min_h(px(ROW_HEIGHT))
      .border_t_1()
      .border_color(theme.table_row_border)
      .when(clickable, |row| {
        let hover = theme.table_hover;
        row
          .cursor_pointer()
          .hover(move |style| style.bg(hover))
          .on_click(cx.listener(move |this, _, _, cx| this.toggle(&row_key, cx)))
      })
      .when(refused.is_some(), |row| row.text_color(theme.muted_foreground))
      .child(
        // The checkbox takes its own clicks: the row does not flip it back.
        cell(Column::Check)
          .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
          .child(match refused {
            Some(reason) => div()
              .id(("track-refused", index))
              .test_support()
              .child(checkbox)
              .tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx))
              .into_any_element(),
            None => checkbox.into_any_element(),
          }),
      )
      .child(cell(Column::Id).child(cell_text(texts.id)))
      .child(cell(Column::Number).child(cell_text(texts.number)))
      .child(
        cell(Column::Type).child(
          div()
            .id(("track-type", index))
            .test_support()
            .child(texts.emoji)
            .tooltip(move |window, cx| Tooltip::new(type_name.clone()).build(window, cx)),
        ),
      )
      .child(cell(Column::Codec).child(cell_text(texts.codec)))
      .child(cell(Column::TrackName).child(cell_text(texts.track_name)))
      .child(cell(Column::Language).child(cell_text(texts.language)))
      .into_any_element()
  }
}

impl Render for TrackTable {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let id = Self::element_id(&self.scope);
    let files = self.stores.files.read(cx);
    // Loading wins over an error.
    let mut load_error = None;
    for file in self.scope.files() {
      match files.load_state(file) {
        LoadState::Loading => {
          return div()
            .id(id)
            .test_support()
            .flex()
            .justify_center()
            .py(px(8.0))
            .child(Spinner::new().small())
            .into_any_element();
        }
        LoadState::Failed(error) if load_error.is_none() => load_error = Some(track_error_message(error)),
        _ => {}
      }
    }
    if let Some(error) = load_error {
      return h_flex()
        .id(id)
        .test_support()
        .gap(super::space(1.0))
        .items_center()
        .child(div().flex_1().text_color(cx.theme().danger).child(cell_text(error)))
        .child(
          super::text_button("track-retry")
            .outline()
            .small()
            .label(tr!("extract.retry"))
            .on_click(cx.listener(|this, _, _, cx| this.retry(cx))),
        )
        .into_any_element();
    }
    let rows = files.table_rows(&self.scope).to_vec();
    if rows.is_empty() {
      return div()
        .id(id)
        .test_support()
        .p(px(8.0))
        .text_color(cx.theme().muted_foreground)
        .child(cell_text(tr!("extract.noTracks").to_string()))
        .into_any_element();
    }
    let checked: HashSet<&str> = files.checked_keys(&self.scope).iter().map(String::as_str).collect();
    let checked: Vec<bool> = rows
      .iter()
      .map(|track| checked.contains(track.key().as_str()))
      .collect();
    let disabled = self.is_disabled(cx);
    let extractable: Vec<bool> = rows.iter().map(is_extractable).collect();
    let checked_extractable = checked
      .iter()
      .zip(&extractable)
      .filter(|(checked, extractable)| **checked && **extractable)
      .count();
    let all = select_all_state(
      checked_extractable,
      extractable.iter().filter(|extractable| **extractable).count(),
    );
    let texts: Vec<RowTexts> = rows.iter().map(row_texts).collect();
    let columns = TrackColumns::measure(&texts, window);
    let row_elements: Vec<AnyElement> = rows
      .iter()
      .zip(texts)
      .enumerate()
      .map(|(index, row)| self.render_row(index, row, checked[index], disabled, &columns, cx))
      .collect();
    let header = self.render_header(&columns, all, disabled || !extractable.contains(&true), cx);
    let theme = cx.theme();
    div()
      .id(id)
      .test_support()
      .relative()
      .w_full()
      .overflow_x_scroll()
      // A vertical wheel scrolls the list, not the table sideways.
      .restrict_scroll_to_axis()
      .track_scroll(&self.scroll_x)
      .text_size(super::text::body2())
      .child(
        kit_base::Table::new("track-table")
          .accessibility_label(tr!("extract.header.tracks"))
          .w_full()
          .min_w(columns.min_width())
          .bg(theme.table)
          .child(header)
          .child(kit_base::TableBody::new("track-body").child(v_flex().children(row_elements))),
      )
      .horizontal_scrollbar(&self.scroll_x)
      .into_any_element()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use core::prelude::v1::test;

  #[test]
  fn select_all_is_mixed_while_some_rows_are_checked() {
    assert_eq!(select_all_state(0, 0), (false, false));
    assert_eq!(select_all_state(0, 3), (false, false));
    assert_eq!(select_all_state(1, 3), (false, true));
    assert_eq!(select_all_state(3, 3), (true, false));
  }

  #[test]
  fn row_texts_show_the_listing_as_it_is() {
    let track = MkvTrack {
      id: 2,
      number: 3,
      track_type: "subtitles".into(),
      codec: "SubRip/SRT".into(),
      codec_id: "S_TEXT/UTF8".into(),
      track_name: "Signs".into(),
      language: "eng".into(),
    };
    let texts = row_texts(&track);
    assert_eq!(
      (
        texts.id.as_str(),
        texts.number.as_str(),
        texts.emoji,
        texts.type_name.as_str()
      ),
      ("2", "3", "💬", "subtitles")
    );
    assert_eq!(
      (texts.codec.as_str(), texts.track_name.as_str(), texts.language.as_str()),
      ("SubRip/SRT", "Signs", "eng")
    );
    let chapters = MkvTrack {
      id: 0,
      number: 0,
      track_type: "chapters".into(),
      codec: String::new(),
      codec_id: String::new(),
      track_name: String::new(),
      language: String::new(),
    };
    let texts = row_texts(&chapters);
    assert_eq!(
      (texts.id.as_str(), texts.number.as_str(), texts.emoji),
      ("0", "0", "📑")
    );
  }
}
