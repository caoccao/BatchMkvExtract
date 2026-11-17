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

//! The About tab: icon with a glow, name, version, tagline, and author and
//! GitHub cards that open their pages. The name is solid primary-coloured text
//! (gradient text is not kept); the glow is a soft shadow around a disc, as
//! GPUI has no radial gradient or blur filter.

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::{ActiveTheme, Icon, h_flex, v_flex};
use gpui_kit::*;

use super::welcome::GITHUB_URL;
use super::{hex, rgba, space, text};
use crate::assets::APP_ICON;

const AUTHOR_NAME: &str = "Sam Cao";
const AUTHOR_URL: &str = "https://github.com/caoccao/";

/// One clickable information card.
fn info_card(
  id: &'static str,
  url: &'static str,
  avatar: Div,
  label: SharedString,
  value: impl IntoElement,
  cx: &App,
) -> impl IntoElement {
  let (border, primary) = (cx.theme().border, cx.theme().primary);
  h_flex()
    .id(id)
    .test_support()
    .w_full()
    .p(space(2.0))
    .gap(space(2.0))
    .rounded(px(12.0))
    .border_1()
    .border_color(border)
    .cursor_pointer()
    .hover(move |style| {
      style.border_color(primary).shadow(vec![BoxShadow {
        color: rgba(99, 102, 241, 0.18),
        offset: point(px(0.0), px(8.0)),
        blur_radius: px(24.0),
        spread_radius: px(0.0),
        inset: false,
      }])
    })
    .on_click(move |_, _, cx| cx.open_url(url))
    .child(
      avatar
        .size(px(48.0))
        .flex_none()
        .rounded_full()
        .flex()
        .items_center()
        .justify_center(),
    )
    .child(
      v_flex()
        .min_w_0()
        .flex_1()
        .child(
          div()
            .text_size(text::caption())
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(label.to_uppercase()),
        )
        .child(value),
    )
}

#[derive(IntoElement)]
pub struct About;

impl RenderOnce for About {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let muted = cx.theme().muted_foreground;
    let gradient = linear_gradient(
      135.0,
      linear_color_stop(hex(0x6366f1), 0.0),
      linear_color_stop(hex(0xec4899), 1.0),
    );
    v_flex()
      .id("about")
      .w_full()
      .max_w(px(640.0))
      .mx_auto()
      .px(space(2.0))
      .py(space(3.0))
      .items_center()
      .gap(space(4.0))
      .child(
        v_flex()
          .items_center()
          .gap(space(1.5))
          .child(
            div()
              .size(px(96.0))
              .flex()
              .items_center()
              .justify_center()
              .rounded_full()
              .shadow(vec![BoxShadow {
                color: rgba(99, 102, 241, 0.30),
                offset: point(px(0.0), px(8.0)),
                blur_radius: px(32.0),
                spread_radius: px(8.0),
                inset: false,
              }])
              .child(img(APP_ICON).size(px(96.0))),
          )
          .child(
            div()
              .text_size(text::h3())
              .font_weight(FontWeight::EXTRA_BOLD)
              .text_color(cx.theme().primary)
              .child(crate::APP_NAME),
          )
          .child(
            div()
              .id("about-version")
              .px(space(1.0))
              .rounded_full()
              .border_1()
              .border_color(cx.theme().border)
              .text_size(text::caption())
              .font_weight(FontWeight::SEMIBOLD)
              .child(format!("v{}", crate::APP_VERSION)),
          )
          .child(
            div()
              .text_size(text::body2())
              .text_color(muted)
              .text_center()
              .child(tr!("about.tagline")),
          ),
      )
      .child(
        v_flex()
          .w_full()
          .gap(space(2.0))
          .child(info_card(
            "about-author",
            AUTHOR_URL,
            div()
              .bg(gradient)
              .child(Icon::new(IconName::User).size(px(24.0)).text_color(hex(0xffffff))),
            tr!("about.author"),
            div().text_size(text::h6()).child(AUTHOR_NAME),
            cx,
          ))
          .child(info_card(
            "about-github",
            GITHUB_URL,
            div()
              .bg(hex(0x24292f))
              .child(Icon::new(IconName::Github).size(px(24.0)).text_color(hex(0xffffff))),
            tr!("about.github"),
            div()
              .text_size(text::body2())
              .font_family(cx.theme().mono_font_family.clone())
              .child(GITHUB_URL),
            cx,
          )),
      )
  }
}
