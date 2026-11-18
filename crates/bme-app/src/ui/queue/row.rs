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

//! One row of a drive's queue table, an entity that repaints itself when
//! its item changes or, while it is extracted, when the clock ticks. Its
//! path can be selected and copied, as on a file card.

use std::time::SystemTime;

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::base::input::TextareaState;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{ACTION_WIDTH, STATUS_WIDTH, TIME_WIDTH};
use crate::domain::extract_error::{extract_error_message, warnings_text};
use crate::domain::format::{format_clock, format_hms};
use crate::domain::status::{QueueItem, QueueItemStatus};
use crate::state::Stores;
use crate::state::queue::QueueEvent;
use crate::ui::cards::{CardHeight, icon_button, text};
use crate::ui::selectable_path;

/// The Status text: the label, with the percentage while extracting.
pub fn status_text(item: &QueueItem) -> String {
  let label = rust_i18n::t!(item.status.label_key());
  if item.status == QueueItemStatus::Extracting {
    format!("{label} {}%", item.progress)
  } else {
    label.into_owned()
  }
}

/// The status tooltip: why a failed extraction failed, or what a completed
/// one warned about; `None` when there is nothing to say.
pub fn status_details(item: &QueueItem) -> Option<String> {
  match item.status {
    QueueItemStatus::Failed => item.error.as_ref().map(extract_error_message),
    QueueItemStatus::Completed if !item.warnings.is_empty() => Some(format!(
      "{}\n{}",
      rust_i18n::t!("queue.warnings"),
      warnings_text(&item.warnings, item.warnings.len())
    )),
    _ => None,
  }
}

pub struct QueueRow {
  stores: Stores,
  file: String,
  /// The path's field, made on the first render, which has the window.
  path: Option<Entity<TextareaState>>,
  height: CardHeight,
  renders: usize,
  _subscriptions: Vec<Subscription>,
}

impl QueueRow {
  pub fn new(stores: Stores, file: String, cx: &mut Context<Self>) -> Self {
    let subscriptions = vec![
      cx.subscribe(
        &stores.queue,
        |this: &mut Self, queue, event: &QueueEvent, cx| match event {
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
        },
      ),
    ];
    Self {
      stores,
      file,
      path: None,
      height: CardHeight::default(),
      renders: 0,
      _subscriptions: subscriptions,
    }
  }

  pub fn file(&self) -> &str {
    &self.file
  }

  pub fn height(&self) -> &CardHeight {
    &self.height
  }

  pub fn render_count(&self) -> usize {
    self.renders
  }

  #[cfg(test)]
  pub(super) fn path_state(&self) -> Option<&Entity<TextareaState>> {
    self.path.as_ref()
  }

  /// The element id of the row of `file`.
  pub fn element_id(file: &str) -> SharedString {
    format!("queue-row:{file}").into()
  }
}

impl Render for QueueRow {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self.renders += 1;
    let theme = cx.theme();
    let (border, success, danger, foreground) = (theme.border, theme.success, theme.danger, theme.foreground);
    let Some(item) = self.stores.queue.read(cx).item(&self.file).cloned() else {
      return div().id(Self::element_id(&self.file)).into_any_element();
    };
    let path = self
      .path
      .get_or_insert_with(|| selectable_path::state(self.file.clone(), window, cx))
      .clone();
    let now = SystemTime::now();
    let color = match item.status {
      QueueItemStatus::Extracting | QueueItemStatus::Completed => success,
      QueueItemStatus::Cancelled | QueueItemStatus::Failed => danger,
      QueueItemStatus::Waiting => foreground,
    };
    let cell = |content: AnyElement| div().px(px(8.0)).py(px(6.0)).child(content);
    let fixed = |width: f32, value: String| {
      div()
        .flex_none()
        .w(px(width))
        .child(cell(text(value).into_any_element()))
    };
    let details = status_details(&item);
    let status = div()
      .id("queue-status")
      .test_support()
      .flex_none()
      .w(px(STATUS_WIDTH))
      .text_color(color)
      .child(cell(text(status_text(&item)).into_any_element()))
      .when_some(details, |status, details| {
        status.tooltip(move |window, cx| Tooltip::new(details.clone()).build(window, cx))
      });
    let action = match item.status {
      QueueItemStatus::Waiting | QueueItemStatus::Extracting => Some(
        icon_button(
          "queue-cancel",
          IconName::CircleX,
          Some(danger),
          tr!("extract.cancel"),
          !item.cancel_requested,
        )
        .on_click(cx.listener(|this, _, _, cx| crate::actions::cancel_extract(&this.stores, &this.file, cx))),
      ),
      QueueItemStatus::Cancelled | QueueItemStatus::Failed => Some(
        icon_button(
          "queue-resume",
          IconName::RotateCcw,
          Some(success),
          tr!("queue.resume"),
          true,
        )
        .on_click(cx.listener(|this, _, _, cx| crate::actions::resume_extract(&this.stores, &this.file, cx).detach())),
      ),
      QueueItemStatus::Completed => None,
    };
    let path = selectable_path::element(
      "queue-path",
      &path,
      tr!("queue.header.filePath"),
      SharedString::from(self.file.clone()),
      cx,
    );
    let row = h_flex()
      .id(Self::element_id(&self.file))
      .test_support()
      .relative()
      .w_full()
      .items_center()
      .border_b_1()
      .border_color(border)
      .child(div().flex_1().min_w_0().child(cell(path.into_any_element())))
      .child(status)
      .child(fixed(TIME_WIDTH, format_clock(item.extract_started_at)))
      .child(fixed(TIME_WIDTH, format_clock(item.extract_ended_at)))
      .child(fixed(TIME_WIDTH, format_hms(item.elapsed(now))))
      .child(fixed(TIME_WIDTH, format_hms(item.eta(now))))
      .child(
        h_flex()
          .flex_none()
          .w(px(ACTION_WIDTH))
          .justify_center()
          .children(action),
      );
    self.height.measure(row).into_any_element()
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::i18n::tests::LOCALE_LOCK;
  use bme_core::config::Language;
  use bme_core::extract::ExtractError;
  use core::prelude::v1::test;

  #[test]
  fn the_status_tooltip_explains_a_failure_or_lists_the_warnings() {
    let _locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let mut item = QueueItem::waiting("/m/a.mkv");
    assert_eq!(status_details(&item), None);
    item.status = QueueItemStatus::Failed;
    item.error = Some(ExtractError::Exited {
      code: Some(2),
      messages: Vec::new(),
    });
    assert_eq!(status_details(&item).as_deref(), Some("mkvextract exited with code 2."));
    item.status = QueueItemStatus::Completed;
    item.error = None;
    assert_eq!(status_details(&item), None);
    item.warnings = vec!["one".to_string(), "two".to_string()];
    assert_eq!(
      status_details(&item).as_deref(),
      Some("Completed with warnings:\none\ntwo")
    );
  }
}
