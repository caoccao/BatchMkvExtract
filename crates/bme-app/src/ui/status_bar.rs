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

//! The main window's status bar with donation and copyright links.

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::link::Link;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::{ActiveTheme, ThemeColor, h_flex};
use gpui_kit::*;

const DONATE_URL: &str = "https://paypal.me/caoccao?locale.x=en_US";
const AUTHOR_URL: &str = "https://github.com/caoccao";
const HOMEPAGE_URL: &str = "https://www.caoccao.com/";

#[derive(IntoElement)]
pub struct AppStatusBar;

impl RenderOnce for AppStatusBar {
  fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
    // The app themes fall back to the window background for the status bar.
    let colors = if cx.theme().is_dark() {
      ThemeColor::dark()
    } else {
      ThemeColor::light()
    };
    div().id("status-bar").test_support().flex_shrink_0().child(
      StatusBar::new()
        .px(super::space(1.0))
        .text_size(super::text::caption())
        .bg(colors.status_bar)
        .border_color(colors.status_bar_border)
        .child(
          h_flex()
            .id("status-bar-links")
            .test_support()
            .mx_auto()
            .gap(super::space(1.5))
            .child(
              Link::new("status-bar-donate")
                .href(DONATE_URL)
                .child(tr!("footer.donate")),
            )
            .child(
              h_flex()
                .gap_1()
                .child(tr!("footer.copyright"))
                .child(Link::new("status-bar-author").href(AUTHOR_URL).child("Sam Cao"))
                .child(Link::new("status-bar-homepage").href(HOMEPAGE_URL).child("caoccao.com")),
            ),
        ),
    )
  }
}
