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

//! The File List tab: the Welcome page while no file is listed, else one
//! plain box per file or group card, enough to follow loading, the selection
//! and the extraction until the cards and the track table replace them.

use bme_core::extract_args::parent_dir;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::*;

use super::welcome::Welcome;
use crate::domain::format::format_track_counts;
use crate::domain::grouping::RenderEntry;
use crate::domain::status::QueueItemStatus;
use crate::domain::track_error::track_error_message;
use crate::state::Stores;
use crate::state::files::{FileEvent, FileStore, LoadState};
use crate::state::queue::{QueueEvent, QueueStore};

pub struct FileList {
  stores: Stores,
  _subscriptions: Vec<Subscription>,
}

impl FileList {
  pub fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
    let subscriptions = vec![
      cx.observe(&stores.files, |_, _, cx| cx.notify()),
      cx.observe(&stores.queue, |_, _, cx| cx.notify()),
      cx.subscribe(&stores.files, |_, _, _: &FileEvent, cx| cx.notify()),
      cx.subscribe(&stores.queue, |_, _, event: &QueueEvent, cx| {
        if let QueueEvent::ItemChanged { .. } = event {
          cx.notify();
        }
      }),
    ];
    Self {
      stores,
      _subscriptions: subscriptions,
    }
  }
}

/// A file's queue status as the queue shows it: `Extracting N%` while it
/// runs; nothing before it was queued.
fn status_text(queue: &QueueStore, file: &str) -> Option<String> {
  let item = queue.item(file)?;
  let label = rust_i18n::t!(item.status.label_key()).into_owned();
  Some(match item.status {
    QueueItemStatus::Extracting => format!("{label} {}%", item.progress),
    _ => label,
  })
}

/// One file's lines: its path with its queue status, then its tracks (a
/// spinner while they load, the error when they could not be listed).
fn file_lines(files: &FileStore, queue: &QueueStore, file: &str, cx: &App) -> Div {
  let theme = cx.theme();
  let header = h_flex()
    .gap(super::space(1.0))
    .child(div().flex_1().min_w_0().overflow_hidden().child(file.to_string()))
    .children(status_text(queue, file).map(|status| div().text_color(theme.muted_foreground).child(status)));
  let detail = match files.load_state(file) {
    LoadState::Loading => Spinner::new().small().into_any_element(),
    LoadState::Failed(error) => div()
      .text_color(theme.danger)
      .child(track_error_message(error))
      .into_any_element(),
    LoadState::Loaded => {
      let counts = files.track_counts(file).unwrap_or_default();
      let selected = files.selected_ids(file).unwrap_or_default().join(", ");
      v_flex()
        .text_color(theme.muted_foreground)
        .child(format_track_counts(&counts))
        .child(format!("☑ {selected}"))
        .into_any_element()
    }
  };
  v_flex().gap(super::space(0.5)).child(header).child(detail)
}

impl Render for FileList {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
      return drop_zone.child(Welcome::new(self.stores.clone())).into_any_element();
    }
    let queue = self.stores.queue.read(cx);
    let theme = cx.theme();
    let entries: Vec<AnyElement> = files
      .entries()
      .iter()
      .enumerate()
      .map(|(index, entry)| {
        let content = match entry {
          RenderEntry::Single { file } => file_lines(files, queue, file, cx),
          RenderEntry::Group { files: members, .. } => v_flex()
            .gap(super::space(1.0))
            .child(
              div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.primary)
                .child(parent_dir(&members[0]).to_string()),
            )
            .children(members.iter().map(|file| file_lines(files, queue, file, cx))),
        };
        div()
          .id(("file-entry", index))
          .test_support()
          .w_full()
          .p(super::space(1.0))
          .border_1()
          .rounded(theme.radius)
          .border_color(match entry {
            RenderEntry::Single { .. } => theme.border,
            RenderEntry::Group { .. } => theme.primary,
          })
          .child(content)
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
          .text_size(super::text::body2())
          .child(
            v_flex()
              .w_full()
              .pt(super::SHELL_MARGIN)
              .pb(super::SHELL_MARGIN)
              .gap(super::space(1.0))
              .children(entries),
          ),
      )
      .into_any_element()
  }
}
