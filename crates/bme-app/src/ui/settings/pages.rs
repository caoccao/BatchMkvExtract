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

//! The two simple pages: Appearance (mode, theme, language) and Update
//! (check interval).

use bme_core::config::{Config, DisplayMode, Language, Theme, UpdateCheckInterval};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem, SettingPage};
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Selectable, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::BUTTON_GROUP_GAP;
use crate::state::{ConfigStore, Stores};
use crate::theme::THEMES;
use crate::ui::{hex, text_button};

/// The nine languages with their config codes and own names (0.3.0's
/// labels).
pub const LANGUAGES: [(Language, &str, &str); 9] = [
  (Language::De, "de", "Deutsch"),
  (Language::EnUS, "en-US", "English (US)"),
  (Language::Es, "es", "Español"),
  (Language::Fr, "fr", "Français"),
  (Language::It, "it", "Italiano"),
  (Language::Ja, "ja", "日本語"),
  (Language::ZhCN, "zh-CN", "简体中文"),
  (Language::ZhHK, "zh-HK", "繁體中文 (香港)"),
  (Language::ZhTW, "zh-TW", "繁體中文 (台灣)"),
];

pub const INTERVALS: [(UpdateCheckInterval, &str); 3] = [
  (UpdateCheckInterval::Daily, "settings.daily"),
  (UpdateCheckInterval::Weekly, "settings.weekly"),
  (UpdateCheckInterval::Monthly, "settings.monthly"),
];

/// The name of a unit variant as the config writes it (`Weekly`): for these
/// enums the variant's own name.
pub fn wire_name<T: std::fmt::Debug>(value: &T) -> SharedString {
  format!("{value:?}").into()
}

/// The variant of `options` called `name`.
pub fn from_wire_name<T: std::fmt::Debug + Clone>(options: impl IntoIterator<Item = T>, name: &str) -> Option<T> {
  options.into_iter().find(|option| wire_name(option) == name)
}

/// The config code of a language (`de`, `en-US`).
pub fn language_code(language: Language) -> &'static str {
  LANGUAGES
    .iter()
    .find(|(other, ..)| *other == language)
    .map_or("en-US", |(_, code, _)| code)
}

/// Reads the config for a field's value.
fn read<R>(store: &Entity<ConfigStore>, cx: &App, f: impl FnOnce(&Config) -> R) -> R {
  f(store.read(cx).config())
}

/// Writes one edit to the config.
fn write(store: &Entity<ConfigStore>, cx: &mut App, edit: impl FnOnce(&mut Config)) {
  store.update(cx, |store, cx| store.update(cx, edit));
}

pub fn appearance(stores: &Stores) -> SettingPage {
  let store = stores.config.clone();
  let mode_store = store.clone();
  let reset_store = store.clone();
  let (theme_store, theme_get, theme_reset) = (store.clone(), store.clone(), store.clone());
  let (language_get, language_set) = (store.clone(), store);
  let default_language = SharedString::from(language_code(Config::default().language));
  SettingPage::new(tr!("settings.appearance"))
    .icon(Icon::new(IconName::Settings2))
    .group(SettingGroup::new().items(vec![
      SettingItem::new(
        tr!("settings.mode"),
        SettingField::render(move |_, _, cx| mode_toggle(&mode_store, cx)).on_reset(
          {
            let store = reset_store.clone();
            move |cx| read(&store, cx, |config| config.display_mode != DisplayMode::Auto)
          },
          move |_, cx| write(&reset_store, cx, |config| config.display_mode = DisplayMode::Auto),
        ),
      ),
      SettingItem::new(
        tr!("settings.theme"),
        SettingField::render(move |options, _, cx| theme_palette(&theme_store, options.is_disabled(), cx)).on_reset(
          move |cx| read(&theme_get, cx, |config| config.theme != Theme::default()),
          move |_, cx| write(&theme_reset, cx, |config| config.theme = Theme::default()),
        ),
      ),
      SettingItem::new(
        tr!("settings.language"),
        SettingField::dropdown(
          LANGUAGES
            .iter()
            .map(|(_, code, label)| (SharedString::from(*code), SharedString::from(*label)))
            .collect(),
          move |cx| read(&language_get, cx, |config| language_code(config.language).into()),
          move |code, cx| {
            if let Some((language, ..)) = LANGUAGES.iter().find(|(_, other, _)| *other == code.as_ref()) {
              let language = *language;
              write(&language_set, cx, |config| config.language = language);
            }
          },
        )
        .default_value(default_language),
      ),
    ]))
}

