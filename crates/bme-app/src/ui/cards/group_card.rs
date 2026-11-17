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

//! The card of a group (`GroupCard.tsx`): files of one folder with the same
//! track counts. The folder and the summary; the actions for every file (Set
//! Output Path, Open in BetterMediaInfo, Copy All Commands, Extract All,
//! Clear All); and a split pane: the file names with their status, and the
//! progress, percent, Cancel and `elapsed / ETA` of an extracting one, on
//! the left, resizable; the first file's track table on the right, whose
//! checkboxes select for every file.

use std::cell::Cell;
use std::rc::Rc;
use std::time::SystemTime;

use bme_core::extract_args::{file_name, parent_dir};
use gpui_kit::assets::IconName;
use gpui_kit::base::{SelectableText, TestSupportExt as _, TestTextExt as _};
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{
  CardHeight, better_media_info_button, card_summary, header_icon_button, icon_button, mono_caption, progress_bar,
  status_icon, time_label,
};
use crate::actions;
use crate::domain::status::QueueItemStatus;
use crate::state::Stores;
use crate::state::files::{FileEvent, TableScope};
use crate::state::queue::QueueEvent;
use crate::ui::output_dialog::{OutputDialogRequest, OutputPathDialog};
use crate::ui::text_button;
use crate::ui::track_table::TrackTable;

/// The file list's starting width and its limits: at least 160 px, and at
/// least 240 px left for the table.
pub const FILE_LIST_WIDTH: f32 = 240.0;
pub const FILE_LIST_MIN_WIDTH: f32 = 160.0;
pub const TABLE_MIN_WIDTH: f32 = 240.0;
const SPLITTER_WIDTH: f32 = 6.0;

/// The file list's width for a pointer at `x` over a split pane spanning
/// `left..left + width`.
pub fn file_list_width(x: Pixels, left: Pixels, width: Pixels) -> Pixels {
  let min = px(FILE_LIST_MIN_WIDTH);
  let max = (width - px(TABLE_MIN_WIDTH)).max(min);
  (x - left).max(min).min(max)
}

pub struct GroupCard {
  stores: Stores,
  /// The group's key ([`crate::domain::grouping`]): its folder and counts.
  key: String,
  files: Vec<String>,
  table: Entity<TrackTable>,
  path_focus: FocusHandle,
  file_list_width: Pixels,
  resizing: bool,
  /// The split pane's bounds from the last frame.
  split: Rc<Cell<Bounds<Pixels>>>,
  height: CardHeight,
  renders: usize,
  _subscriptions: Vec<Subscription>,
}

impl GroupCard {
  pub fn new(
    stores: Stores,
    key: String,
    files: Vec<String>,
    table: Entity<TrackTable>,
    cx: &mut Context<Self>,
  ) -> Self {
    let subscriptions = vec![
      cx.observe(&stores.files, |_, _, cx| cx.notify()),
      cx.subscribe(&stores.files, |this, _, event: &FileEvent, cx| {
        let FileEvent::Changed { files } = event;
        if files.iter().any(|file| this.files.contains(file)) {
          cx.notify();
        }
      }),
      cx.subscribe(&stores.queue, |this, queue, event: &QueueEvent, cx| match event {
        QueueEvent::ItemChanged { file } if this.files.contains(file) => cx.notify(),
        QueueEvent::Tick => {
          let queue = queue.read(cx);
          if this.files.iter().any(|file| {
            queue
              .item(file)
              .is_some_and(|item| item.status == QueueItemStatus::Extracting)
          }) {
            cx.notify();
          }
        }
        _ => {}
      }),
    ];
    Self {
      stores,
      key,
      files,
      table,
      path_focus: cx.focus_handle(),
      file_list_width: px(FILE_LIST_WIDTH),
      resizing: false,
      split: Rc::default(),
      height: CardHeight::default(),
      renders: 0,
      _subscriptions: subscriptions,
    }
  }

  pub fn files(&self) -> &[String] {
    &self.files
  }

  pub fn table(&self) -> &Entity<TrackTable> {
    &self.table
  }

  pub fn height(&self) -> &CardHeight {
    &self.height
  }

  pub fn render_count(&self) -> usize {
    self.renders
  }

  pub fn file_list_width(&self) -> Pixels {
    self.file_list_width
  }

  /// The group gained or lost files; its table follows.
  pub fn set_files(&mut self, files: Vec<String>, cx: &mut Context<Self>) {
    if self.files != files {
      let scope = TableScope::Group(files.clone());
      self.table.update(cx, |table, cx| table.set_scope(scope, cx));
      self.files = files;
      cx.notify();
    }
  }

  /// The element id of the group card with the group key `key`.
  pub fn element_id(key: &str) -> ElementId {
    ElementId::Name(format!("group:{key}").into())
  }

