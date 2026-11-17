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

//! The Welcome page, shown while the file list is empty.
//!
//! Colours are fixed blues in light and dark, whatever the theme; only the
//! headline uses the theme's primary colour (solid, not a gradient). GPUI has
//! no radial gradient, so the page background is a diagonal linear one, and
//! the hover lift of the cards is a deeper shadow.

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::button::{ButtonCustomVariant, ButtonVariants};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;

use super::{hex, rgba, space, text, text_button};
use crate::assets::{APP_ICON, BATCH_MKV_MERGE_ICON, BD_MASTER_ICON, BETTER_MEDIA_INFO_ICON};
use crate::platform::dialogs;
use crate::state::Stores;

const AUTHOR_NAME: &str = "Sam Cao";
const AUTHOR_URL: &str = "https://github.com/caoccao/";
pub const GITHUB_URL: &str = "https://github.com/caoccao/BatchMkvExtract";
const BATCH_MKV_MERGE_URL: &str = "https://github.com/caoccao/BatchMkvMerge";
const BETTER_MEDIA_INFO_URL: &str = "https://github.com/caoccao/BetterMediaInfo";
const BD_MASTER_URL: &str = "https://github.com/caoccao/BDMaster";

/// The page's fixed palette for one mode.
struct Palette {
  page_from: Hsla,
  page_to: Hsla,
  card_border: Hsla,
  primary_card: (Hsla, Hsla),
  card: (Hsla, Hsla),
  card_shadow: Hsla,
  card_hover_shadow: Hsla,
  card_title: Hsla,
  link: Hsla,
  link_hover_background: Hsla,
  folder_border: Hsla,
  folder_hover_background: Hsla,
  separator: Hsla,
}

impl Palette {
  fn new(dark: bool) -> Self {
    if dark {
      Self {
        page_from: rgba(30, 64, 175, 0.20),
        page_to: rgba(14, 165, 233, 0.16),
        card_border: rgba(96, 165, 250, 0.35),
        primary_card: (rgba(37, 99, 235, 0.32), rgba(14, 165, 233, 0.18)),
        card: (rgba(30, 58, 138, 0.28), rgba(15, 23, 42, 0.40)),
        card_shadow: rgba(2, 6, 23, 0.45),
        card_hover_shadow: rgba(2, 6, 23, 0.55),
        card_title: hex(0xbfdbfe),
        link: hex(0x93c5fd),
        link_hover_background: rgba(59, 130, 246, 0.16),
        folder_border: hex(0x60a5fa),
        folder_hover_background: rgba(59, 130, 246, 0.12),
        separator: rgba(148, 163, 184, 0.20),
      }
    } else {
      Self {
        page_from: rgba(191, 219, 254, 0.55),
        page_to: rgba(186, 230, 253, 0.45),
        card_border: rgba(37, 99, 235, 0.25),
        primary_card: (rgba(59, 130, 246, 0.16), rgba(14, 165, 233, 0.10)),
        card: (rgba(219, 234, 254, 0.85), rgba(241, 245, 249, 0.85)),
        card_shadow: rgba(37, 99, 235, 0.10),
        card_hover_shadow: rgba(37, 99, 235, 0.18),
        card_title: hex(0x1d4ed8),
        link: hex(0x1d4ed8),
        link_hover_background: rgba(37, 99, 235, 0.08),
        folder_border: hex(0x2563eb),
        folder_hover_background: rgba(37, 99, 235, 0.06),
        separator: rgba(148, 163, 184, 0.30),
      }
    }
  }
}

fn shadow(color: Hsla, y: f32, blur: f32) -> Vec<BoxShadow> {
  vec![BoxShadow {
    color,
    offset: point(px(0.0), px(y)),
    blur_radius: px(blur),
    spread_radius: px(0.0),
    inset: false,
  }]
}

/// One application card: logo, name, introduction, GitHub button.
struct AppCard {
  id: &'static str,
  logo: &'static str,
  title: &'static str,
  intro: SharedString,
  url: &'static str,
  /// BatchMkvExtract's own card has a stronger tint.
  primary: bool,
}

