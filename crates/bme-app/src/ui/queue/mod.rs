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

//! The Queue tab shows a table per drive, with drive names for Windows drive
//! letters and shares. Each table shows File Path, Status, Start, End,
//! Elapsed, ETA and each job's action. An empty queue shows just the headers.
//!
//! Each row is a [`QueueRow`] entity drawn as a cached view at its measured
//! height, so a progress event or a clock tick re-renders the extracting row,
//! not the table; the view itself re-renders on structural changes. A table
//! narrower than its columns scrolls sideways.

pub mod row;
#[cfg(test)]
mod tests;

use std::collections::HashMap;

use gpui_kit::base::{TestSupportExt as _, TestTextExt as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme, h_flex, v_flex};
use gpui_kit::*;

pub use row::QueueRow;

use crate::state::Stores;

/// Width of the Status column: the longest statuses, German
/// "Extrahiere 100%" and "Fehlgeschlagen", fit on one line.
pub const STATUS_WIDTH: f32 = 160.0;
/// Width of the Start, End, Elapsed and ETA columns: the longest header,
/// Spanish "Transcurrido", fits on one line.
pub const TIME_WIDTH: f32 = 92.0;
/// Width of the column holding a row's Cancel or Resume button.
pub const ACTION_WIDTH: f32 = 36.0;
/// The narrowest File Path column; a narrower table scrolls sideways.
pub const PATH_MIN_WIDTH: f32 = 200.0;

/// The narrowest table: the path's minimum and the fixed columns.
fn table_min_width() -> Pixels {
  px(PATH_MIN_WIDTH + STATUS_WIDTH + 4.0 * TIME_WIDTH + ACTION_WIDTH)
}

pub struct QueueView {
  stores: Stores,
  rows: HashMap<String, Entity<QueueRow>>,
  /// The sideways scroll position of each drive's table.
  scrolls: HashMap<String, ScrollHandle>,
  _subscriptions: Vec<Subscription>,
}

impl QueueView {
  pub fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
    // Items added, removed or with a new status.
    let subscriptions = vec![cx.observe(&stores.queue, |this, _, cx| {
      this.sync_rows(cx);
      cx.notify();
    })];
    let mut view = Self {
      stores,
      rows: HashMap::new(),
      scrolls: HashMap::new(),
      _subscriptions: subscriptions,
    };
    view.sync_rows(cx);
    view
  }

  /// The row of `file`, if it is listed.
  pub fn row(&self, file: &str) -> Option<&Entity<QueueRow>> {
    self.rows.get(file)
  }

  /// Keeps a row for every listed file.
  fn sync_rows(&mut self, cx: &mut Context<Self>) {
    let queue = self.stores.queue.read(cx);
    let files: Vec<String> = queue.items().map(|item| item.file.clone()).collect();
    self.rows.retain(|file, _| files.contains(file));
    for file in files {
      if !self.rows.contains_key(&file) {
        let stores = self.stores.clone();
        let row = cx.new(|cx| QueueRow::new(stores, file.clone(), cx));
        self.rows.insert(file, row);
      }
    }
  }

  fn row_element(&self, file: &str, cache: bool, cx: &App) -> Option<AnyElement> {
    let row = self.rows.get(file)?;
    Some(row.read(cx).height().view(row, cache))
  }

  fn drive_table(&self, drive: &str, files: &[String], cache: bool, cx: &App) -> impl IntoElement {
    let border = cx.theme().border;
    let id = |name: &str| SharedString::from(format!("{name}:{drive}"));
    let header_cell = |key: &str| -> AnyElement {
      let label = tr!(key);
      // One line: a narrow column clips its header rather than breaking a
      // word in two.
      div()
        .px(px(8.0))
        .py(px(6.0))
        .whitespace_nowrap()
        .overflow_hidden()
        .font_weight(FontWeight::MEDIUM)
        .child(label.clone())
        .test_text(label)
        .into_any_element()
    };
    let fixed = |key: &str, width: f32| div().flex_none().w(px(width)).child(header_cell(key));
    let header = h_flex()
      .w_full()
      .border_b_1()
      .border_color(border)
      .child(div().flex_1().min_w_0().child(header_cell("queue.header.filePath")))
      .child(fixed("queue.header.status", STATUS_WIDTH))
      .child(fixed("queue.header.start", TIME_WIDTH))
      .child(fixed("queue.header.end", TIME_WIDTH))
      .child(fixed("queue.header.elapsed", TIME_WIDTH))
      .child(fixed("queue.header.eta", TIME_WIDTH))
      .child(div().flex_none().w(px(ACTION_WIDTH)));
    let rows: Vec<AnyElement> = files
      .iter()
      .filter_map(|file| self.row_element(file, cache, cx))
      .collect();
    let scroll = self.scrolls.get(drive).cloned().unwrap_or_default();
    v_flex()
      .id(id("queue-drive"))
      .test_support()
      .w_full()
      .text_size(super::text::body2())
      .children((!drive.is_empty() && drive != "default").then(|| {
        let label = SharedString::from(drive.to_string());
        div()
          .px(px(8.0))
          .truncate()
          .text_size(super::text::h6())
          .child(label.clone())
          .test_text(label)
      }))
      .child(
        div()
          .id(id("queue-table"))
          .relative()
          .w_full()
          .overflow_x_scroll()
          // A vertical wheel scrolls the tab, not the table sideways.
          .restrict_scroll_to_axis()
          .track_scroll(&scroll)
          .child(v_flex().w_full().min_w(table_min_width()).child(header).children(rows))
          .horizontal_scrollbar(&scroll),
      )
  }
}

impl Render for QueueView {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let mut drives = self.stores.queue.read(cx).drives();
    if drives.is_empty() {
      drives.push((String::new(), Vec::new()));
    }
    self
      .scrolls
      .retain(|drive, _| drives.iter().any(|(listed, _)| listed == drive));
    for (drive, _) in &drives {
      self.scrolls.entry(drive.clone()).or_default();
    }
    // A screen reader sees cached rows only on the frames that render them.
    let cache = !window.is_a11y_active();
    let tables: Vec<AnyElement> = drives
      .iter()
      .map(|(drive, files)| self.drive_table(drive, files, cache, cx).into_any_element())
      .collect();
    v_flex()
      .id("queue")
      .test_support()
      .w_full()
      .gap(super::space(2.0))
      .children(tables)
  }
}