  /// The output directory every file shares, if any.
  fn group_output_dir(&self, cx: &App) -> Option<String> {
    let files = self.stores.files.read(cx);
    let first = files.output_dir(&self.files[0]).filter(|dir| !dir.is_empty())?;
    self
      .files
      .iter()
      .all(|file| files.output_dir(file) == Some(first))
      .then(|| first.to_string())
  }

  /// Set Output Path for every file: the dialog starts from the shared
  /// folder, else the session's, else the group's folder.
  fn open_output_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let initial = self
      .group_output_dir(cx)
      .or_else(|| self.stores.files.read(cx).global_output_dir().map(str::to_string))
      .unwrap_or_else(|| parent_dir(&self.files[0]).to_string());
    let (store, files) = (self.stores.files.clone(), self.files.clone());
    let request = OutputDialogRequest {
      title: tr!("extract.setOutputPath"),
      initial,
      on_confirm: Rc::new(move |value: String, cx: &mut App| {
        store.update(cx, |store, cx| store.set_output_dir(&files, &value, cx));
      }),
    };
    OutputPathDialog::open(request, window, cx);
  }

  fn resize_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
    let split = self.split.get();
    let width = file_list_width(x, split.left(), split.size.width);
    if width != self.file_list_width {
      self.file_list_width = width;
      cx.notify();
    }
  }

  fn end_resize(&mut self, cx: &mut Context<Self>) {
    if self.resizing {
      self.resizing = false;
      cx.notify();
    }
  }

  /// While resizing, the pointer may leave the splitter and the card.
  fn render_resize_listeners(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
    if !self.resizing {
      return None;
    }
    let entity = cx.entity();
    Some(
      canvas(
        |_, _, _| {},
        move |_, _, window, _| {
          let moves = entity.clone();
          window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble {
              moves.update(cx, |card, cx| {
                if event.pressed_button == Some(MouseButton::Left) {
                  card.resize_to(event.position.x, cx);
                } else {
                  card.end_resize(cx);
                }
              });
            }
          });
          let ups = entity.clone();
          window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
              ups.update(cx, |card, cx| card.end_resize(cx));
            }
          });
        },
      )
      .absolute()
      .top_0()
      .left_0()
      .size_full(),
    )
  }

  fn render_files(&self, now: SystemTime, cx: &mut Context<Self>) -> impl IntoElement {
    let queue = self.stores.queue.read(cx);
    let rows: Vec<AnyElement> = self
      .files
      .iter()
      .enumerate()
      .map(|(index, file)| {
        let item = queue.item(file).cloned();
        let status = item.as_ref().map(|item| item.status);
        let extracting = status == Some(QueueItemStatus::Extracting);
        let progress = item.as_ref().map_or(0, |item| item.progress);
        let danger = cx.theme().danger;
        let cancelled = file.clone();
        v_flex()
          .id(("group-file", index))
          .test_support()
          .w_full()
          .py(crate::ui::space(0.5))
          .px(crate::ui::space(2.0))
          .gap(crate::ui::space(0.5))
          .child(
            h_flex()
              .w_full()
              .items_center()
              .gap(crate::ui::space(0.5))
              .children(status_icon(status, cx))
              .child(super::text(file_name(file).to_string()).min_w_0().flex_1()),
          )
          .when(extracting, |row| {
            row
              .child(
                h_flex()
                  .w_full()
                  .items_center()
                  .gap(crate::ui::space(0.5))
                  .child(progress_bar(progress, cx))
                  .child(mono_caption(format!("{progress}%"), cx).min_w(px(34.0)).text_right())
                  .child(
                    icon_button(
                      ("group-cancel", index),
                      IconName::CircleX,
                      Some(danger),
                      tr!("extract.cancel"),
                      true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| actions::cancel_extract(&this.stores, &cancelled, cx))),
                  ),
              )
              .child(mono_caption(time_label(item.as_ref(), now), cx).w_full())
          })
          .into_any_element()
      })
      .collect();
    v_flex().id("group-files").test_support().w_full().children(rows)
  }
}

impl Render for GroupCard {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self.renders += 1;
    let first = self.files[0].clone();
    let files = self.stores.files.read(cx);
    let queue = self.stores.queue.read(cx);
    let statuses: Vec<Option<QueueItemStatus>> = self
      .files
      .iter()
      .map(|file| queue.item(file).map(|item| item.status))
      .collect();
    let any_active = statuses.iter().flatten().any(|status| status.is_active());
    let any_waiting = statuses.contains(&Some(QueueItemStatus::Waiting));
    // The first file's selection is the group's.
    let has_selection = files.selected_ids(&first).is_some_and(|ids| !ids.is_empty());
    let counts = files.track_counts(&first);
    let better_media_info = files.better_media_info_available();
    let output_dir = self.group_output_dir(cx);
    let summary = card_summary(counts.as_ref(), output_dir.as_deref());
    let now = SystemTime::now();