fn app_card(card: AppCard, palette: &Palette, cx: &App) -> impl IntoElement {
  let AppCard {
    id,
    logo,
    title,
    intro,
    url,
    primary,
  } = card;
  let (from, to) = if primary { palette.primary_card } else { palette.card };
  let github = ButtonCustomVariant::new(cx)
    .color(rgba(0, 0, 0, 0.0))
    .foreground(palette.link)
    .hover(palette.link_hover_background)
    .active(palette.link_hover_background)
    .shadow(false);
  let hover_shadow = shadow(palette.card_hover_shadow, 14.0, 36.0);
  v_flex()
    .id(id)
    .flex_1()
    .min_w(px(260.0))
    .p(space(3.0))
    .gap(space(1.5))
    .rounded(px(12.0))
    .border_1()
    .border_color(palette.card_border)
    .bg(linear_gradient(
      140.0,
      linear_color_stop(from, 0.0),
      linear_color_stop(to, 1.0),
    ))
    .shadow(shadow(palette.card_shadow, 10.0, 30.0))
    .hover(move |style| style.shadow(hover_shadow.clone()))
    .child(
      h_flex()
        .gap(space(2.0))
        .child(
          div()
            .size(px(56.0))
            .p(space(0.5))
            .rounded(px(8.0))
            .bg(rgba(255, 255, 255, 0.6))
            .shadow(shadow(rgba(15, 23, 42, 0.12), 4.0, 12.0))
            .child(img(logo).size_full().object_fit(ObjectFit::Contain)),
        )
        .child(
          div()
            .text_size(text::h6())
            .font_weight(FontWeight::BOLD)
            .text_color(palette.card_title)
            .child(title),
        ),
    )
    .child(
      div()
        .text_size(text::body2())
        .line_height(relative(1.6))
        .text_color(cx.theme().muted_foreground)
        .child(intro),
    )
    .child(div().flex_1())
    .child(
      h_flex().child(
        text_button(SharedString::from(format!("{id}-github")))
          .custom(github)
          .small()
          .icon(IconName::Github)
          .label(tr!("welcome.viewOnGithub"))
          .on_click(move |_, _, cx| cx.open_url(url)),
      ),
    )
    .test_support()
}

#[derive(IntoElement)]
pub struct Welcome {
  stores: Stores,
}

impl Welcome {
  pub fn new(stores: Stores) -> Self {
    Self { stores }
  }
}

