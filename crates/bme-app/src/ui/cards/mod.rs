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

//! The file list's cards: [`FileCard`] (one file) and [`GroupCard`] (files
//! of one folder with the same track counts, selected together), with what
//! both show: the queue status icon, the summary line and the extraction's
//! progress.
//!
//! A card is an entity that repaints itself when one of its files or queue
//! items changes ([`FileEvent`](crate::state::files::FileEvent),
//! [`QueueEvent`](crate::state::queue::QueueEvent)); the file list draws it
//! as a cached view at its measured height ([`CardHeight`]), so a progress
//! tick re-renders one card, not the list. While assistive technology is
//! connected the cards are drawn uncached ([`CardHeight::view`]).

pub mod file_card;
pub mod group_card;
#[cfg(test)]
mod tests;

use std::cell::Cell;
use std::rc::Rc;
use std::time::SystemTime;

use bme_core::protocol::TrackCounts;
use gpui_kit::assets::IconName;
use gpui_kit::base::{ObservedTextElement, TestTextExt as _};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable};
use gpui_kit::*;

pub use file_card::FileCard;
pub use group_card::GroupCard;

use crate::assets::{BETTER_MEDIA_INFO_ICON, card_icon};
use crate::domain::format::{format_hms, format_track_counts};
use crate::domain::status::{QueueItem, QueueItemStatus};

const CARD_ACTION_SIZE: f32 = 24.0;
/// Pixel size of the status icon.
const STATUS_ICON_SIZE: f32 = 18.0;

/// The height a card measured while it last rendered, shared with the file
/// list. A cached view is laid out from a given style, not from its content,
/// so the list lays a card out at this height; a card whose height changes
/// asks for one more frame, in which the list uses the new height.
#[derive(Clone, Default)]
pub struct CardHeight(Rc<Cell<Option<Pixels>>>);

impl CardHeight {
  pub fn get(&self) -> Option<Pixels> {
    self.0.get()
  }

  /// Measure the outer bounds of the framed card used in the list.
  pub fn measure(&self, content: impl IntoElement) -> Div {
    let height = self.0.clone();
    div()
      .w_full()
      .on_children_prepainted(move |bounds, window, _| {
        let measured = bounds[0].size.height;
        if height.get() != Some(measured) {
          height.set(Some(measured));
          window.request_animation_frame();
        }
      })
      .child(content)
  }

  /// `view` as a cached view at this height, or uncached before it has
  /// measured itself or when `cache` is false. GPUI does not replay a reused
  /// view's accessibility nodes, so a cached view drops out of the
  /// accessibility tree on every frame that reuses it; callers pass
  /// `!window.is_a11y_active()`, so a screen reader always sees every card,
  /// at the cost of rendering them all on every frame.
  pub fn view<V: Render>(&self, view: &Entity<V>, cache: bool) -> AnyElement {
    match self.get() {
      Some(height) if cache => view
        .clone()
        .cached(StyleRefinement::default().w_full().h(height))
        .into_any_element(),
      _ => view.clone().into_any_element(),
    }
  }
}

/// The queue status icon (`FileStatusIcon.tsx`): a spinner while extracting,
/// a clock while waiting, a check when completed, an alert when failed,
/// nothing when cancelled or never queued.
pub fn status_icon(status: Option<QueueItemStatus>, cx: &App) -> Option<AnyElement> {
  let theme = cx.theme();
  let size = px(STATUS_ICON_SIZE);
  let icon = |name: IconName, color: Hsla| Icon::new(name).size(size).flex_none().text_color(color);
  Some(match status? {
    QueueItemStatus::Extracting => div()
      .flex()
      .flex_none()
      .items_center()
      .justify_center()
      .size(size)
      .child(Spinner::new().xsmall())
      .into_any_element(),
    QueueItemStatus::Waiting => icon(IconName::Clock, theme.muted_foreground).into_any_element(),
    QueueItemStatus::Completed => icon(IconName::CircleCheck, theme.success).into_any_element(),
    QueueItemStatus::Failed => icon(IconName::CircleAlert, theme.danger).into_any_element(),
    QueueItemStatus::Cancelled => return None,
  })
}