    let theme = cx.theme();
    let (primary, muted, danger, divider, divider_hover) = (
      theme.primary,
      theme.muted_foreground,
      theme.danger,
      theme.border,
      theme.muted,
    );
    let background = if any_waiting {
      theme.list_hover
    } else {
      theme.background
    };
    let output_color = if output_dir.is_some() { primary } else { muted };
    let directory = SharedString::from(parent_dir(&first).to_string());

    let header = h_flex()
      .items_center()
      .gap(crate::ui::space(1.0))
      .mb(crate::ui::space(1.0))
      .child(
        v_flex()
          .flex_1()
          .min_w_0()
          .ml(crate::ui::space(2.0))
          .gap(px(2.0))
          .child(
            div()
              .id("group-dir")
              .test_support()
              .text_color(muted)
              .track_focus(&self.path_focus)
              .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| window.focus(&this.path_focus, cx)),
              )
              .child(SelectableText::new("group-dir-text", directory.clone()))
              .test_text(directory),
          )
          .children(summary.map(|summary| {
            super::text(summary)
              .id("group-summary")
              .test_support()
              .text_size(crate::ui::text::caption())
              .text_color(muted)
          })),
      )
      .child(
        header_icon_button(
          "group-output",
          "output-folder",
          Some(output_color),
          tr!("extract.setOutputPath"),
          !any_active,
        )
        .on_click(cx.listener(|this, _, window, cx| this.open_output_dialog(window, cx))),
      )
      .when(better_media_info, |header| {
        header.child(
          better_media_info_button("group-bmi").on_click(cx.listener(|this, _, _, cx| {
            let files = this.files.clone();
            actions::open_in_better_media_info(&this.stores, files, cx).detach();
          })),
        )
      })
      .child(
        header_icon_button(
          "group-copy",
          "copy-command",
          None,
          tr!("group.copyAllCommands"),
          has_selection,
        )
        .on_click(cx.listener(|this, _, _, cx| {
          let requests = actions::requests(&this.stores, &this.files, cx);
          actions::copy_commands(&this.stores, requests, cx).detach();
        })),
      )
      .child(
        text_button("group-extract")
          .outline()
          .small()
          .icon(IconName::Scissors)
          .label(tr!("group.extractAll"))
          .tooltip(tr!("group.extractAll"))
          .disabled(!has_selection || any_active)
          .on_click(cx.listener(|this, _, _, cx| {
            actions::extract(&this.stores, &this.files, cx).detach();
          })),
      )
      .child(
        header_icon_button(
          "group-clear",
          "delete",
          Some(danger),
          tr!("group.clearAll"),
          !any_active,
        )
        .on_click(cx.listener(|this, _, _, cx| {
          let files = this.files.clone();
          actions::clear_group(&this.stores, &files, cx);
        })),
      );

    let split = self.split.clone();
    let body = h_flex()
      .id("group-split")
      .relative()
      .w_full()
      .items_start()
      .child(
        div()
          .flex_none()
          .w(self.file_list_width)
          .overflow_hidden()
          .child(self.render_files(now, cx)),
      )
      .child(
        div()
          .id("group-splitter")
          .test_support()
          .flex_none()
          .self_stretch()
          .w(px(SPLITTER_WIDTH))
          .cursor_col_resize()
          .bg(if self.resizing { divider_hover } else { divider })
          .hover(move |style| style.bg(divider_hover))
          .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
              this.resizing = true;
              cx.stop_propagation();
              cx.notify();
            }),
          ),
      )
      .child(
        div()
          .flex_1()
          .min_w_0()
          .ml(crate::ui::space(1.0))
          .child(self.table.clone()),
      )
      .child(
        canvas(move |bounds, _, _| split.set(bounds), |_, _, _, _| {})
          .absolute()
          .top_0()
          .left_0()
          .size_full(),
      );

    let frame = v_flex()
      .id(Self::element_id(&self.key))
      .test_support()
      .relative()
      .w_full()
      .p(crate::ui::space(1.0))
      .border_1()
      .rounded(px(4.0))
      .border_color(primary)
      .bg(background)
      .text_size(crate::ui::text::body2())
      // A quick release before the window listeners start.
      .on_mouse_up(MouseButton::Left, cx.listener(|this, _, _, cx| this.end_resize(cx)))
      .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, _, cx| this.end_resize(cx)))
      .child(header)
      .child(body)
      .children(self.render_resize_listeners(cx));
    self.height.measure(frame)
  }
}
