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

//! The card of one file (`MkvFileCard.tsx`): the status icon, the path, the
//! summary, the actions (Set Output Path, Open in BetterMediaInfo, Copy
//! Command, Extract, Delete), the progress with `elapsed / ETA` and Cancel
//! while the file waits or is extracted, and the track table.

use std::rc::Rc;
use std::time::SystemTime;

use bme_core::extract_args::resolve_output_dir;
use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::base::input::TextareaState;
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{CardHeight, better_media_info_button, card_summary, header_icon_button, icon_button, status_icon};
use crate::actions;
use crate::domain::status::QueueItemStatus;
use crate::state::Stores;
use crate::state::files::FileEvent;
use crate::state::queue::QueueEvent;
use crate::ui::output_dialog::{OutputDialogRequest, OutputPathDialog};
use crate::ui::selectable_path;
use crate::ui::text_button;
use crate::ui::track_table::TrackTable;

pub struct FileCard {
  stores: Stores,
  file: String,
  table: Entity<TrackTable>,
  path: Entity<TextareaState>,
  height: CardHeight,
  renders: usize,
  _subscriptions: Vec<Subscription>,
}

impl FileCard {
  pub fn new(
    stores: Stores,
    file: String,
    table: Entity<TrackTable>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Self {
    let path = selectable_path::state(file.clone(), window, cx);
    let subscriptions = vec![
      // Structural changes: a listing done, BetterMediaInfo found.
      cx.observe(&stores.files, |_, _, cx| cx.notify()),
      cx.subscribe(&stores.files, |this, _, event: &FileEvent, cx| {
        let FileEvent::Changed { files } = event;
        if files.contains(&this.file) {
          cx.notify();
        }
      }),
      cx.subscribe(&stores.queue, |this, queue, event: &QueueEvent, cx| match event {
        QueueEvent::ItemChanged { file } if *file == this.file => cx.notify(),
        QueueEvent::Tick
          if queue
            .read(cx)
            .item(&this.file)
            .is_some_and(|item| item.status == QueueItemStatus::Extracting) =>
        {
          cx.notify()
        }
        _ => {}
      }),
    ];
    Self {
      stores,
      file,
      table,
      path,
      height: CardHeight::default(),
      renders: 0,
      _subscriptions: subscriptions,
    }
  }

  pub fn file(&self) -> &str {
    &self.file
  }

  pub fn table(&self) -> &Entity<TrackTable> {
    &self.table
  }

  #[cfg(test)]
  pub(super) fn path_state(&self) -> &Entity<TextareaState> {
    &self.path
  }

  pub fn height(&self) -> &CardHeight {
    &self.height
  }

  /// How many times the card rendered (the repaint check).
  pub fn render_count(&self) -> usize {
    self.renders
  }

  /// The element id of the card of `file`.
  pub fn element_id(file: &str) -> ElementId {
    ElementId::Name(format!("card:{file}").into())
  }

  fn files(&self) -> Vec<String> {
    vec![self.file.clone()]
  }

  /// Set Output Path: the dialog starts from the card's own folder, else the
  /// session's, else the file's folder (`handleOpenOutputDialog`).
  fn open_output_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let files = self.stores.files.read(cx);
    let initial = resolve_output_dir(&self.file, files.output_dir(&self.file), files.global_output_dir());
    let (store, file) = (self.stores.files.clone(), self.file.clone());
    let request = OutputDialogRequest {
      title: tr!("extract.setOutputPath"),
      initial,
      on_confirm: Rc::new(move |value: String, cx: &mut App| {
        store.update(cx, |files, cx| {
          files.set_output_dir(std::slice::from_ref(&file), &value, cx)
        });
      }),
    };
    OutputPathDialog::open(request, window, cx);
  }
}

impl Render for FileCard {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self.renders += 1;
    let file = self.file.clone();
    let files = self.stores.files.read(cx);
    let item = self.stores.queue.read(cx).item(&file).cloned();
    let status = item.as_ref().map(|item| item.status);
    let extracting = status == Some(QueueItemStatus::Extracting);
    let queued = status == Some(QueueItemStatus::Waiting);
    let active = extracting || queued;
    let has_selection = files.selected_ids(&file).is_some_and(|ids| !ids.is_empty());
    let counts = files.track_counts(&file);
    let output = files.output_dir(&file).map(str::to_string);
    let summary = card_summary(counts.as_ref(), output.as_deref());
    let better_media_info = files.better_media_info_available();