impl RenderOnce for Welcome {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let palette = Palette::new(cx.theme().is_dark());
    let muted = cx.theme().muted_foreground;
    let add_files = ButtonCustomVariant::new(cx)
      .color(hex(0x2563eb))
      .foreground(hex(0xffffff))
      .hover(hex(0x1d4ed8))
      .active(hex(0x1d4ed8))
      .shadow(true);
    let add_folder = ButtonCustomVariant::new(cx)
      .color(rgba(0, 0, 0, 0.0))
      .foreground(palette.link)
      .hover(palette.folder_hover_background)
      .active(palette.folder_hover_background)
      .shadow(false);
    let (files_stores, folder_stores) = (self.stores.clone(), self.stores);
    let link = |id: &'static str, label: SharedString, url: &'static str| {
      div()
        .id(id)
        .cursor_pointer()
        .text_size(px(12.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(palette.link)
        .hover(|style| style.underline())
        .on_click(move |_, _, cx| cx.open_url(url))
        .child(label)
    };

    div()
      .id("welcome")
      .test_support()
      .flex_1()
      .min_h_0()
      .flex()
      .justify_center()
      .items_start()
      .py(space(4.0))
      .px(space(2.0))
      .rounded(px(8.0))
      .bg(linear_gradient(
        150.0,
        linear_color_stop(palette.page_from, 0.0),
        linear_color_stop(palette.page_to, 1.0),
      ))
      .overflow_y_scroll()
      .child(
        v_flex()
          .w_full()
          .max_w(px(880.0))
          .gap(space(3.0))
          .child(
            v_flex()
              .items_center()
              .gap(space(1.0))
              .child(
                div()
                  .text_size(text::h4())
                  .font_weight(FontWeight::EXTRA_BOLD)
                  .text_color(cx.theme().primary)
                  .child(tr!("welcome.title")),
              )
              .child(
                div()
                  .text_size(text::body2())
                  .text_color(muted)
                  .child(tr!("welcome.subtitle")),
              ),
          )
          .child(
            h_flex()
              .flex_wrap()
              .items_stretch()
              .gap(space(2.0))
              .child(app_card(
                AppCard {
                  id: "welcome-card-batch-mkv-extract",
                  logo: APP_ICON,
                  title: "BatchMkvExtract",
                  intro: tr!("welcome.intro"),
                  url: GITHUB_URL,
                  primary: true,
                },
                &palette,
                cx,
              ))
              .child(app_card(
                AppCard {
                  id: "welcome-card-batch-mkv-merge",
                  logo: BATCH_MKV_MERGE_ICON,
                  title: "BatchMkvMerge",
                  intro: tr!("welcome.introBatchMkvMerge"),
                  url: BATCH_MKV_MERGE_URL,
                  primary: false,
                },
                &palette,
                cx,
              ))
              .child(app_card(
                AppCard {
                  id: "welcome-card-better-media-info",
                  logo: BETTER_MEDIA_INFO_ICON,
                  title: "BetterMediaInfo",
                  intro: tr!("welcome.introBetterMediaInfo"),
                  url: BETTER_MEDIA_INFO_URL,
                  primary: false,
                },
                &palette,
                cx,
              ))
              .child(app_card(
                AppCard {
                  id: "welcome-card-bd-master",
                  logo: BD_MASTER_ICON,
                  title: "BDMaster",
                  intro: tr!("welcome.introBDMaster"),
                  url: BD_MASTER_URL,
                  primary: false,
                },
                &palette,
                cx,
              )),
          )
          .child(
            h_flex()
              .justify_center()
              .flex_wrap()
              .gap(space(1.5))
              .child(
                text_button("welcome-add-files")
                  .custom(add_files)
                  // gpui-kit draws a custom variant at 20 % strength; this one
                  // is solid.
                  .bg(hex(0x2563eb))
                  .icon(Icon::new(IconName::FileText))
                  .label(tr!("welcome.addFiles"))
                  .font_weight(FontWeight::SEMIBOLD)
                  .on_click(move |_, _, cx| dialogs::add_files(&files_stores, cx)),
              )
              .child(
                text_button("welcome-add-folder")
                  .custom(add_folder)
                  .border_1()
                  .border_color(palette.folder_border)
                  .icon(Icon::new(IconName::Folder))
                  .label(tr!("welcome.addFolder"))
                  .font_weight(FontWeight::SEMIBOLD)
                  .on_click(move |_, _, cx| dialogs::add_folder(&folder_stores, cx)),
              ),
          )
          .child(
            div()
              .w_full()
              .text_center()
              .text_size(text::caption())
              .text_color(muted)
              .child(tr!("welcome.emptyHint")),
          )
          .child(
            h_flex()
              .justify_center()
              .flex_wrap()
              .gap(space(2.0))
              .pt(space(1.0))
              .border_t_1()
              .border_color(palette.separator)
              .child(
                h_flex()
                  .gap(space(0.75))
                  .child(Icon::new(IconName::User).small().text_color(muted))
                  .child(
                    div()
                      .text_size(text::caption())
                      .text_color(muted)
                      .child(format!("{}:", tr!("about.author"))),
                  )
                  .child(link("welcome-author", AUTHOR_NAME.into(), AUTHOR_URL)),
              )
              .child(
                h_flex()
                  .gap(space(0.75))
                  .child(Icon::new(IconName::Github).small().text_color(muted))
                  .child(link(
                    "welcome-github",
                    GITHUB_URL.trim_start_matches("https://").to_owned().into(),
                    GITHUB_URL,
                  )),
              ),
          ),
      )
  }
}