/// The summary under a card's path (`CardSummary.tsx`): the track counts,
/// then the output directory override; `None` when there is nothing to say.
pub fn card_summary(counts: Option<&TrackCounts>, output: Option<&str>) -> Option<String> {
  let mut pieces = Vec::new();
  if let Some(counts) = counts {
    let text = format_track_counts(counts);
    if !text.is_empty() {
      pieces.push(text);
    }
  }
  if let Some(path) = output.filter(|path| !path.is_empty()) {
    pieces.push(tr!("card.summary.toPath", path = path).to_string());
  }
  (!pieces.is_empty()).then(|| pieces.join(" "))
}

/// An extracting card's `elapsed / ETA`, each `--:--:--` while unknown.
pub fn time_label(item: Option<&QueueItem>, now: SystemTime) -> String {
  format!(
    "{} / {}",
    format_hms(item.and_then(|item| item.card_elapsed(now))),
    format_hms(item.and_then(|item| item.eta(now)))
  )
}

/// An extraction's progress bar: 6 px, the success colour on a muted track.
pub fn progress_bar(progress: u32, cx: &App) -> impl IntoElement {
  let theme = cx.theme();
  let fraction = progress.min(100) as f32 / 100.0;
  div()
    .flex_1()
    .h(px(6.0))
    .rounded(px(4.0))
    .bg(theme.muted)
    .overflow_hidden()
    .child(div().h_full().w(relative(fraction)).bg(theme.success))
}

/// Monospace caption text (times, percentages).
pub fn mono_caption(text: impl Into<SharedString>, cx: &App) -> ObservedTextElement<Div> {
  let theme = cx.theme();
  let text = text.into();
  div()
    .flex_none()
    .font_family(theme.mono_font_family.clone())
    .text_size(super::text::caption())
    .text_color(theme.muted_foreground)
    .child(text.clone())
    .test_text(text)
}

/// A text element that tests can read back.
pub fn text(text: impl Into<SharedString>) -> ObservedTextElement<Div> {
  let text = text.into();
  div().child(text.clone()).test_text(text)
}

/// A small icon button with a tooltip that also names it for screen
/// readers and tests. The icon takes `color` only while the button is
/// enabled, so a disabled one dims like the others (the toolbar's rule).
pub fn icon_button(
  id: impl Into<ElementId>,
  icon: IconName,
  color: Option<Hsla>,
  label: SharedString,
  enabled: bool,
) -> Button {
  let icon = match color.filter(|_| enabled) {
    Some(color) => Icon::new(icon).text_color(color),
    None => Icon::new(icon),
  };
  Button::new(id)
    .ghost()
    .small()
    .icon(icon)
    .accessibility_label(label.clone())
    .tooltip(label)
    .disabled(!enabled)
}

fn header_button(id: &'static str, label: SharedString) -> Button {
  Button::new(id)
    .ghost()
    .size(px(CARD_ACTION_SIZE))
    .px_0()
    .accessibility_label(label.clone())
    .tooltip(label)
}

/// A card header's action drawn with the card artwork `name`.
pub(super) fn header_icon_button(
  id: &'static str,
  name: &str,
  color: Option<Hsla>,
  label: SharedString,
  enabled: bool,
) -> Button {
  let icon = card_icon(name).with_size(px(CARD_ACTION_SIZE));
  let icon = match color.filter(|_| enabled) {
    Some(color) => icon.text_color(color),
    None => icon,
  };
  header_button(id, label).child(icon).disabled(!enabled)
}

pub(super) fn better_media_info_button(id: &'static str) -> Button {
  header_button(id, tr!("extract.openInBetterMediaInfo")).child(img(BETTER_MEDIA_INFO_ICON).size(px(CARD_ACTION_SIZE)))
}