    let theme = cx.theme();
    let (border, primary, muted, danger) = (theme.border, theme.primary, theme.muted_foreground, theme.danger);
    let background = if queued { theme.list_hover } else { theme.background };

    let path = selectable_path::element(
      "card-path",
      &self.path,
      tr!("queue.header.filePath"),
      SharedString::from(file.clone()),
      cx,
    );
    let title_row = h_flex()
      .items_center()
      .gap(crate::ui::space(0.5))
      .children(status_icon(status, cx))
      .child(path);

    let output_color = if output.is_some() { primary } else { muted };
    let actions = h_flex()
      .flex_none()
      .items_center()
      .gap(crate::ui::space(0.5))
      .child(
        header_icon_button(
          "card-output",
          "output-folder",
          Some(output_color),
          tr!("extract.setOutputPath"),
          !active,
        )
        .on_click(cx.listener(|this, _, window, cx| this.open_output_dialog(window, cx))),
      )
      .when(better_media_info, |actions| {
        actions.child(
          better_media_info_button("card-bmi").on_click(cx.listener(|this, _, _, cx| {
            let files = this.files();
            actions::open_in_better_media_info(&this.stores, files, cx).detach();
          })),
        )
      })
      .child(
        header_icon_button(
          "card-copy",
          "copy-command",
          None,
          tr!("extract.copyCommand"),
          has_selection && !active,
        )
        .on_click(cx.listener(|this, _, _, cx| {
          let requests = actions::requests(&this.stores, &this.files(), cx);
          actions::copy_commands(&this.stores, requests, cx).detach();
        })),
      )
      .child(
        text_button("card-extract")
          .outline()
          .small()
          .icon(IconName::Scissors)
          .label(tr!("extract.extract"))
          .disabled(!has_selection || active)
          .on_click(cx.listener(|this, _, _, cx| {
            actions::extract(&this.stores, &this.files(), cx).detach();
          })),
      )
      .child(
        header_icon_button("card-delete", "delete", Some(danger), tr!("list.delete"), !extracting).on_click(
          cx.listener(|this, _, _, cx| {
            let file = this.file.clone();
            actions::remove_file(&this.stores, &file, cx);
          }),
        ),
      );

    let progress = active.then(|| {
      let now = SystemTime::now();
      h_flex()
        .id("card-progress")
        .test_support()
        .items_center()
        .gap(crate::ui::space(1.0))
        .px(crate::ui::space(2.0))
        .pb(crate::ui::space(1.0))
        .mt(crate::ui::space(1.0))
        .map(|row| {
          if extracting {
            row
              .child(super::progress_bar(item.as_ref().map_or(0, |item| item.progress), cx))
              .child(super::mono_caption(super::time_label(item.as_ref(), now), cx))
          } else {
            row.child(div().flex_1())
          }
        })
        .child(
          icon_button(
            "card-cancel",
            IconName::CircleX,
            Some(danger),
            tr!("extract.cancel"),
            true,
          )
          .on_click(cx.listener(|this, _, _, cx| {
            let file = this.file.clone();
            actions::cancel_extract(&this.stores, &file, cx);
          })),
        )
    });

    let frame = v_flex()
      .id(Self::element_id(&file))
      .test_support()
      .relative()
      .w_full()
      .border_1()
      .rounded(px(4.0))
      .border_color(border)
      .bg(background)
      .text_size(crate::ui::text::body2())
      .child(
        h_flex()
          .items_start()
          .gap(crate::ui::space(1.0))
          .px(crate::ui::space(2.0))
          .pt(crate::ui::space(2.0))
          .pb(crate::ui::space(if active { 0.0 } else { 1.0 }))
          .child(
            v_flex()
              .flex_1()
              .min_w_0()
              .gap(px(2.0))
              .child(title_row)
              .children(summary.map(|summary| {
                super::text(summary)
                  .id("card-summary")
                  .test_support()
                  .text_size(crate::ui::text::caption())
                  .text_color(muted)
              })),
          )
          .child(actions),
      )
      .children(progress)
      .child(
        div()
          .px(crate::ui::space(2.0))
          .pb(crate::ui::space(2.0))
          .child(self.table.clone()),
      );
    self.height.measure(frame)
  }
}