fn theme_label(name: &str) -> SharedString {
  let key = format!("settings.theme{name}");
  tr!(&key)
}

fn theme_palette(store: &Entity<ConfigStore>, disabled: bool, cx: &App) -> impl IntoElement + use<> {
  let current = read(store, cx, |config| config.theme);
  let theme = cx.theme();
  v_flex().gap(BUTTON_GROUP_GAP).children(THEMES.chunks(5).map(|row| {
    h_flex().gap(BUTTON_GROUP_GAP).children(row.iter().map(|spec| {
      let store = store.clone();
      let label = theme_label(spec.name);
      let choice = spec.theme;
      let selected = choice == current;
      let swatch = div()
        .flex()
        .items_center()
        .justify_center()
        .size_full()
        .rounded(px(4.0))
        .border_1()
        .border_color(theme.border)
        .bg(hex(spec.main))
        .when(selected, |swatch| {
          swatch.child(
            Icon::new(IconName::Check)
              .with_size(px(16.0))
              .text_color(theme.primary_foreground),
          )
        });
      Button::new(SharedString::from(format!("settings-theme-option-{}", spec.name)))
        .ghost()
        .size(px(32.0))
        .p_0()
        .selected(selected)
        .toggled(selected)
        .accessibility_label(label.clone())
        .tooltip(label)
        .child(swatch)
        .disabled(disabled)
        .on_click(move |_, _, cx| write(&store, cx, |config| config.theme = choice))
    }))
  }))
}

/// The mode toggle group (Auto, Light, Dark with their icons).
fn mode_toggle(store: &Entity<ConfigStore>, cx: &App) -> impl IntoElement + use<> {
  let current = read(store, cx, |config| config.display_mode);
  let modes = [
    (DisplayMode::Auto, IconName::SunMoon, "settings.autoMode", "auto"),
    (DisplayMode::Light, IconName::Sun, "settings.lightMode", "light"),
    (DisplayMode::Dark, IconName::Moon, "settings.darkMode", "dark"),
  ];
  let store = store.clone();
  h_flex()
    .id("settings-mode")
    .gap(BUTTON_GROUP_GAP)
    .children(modes.map(|(mode, icon, key, name)| {
      let store = store.clone();
      let selected = mode == current;
      text_button(SharedString::from(format!("settings-mode-{name}")))
        .outline()
        .small()
        .icon(Icon::new(icon))
        .label(tr!(key))
        .selected(selected)
        .toggled(selected)
        .on_click(move |_, _, cx| write(&store, cx, |config| config.display_mode = mode))
    }))
}

pub fn update(stores: &Stores) -> SettingPage {
  let (get, set) = (stores.config.clone(), stores.config.clone());
  SettingPage::new(tr!("settings.update"))
    .icon(Icon::new(IconName::Cpu))
    .group(
      SettingGroup::new().item(SettingItem::new(
        tr!("settings.checkNewVersion"),
        SettingField::dropdown(
          INTERVALS
            .iter()
            .map(|(interval, key)| (wire_name(interval), tr!(*key)))
            .collect(),
          move |cx| read(&get, cx, |config| wire_name(&config.update.check_interval)),
          move |name, cx| {
            if let Some(interval) = from_wire_name(INTERVALS.map(|(interval, _)| interval), &name) {
              write(&set, cx, |config| config.update.check_interval = interval);
            }
          },
        )
        .default_value(wire_name(&UpdateCheckInterval::default())),
      )),
    )
}
