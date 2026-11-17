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

//! The Settings dialog on gpui-kit's `Settings` component: a searchable,
//! resizable sidebar of four pages (Appearance, Integration, Profiles,
//! Update) whose titled groups are listed under their page. Every control
//! writes its config field at once (the config store persists in the
//! background and never reads back). Every page but Profiles has gpui-kit's
//! Reset All, back to `Config::default()`; Profiles has a Reset of the active
//! profile instead.
//!
//! The view lives while the dialog is open. Its children keep the editing state
//! (the text fields).

pub mod integration;
mod pages;
pub mod profiles;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogEntrance;
use gpui_kit::component::group_box::GroupBoxVariant;
use gpui_kit::component::setting::{SettingItem, Settings};
use gpui_kit::component::{ActiveTheme, Disableable, Sizable, WindowExt, h_flex};
use gpui_kit::*;

use self::integration::ToolPathRow;
use self::profiles::ProfileEditor;
use crate::assets::{SETTINGS_ICON_NAME, toolbar_icon};
use crate::state::Stores;
use crate::tools::Tool;

const DIALOG_WIDTH: Pixels = px(880.0);
const BUTTON_GROUP_GAP: Pixels = px(2.0);

/// A setting whose label is part of the checkbox, with the indicator first.
fn checkbox_item(
  id: &'static str,
  label: SharedString,
  description: Option<SharedString>,
  get: impl Fn(&App) -> bool + 'static,
  set: impl Fn(bool, &mut App) + 'static,
) -> SettingItem {
  let keywords = std::iter::once(label.clone()).chain(description.clone());
  let set = Rc::new(set);
  SettingItem::render(move |options, _, cx| {
    let set = set.clone();
    Checkbox::new(id)
      .with_size(options.size())
      .disabled(options.is_disabled())
      .checked(get(cx))
      .label(label.clone())
      .children(
        description
          .clone()
          .map(|description| checkbox_description(description, cx)),
      )
      .on_change(move |checked, _, cx| set(*checked, cx))
  })
  .keywords(keywords)
}

fn checkbox_description(description: SharedString, cx: &App) -> Div {
  div()
    .text_sm()
    .text_color(cx.theme().muted_foreground)
    .child(description)
}

/// The narrowest text field of a row with buttons (a tool path, the new
/// profile name); below it the buttons wrap under the field, so long German
/// and French labels still fit at the 600 × 450 minimum window size.
pub const FIELD_MIN_WIDTH: f32 = 200.0;

pub struct SettingsView {
  stores: Stores,
  mkvtoolnix: Entity<ToolPathRow>,
  better_media_info: Entity<ToolPathRow>,
  profile: Entity<ProfileEditor>,
  _subscriptions: Vec<Subscription>,
}

impl SettingsView {
  pub(crate) fn open(stores: &Stores, window: &mut Window, cx: &mut App) -> Option<Entity<Self>> {
    if window.has_active_dialog(cx) {
      return None;
    }
    let view = cx.new(|cx| Self::new(stores.clone(), window, cx));
    let settings = view.clone();
    window.open_dialog(cx, move |dialog, window, cx| {
      let theme = cx.theme();
      let border = theme.border;
      let header = h_flex()
        .pl(super::space(2.0))
        .pr(super::space(5.0))
        .gap(super::space(1.0))
        .child(toolbar_icon(SETTINGS_ICON_NAME).with_size(px(24.0)))
        .child(tr!("settings.title"));
      let settings = settings.clone();
      dialog
        .w(DIALOG_WIDTH)
        .h(window.viewport_size().height * 0.9)
        .centered()
        .entrance(DialogEntrance::None)
        .overlay_closable(false)
        .on_ok(|_, _, _| false)
        .px_0()
        .pb_0()
        .title(header)
        .content(move |content, _, _| {
          content
            .min_h_0()
            .overflow_hidden()
            .border_t_1()
            .border_color(border)
            .child(settings.clone())
        })
    });
    Some(view)
  }

  fn new(stores: Stores, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let profile = cx.new(|cx| ProfileEditor::new(stores.clone(), window, cx));
    // The profile editor's parts are drawn inside the settings, not as a view.
    let subscriptions = vec![
      cx.observe(&stores.config, |_, _, cx| cx.notify()),
      cx.observe(&profile, |_, _, cx| cx.notify()),
    ];
    Self {
      mkvtoolnix: cx.new(|cx| ToolPathRow::new(stores.clone(), Tool::MkvToolNix, window, cx)),
      better_media_info: cx.new(|cx| ToolPathRow::new(stores.clone(), Tool::BetterMediaInfo, window, cx)),
      profile,
      stores,
      _subscriptions: subscriptions,
    }
  }

  pub fn tool_row(&self, tool: Tool) -> &Entity<ToolPathRow> {
    match tool {
      Tool::MkvToolNix => &self.mkvtoolnix,
      Tool::BetterMediaInfo => &self.better_media_info,
    }
  }

  pub fn profile(&self) -> &Entity<ProfileEditor> {
    &self.profile
  }
}

impl Render for SettingsView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let stores = &self.stores;
    div().id("settings").test_support().size_full().child(
      Settings::new("settings-pages")
        .sidebar_width(px(200.0))
        .with_group_variant(GroupBoxVariant::Outline)
        .pages([
          pages::appearance(stores),
          integration::page(stores, &self.mkvtoolnix, &self.better_media_info),
          profiles::page(stores, &self.profile, cx),
          pages::update(stores),
        ]),
    )
  }
}
