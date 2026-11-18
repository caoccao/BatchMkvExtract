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

//! The update banner above the toolbar: an info alert whose text opens the
//! releases page, a "Skip this version" checkbox and a close button. Closing
//! it with the box checked skips that version. The version comes from
//! [`crate::updates`] through `UiState` instead of a one-second poll.
//! gpui-kit's `Alert` takes only text, so the banner is drawn in its info
//! style.

use gpui_kit::assets::IconName;
use gpui_kit::base::{TestSupportExt, TestTextExt};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex};
use gpui_kit::*;

use crate::state::Stores;

pub struct UpdateBanner {
  stores: Stores,
  /// The "Skip this version" box; unchecked again for the next banner.
  skip: bool,
  _subscription: Subscription,
}

impl UpdateBanner {
  pub fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
    Self {
      _subscription: cx.observe(&stores.ui, |_, _, cx| cx.notify()),
      stores,
      skip: false,
    }
  }

  fn close(&mut self, cx: &mut Context<Self>) {
    let version = self.stores.ui.read(cx).update_available().map(str::to_string);
    if let Some(version) = version {
      crate::updates::dismiss(&self.stores, &version, self.skip, cx);
    }
    self.skip = false;
    cx.notify();
  }
}

impl Render for UpdateBanner {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(version) = self.stores.ui.read(cx).update_available().map(str::to_string) else {
      return div().id("update-banner-hidden").into_any_element();
    };
    let theme = cx.theme();
    let (info, link) = (theme.info, theme.link);
    let message = tr!("update.newVersionAvailable", version = version);
    h_flex()
      .id("update-banner")
      .test_support()
      .flex_shrink_0()
      .w_full()
      .mb(super::space(0.5))
      .px(px(16.0))
      .py(px(6.0))
      .gap(px(12.0))
      .items_center()
      .rounded(theme.radius)
      .border_1()
      .border_color(info.opacity(0.3))
      .bg(info.opacity(0.06))
      .text_sm()
      .child(Icon::new(IconName::Info).text_color(info))
      .child(
        div()
          .id("update-link")
          .test_support()
          .test_text(message.clone())
          .cursor_pointer()
          .text_color(link)
          .hover(|style| style.underline())
          .child(message)
          .on_click(|_, _, cx| cx.open_url(crate::updates::RELEASES_URL)),
      )
      .child(div().flex_1())
      .child(
        Checkbox::new("update-skip")
          .small()
          .label(tr!("update.skipThisVersion"))
          .checked(self.skip)
          .on_click(cx.listener(|this, checked: &bool, _, cx| {
            this.skip = *checked;
            cx.notify();
          })),
      )
      .child(
        Button::new("update-close")
          .ghost()
          .xsmall()
          .icon(IconName::X)
          .accessibility_label(tr!("tabs.close"))
          .tooltip(tr!("tabs.close"))
          .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
      )
      .into_any_element()
  }
}
