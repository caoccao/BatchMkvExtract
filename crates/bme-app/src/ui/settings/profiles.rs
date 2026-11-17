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

//! The Profiles page (0.3.0's profile form on BatchMkvMerge's page scaffold):
//! the active profile with Add, Delete and Reset (both asking first) and
//! Default group mode, then two groups: Track Selection (an auto-select
//! checkbox per track type, with a language filter for video, audio and
//! subtitles) and File Name Templates (a template per track type, the
//! placeholders below). Every control edits the active profile at once, as
//! in 0.3.0. The page has no Reset All; its Reset
//! puts the active profile back to a fresh profile of its name.

use std::rc::Rc;

use bme_core::config::ConfigProfile;
use gpui_kit::assets::IconName;
use gpui_kit::base::{TestSupportExt, TestTextExt};
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::setting::{RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage};
use gpui_kit::component::{Disableable, Icon, Sizable, WindowExt, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{BUTTON_GROUP_GAP, FIELD_MIN_WIDTH, checkbox_description, checkbox_item};
use crate::state::config::active_profile;
use crate::state::{ConfigStore, Stores};
use crate::ui::word_navigation::PATH_CONTEXT;
use crate::ui::{confirm_button, text_button};

/// The width of a language filter beside its checkbox.
const FILTER_WIDTH: Pixels = px(220.0);
/// The width of a template field beside its track type.
const TEMPLATE_WIDTH: Pixels = px(320.0);

/// The track types a profile selects tracks and names files for, in 0.3.0's
/// order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrackKind {
  Video,
  Audio,
  Subtitles,
  Chapters,
  Attachments,
}

impl TrackKind {
  pub const ALL: [Self; 5] = [
    Self::Video,
    Self::Audio,
    Self::Subtitles,
    Self::Chapters,
    Self::Attachments,
  ];

  /// The types with a language filter, in the order of [`Self::ALL`].
  pub const FILTERED: [Self; 3] = [Self::Video, Self::Audio, Self::Subtitles];

  /// A stable name for element ids.
  pub fn name(self) -> &'static str {
    match self {
      Self::Video => "video",
      Self::Audio => "audio",
      Self::Subtitles => "subtitles",
      Self::Chapters => "chapters",
      Self::Attachments => "attachments",
    }
  }

  pub fn label(self) -> SharedString {
    match self {
      Self::Video => tr!("settings.video"),
      Self::Audio => tr!("settings.audio"),
      Self::Subtitles => tr!("settings.subtitles"),
      Self::Chapters => tr!("settings.chapters"),
      Self::Attachments => tr!("settings.attachments"),
    }
  }

  /// The file name template (`*Template`).
  pub fn template(self, profile: &ConfigProfile) -> &str {
    match self {
      Self::Video => &profile.video_template,
      Self::Audio => &profile.audio_template,
      Self::Subtitles => &profile.subtitle_template,
      Self::Chapters => &profile.chapters_template,
      Self::Attachments => &profile.attachments_template,
    }
  }

  pub fn set_template(self, profile: &mut ConfigProfile, value: String) {
    match self {
      Self::Video => profile.video_template = value,
      Self::Audio => profile.audio_template = value,
      Self::Subtitles => profile.subtitle_template = value,
      Self::Chapters => profile.chapters_template = value,
      Self::Attachments => profile.attachments_template = value,
    }
  }

  /// Whether the profile selects tracks of this type (`select*`).
  pub fn selects(self, profile: &ConfigProfile) -> bool {
    match self {
      Self::Video => profile.select_video,
      Self::Audio => profile.select_audio,
      Self::Subtitles => profile.select_subtitle,
      Self::Chapters => profile.select_chapters,
      Self::Attachments => profile.select_attachments,
    }
  }

  pub fn set_selects(self, profile: &mut ConfigProfile, value: bool) {
    match self {
      Self::Video => profile.select_video = value,
      Self::Audio => profile.select_audio = value,
      Self::Subtitles => profile.select_subtitle = value,
      Self::Chapters => profile.select_chapters = value,
      Self::Attachments => profile.select_attachments = value,
    }
  }

  /// The language filter of the selection (`*Languages`); chapters and
  /// attachments have none.
  pub fn languages(self, profile: &ConfigProfile) -> Option<&str> {
    match self {
      Self::Video => Some(&profile.video_languages),
      Self::Audio => Some(&profile.audio_languages),
      Self::Subtitles => Some(&profile.subtitle_languages),
      Self::Chapters | Self::Attachments => None,
    }
  }

  pub fn set_languages(self, profile: &mut ConfigProfile, value: String) {
    match self {
      Self::Video => profile.video_languages = value,
      Self::Audio => profile.audio_languages = value,
      Self::Subtitles => profile.subtitle_languages = value,
      Self::Chapters | Self::Attachments => {}
    }
  }

  /// The index of a type with a language filter in [`Self::FILTERED`].
  fn filter_index(self) -> Option<usize> {
    Self::FILTERED.iter().position(|&kind| kind == self)
  }

  /// Whether the selection of this type has a language filter.
  pub fn has_languages(self) -> bool {
    self.filter_index().is_some()
  }
}

/// What the page last put into its fields, to follow changes made
/// elsewhere (another profile, Reset) without overwriting typing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Synced {
  name: String,
  filters: [String; 3],
  templates: [String; 5],
}

impl Synced {
  fn of(profile: Option<&ConfigProfile>) -> Self {
    let Some(profile) = profile else {
      return Self::default();
    };
    Self {
      name: profile.name.clone(),
      filters: TrackKind::FILTERED.map(|kind| kind.languages(profile).unwrap_or_default().to_string()),
      templates: TrackKind::ALL.map(|kind| kind.template(profile).to_string()),
    }
  }
}

pub struct ProfileEditor {
  stores: Stores,
  new_name: Entity<InputState>,
  filters: [Entity<InputState>; 3],
  templates: [Entity<InputState>; 5],
  synced: Synced,
  _subscriptions: Vec<Subscription>,
}

/// Edits the active profile.
fn edit_profile(store: &Entity<ConfigStore>, cx: &mut App, edit: impl FnOnce(&mut ConfigProfile)) {
  store.update(cx, |store, cx| store.update_active_profile(cx, edit));
}

/// Reads the active profile.
fn read_profile<R: Default>(store: &Entity<ConfigStore>, cx: &App, read: impl FnOnce(&ConfigProfile) -> R) -> R {
  active_profile(store.read(cx).config()).map(read).unwrap_or_default()
}

impl ProfileEditor {
  pub fn new(stores: Stores, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let profile = active_profile(stores.config.read(cx).config()).cloned();
    let synced = Synced::of(profile.as_ref());
    let new_name = cx.new(|cx| InputState::new(window, cx).placeholder(tr!("settings.newProfileName")));
    let mut field = |value: &String, cx: &mut Context<Self>| {
      let value = value.clone();
      cx.new(|cx| InputState::new(window, cx).default_value(value))
    };
    let filters = [0, 1, 2].map(|index| field(&synced.filters[index], cx));
    let templates = [0, 1, 2, 3, 4].map(|index| field(&synced.templates[index], cx));
    let mut subscriptions = vec![
      cx.subscribe(&new_name, |_, _, event: &InputEvent, cx| {
        if matches!(event, InputEvent::Change) {
          cx.notify();
        }
      }),
      cx.observe_in(&stores.config, window, |this, _, window, cx| this.sync(window, cx)),
    ];
    for (index, kind) in TrackKind::FILTERED.into_iter().enumerate() {
      subscriptions.push(
        cx.subscribe(&filters[index], move |this, input, event: &InputEvent, cx| {
          if matches!(event, InputEvent::Change) {
            let value = input.read(cx).value().to_string();
            this.synced.filters[index] = value.clone();
            edit_profile(&this.stores.config, cx, |profile| kind.set_languages(profile, value));
          }
        }),
      );
    }
    for (index, kind) in TrackKind::ALL.into_iter().enumerate() {
      subscriptions.push(
        cx.subscribe(&templates[index], move |this, input, event: &InputEvent, cx| {
          if matches!(event, InputEvent::Change) {
            let value = input.read(cx).value().to_string();
            this.synced.templates[index] = value.clone();
            edit_profile(&this.stores.config, cx, |profile| kind.set_template(profile, value));
          }
        }),
      );
    }
    Self {
      stores,
      new_name,
      filters,
      templates,
      synced,
      _subscriptions: subscriptions,
    }
  }

  pub fn new_name(&self) -> &Entity<InputState> {
    &self.new_name
  }

  /// The language filter of `kind`; `None` for chapters and attachments.
  pub fn filter(&self, kind: TrackKind) -> Option<&Entity<InputState>> {
    kind.filter_index().map(|index| &self.filters[index])
  }

  pub fn template(&self, kind: TrackKind) -> &Entity<InputState> {
    &self.templates[kind as usize]
  }

  /// Puts the active profile's values into the fields whose value changed
  /// elsewhere (another profile, Reset), also into a focused one. Typing
  /// records its value before it writes, so it is never put back.
  fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let profile = active_profile(self.stores.config.read(cx).config()).cloned();
    let next = Synced::of(profile.as_ref());
    if next == self.synced {
      return;
    }
    let fields = (self.filters.iter().zip(self.synced.filters.iter().zip(&next.filters))).chain(
      self
        .templates
        .iter()
        .zip(self.synced.templates.iter().zip(&next.templates)),
    );
    for (input, (synced, value)) in fields {
      if synced != value && input.read(cx).value() != value.as_str() {
        let value = value.clone();
        input.update(cx, |input, cx| input.set_value(value, window, cx));
      }
    }
    self.synced = next;
    cx.notify();
  }

  fn add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let name = self.new_name.read(cx).value().to_string();
    let added = self.stores.config.update(cx, |store, cx| store.add_profile(&name, cx));
    if added {
      self.new_name.update(cx, |input, cx| input.set_value("", window, cx));
      cx.notify();
    }
  }

  /// The new-name field with Add, Delete and Reset.
  fn name_row(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
    let store = self.stores.config.read(cx);
    let can_add = store.can_add_profile(&self.new_name.read(cx).value());
    let can_delete = store.can_delete_active_profile();
    let name = store.config().active_profile.clone();
    let config = self.stores.config.clone();
    // The buttons move below the name field when it would get narrower than
    // FIELD_MIN_WIDTH.
    let buttons = h_flex()
      .flex_none()
      .gap(BUTTON_GROUP_GAP)
      .child(
        text_button("settings-profile-add")
          .outline()
          .small()
          .label(tr!("settings.addProfile"))
          .disabled(!can_add)
          .on_click(cx.listener(|this, _, window, cx| this.add(window, cx))),
      )
      .child(
        text_button("settings-profile-delete")
          .outline()
          .danger()
          .small()
          .label(tr!("settings.deleteProfile"))
          .disabled(!can_delete)
          .on_click({
            let (config, name) = (config.clone(), name.clone());
            move |_, window, cx| {
              let config = config.clone();
              confirm(
                window,
                cx,
                "settings-profile-delete",
                tr!("confirm.deleteProfileTitle"),
                tr!("confirm.deleteProfileMessage", name = name),
                move |cx| config.update(cx, |store, cx| store.delete_active_profile(cx)),
              );
            }
          }),
      )
      .child(
        text_button("settings-profile-reset")
          .outline()
          .small()
          .label(tr!("settings.reset"))
          .on_click(move |_, window, cx| {
            let config = config.clone();
            confirm(
              window,
              cx,
              "settings-profile-reset",
              tr!("confirm.resetProfileTitle"),
              tr!("confirm.resetProfileMessage", name = name),
              move |cx| config.update(cx, |store, cx| store.reset_active_profile(cx)),
            );
          }),
      );
    h_flex()
      .id("settings-profile-row")
      .test_support()
      .w_full()
      .flex_wrap()
      .gap(px(8.0))
      .child(
        div()
          .flex_1()
          .min_w(px(FIELD_MIN_WIDTH))
          .child(Input::new(&self.new_name).small().cleanable(true).w_full()),
      )
      .child(buttons)
  }

  /// A Track Selection row with an auto-select checkbox and a language filter.
  fn selection_row(&self, kind: TrackKind, options: &RenderOptions, cx: &App) -> impl IntoElement + use<> {
    let store = self.stores.config.clone();
    let checked = read_profile(&store, cx, |profile| kind.selects(profile));
    let filter = self.filter(kind).expect("a type with a language filter").clone();
    checkbox_field(
      options.layout(),
      Checkbox::new(SharedString::from(format!("settings-select-{}", kind.name())))
        .with_size(options.size())
        .disabled(options.is_disabled())
        .label(kind.label())
        .child(checkbox_description(tr!("settings.autoSelectOnDropForLanguages"), cx))
        .checked(checked)
        .on_change(move |checked, _, cx| edit_profile(&store, cx, |profile| kind.set_selects(profile, *checked))),
      Input::new(&filter)
        .small()
        .disabled(options.is_disabled())
        .cleanable(true)
        .w_full(),
    )
  }

  /// A template field, split into words as a file name is.
  fn template_field(&self, kind: TrackKind, options: &RenderOptions) -> impl IntoElement + use<> {
    let stacked = options.layout() == Axis::Vertical;
    div()
      .id(SharedString::from(format!("settings-template-{}", kind.name())))
      .test_support()
      .key_context(PATH_CONTEXT)
      .map(|field| {
        if stacked {
          field.w_full()
        } else {
          field.w(TEMPLATE_WIDTH).max_w_full()
        }
      })
      .child(
        Input::new(self.template(kind))
          .small()
          .disabled(options.is_disabled())
          .w_full(),
      )
  }
}

fn checkbox_field(layout: Axis, checkbox: Checkbox, field: impl IntoElement) -> impl IntoElement {
  let stacked = layout == Axis::Vertical;
  h_flex()
    .w_full()
    .gap_3()
    .when(stacked, |row| row.flex_col().items_start())
    .child(checkbox.when(!stacked, |checkbox| checkbox.flex_1().min_w_0()))
    .child(div().flex_none().w(FILTER_WIDTH).max_w_full().child(field))
}

/// A Yes / No question before `on_yes`; Enter and Escape answer No, as in
/// the exit dialog.
fn confirm(
  window: &mut Window,
  cx: &mut App,
  id: &'static str,
  title: SharedString,
  message: SharedString,
  on_yes: impl Fn(&mut App) + 'static,
) {
  let on_yes = Rc::new(on_yes);
  window.open_alert_dialog(cx, move |dialog, _, _| {
    let on_yes = on_yes.clone();
    dialog
      .title(title.clone())
      .description(message.clone())
      .on_ok(|_, _, _| true)
      .on_cancel(|_, _, _| true)
      .footer(
        DialogFooter::new()
          .child(
            text_button(SharedString::from(format!("{id}-no")))
              .outline()
              .label(tr!("confirm.no"))
              .on_click(|_, window, cx| window.close_dialog(cx)),
          )
          .child(
            confirm_button(SharedString::from(format!("{id}-yes")))
              .danger()
              .label(tr!("confirm.yes"))
              .on_click(move |_, window, cx| {
                on_yes(cx);
                window.close_dialog(cx);
              }),
          ),
      )
  });
}

/// A plain checkbox item of the active profile.
fn profile_checkbox(
  store: &Entity<ConfigStore>,
  id: &'static str,
  title: SharedString,
  description: Option<SharedString>,
  get: impl Fn(&ConfigProfile) -> bool + 'static,
  set: impl Fn(&mut ConfigProfile, bool) + 'static,
) -> SettingItem {
  let (get_store, set_store) = (store.clone(), store.clone());
  checkbox_item(
    id,
    title,
    description,
    move |cx| read_profile(&get_store, cx, &get),
    move |checked, cx| edit_profile(&set_store, cx, |profile| set(profile, checked)),
  )
}

pub fn page(stores: &Stores, editor: &Entity<ProfileEditor>, cx: &App) -> SettingPage {
  let store = stores.config.clone();
  let config = store.read(cx).config();
  let profiles: Vec<(SharedString, SharedString)> = config
    .profiles
    .iter()
    .map(|profile| (profile.name.clone().into(), profile.name.clone().into()))
    .collect();
  let (active_get, active_set) = (store.clone(), store.clone());
  let row_editor = editor.clone();
  let selection = TrackKind::ALL.map(|kind| {
    if kind.has_languages() {
      let editor = editor.clone();
      SettingItem::render(move |options, _, cx| editor.update(cx, |editor, cx| editor.selection_row(kind, options, cx)))
        .keywords([kind.label(), tr!("settings.autoSelectOnDropForLanguages")])
    } else {
      profile_checkbox(
        &store,
        match kind {
          TrackKind::Chapters => "settings-select-chapters",
          _ => "settings-select-attachments",
        },
        kind.label(),
        Some(tr!("settings.autoSelectOnDrop")),
        move |profile| kind.selects(profile),
        move |profile, value| kind.set_selects(profile, value),
      )
    }
  });
  let templates = TrackKind::ALL.map(|kind| {
    let editor = editor.clone();
    SettingItem::new(
      kind.label(),
      SettingField::render(move |options, _, cx| editor.read(cx).template_field(kind, options)),
    )
    .keywords([tr!("settings.fileNameTemplates")])
  });
  SettingPage::new(tr!("settings.profiles"))
    .icon(Icon::new(IconName::User))
    .resettable(false)
    .groups([
      SettingGroup::new().items(vec![
        SettingItem::new(
          tr!("settings.activeProfile"),
          SettingField::dropdown(
            profiles,
            move |cx| active_get.read(cx).config().active_profile.clone().into(),
            move |name, cx| active_set.update(cx, |store, cx| store.set_active_profile(&name, cx)),
          ),
        ),
        SettingItem::render(move |_, _, cx| row_editor.update(cx, |editor, cx| editor.name_row(cx))).keywords([
          tr!("settings.newProfileName"),
          tr!("settings.addProfile"),
          tr!("settings.deleteProfile"),
          tr!("settings.reset"),
        ]),
        profile_checkbox(
          &store,
          "settings-default-group-mode",
          tr!("settings.defaultGroupMode"),
          None,
          |profile| profile.default_group_mode,
          |profile, value| profile.default_group_mode = value,
        ),
      ]),
      SettingGroup::new()
        .title(tr!("settings.trackSelection"))
        .items(selection),
      SettingGroup::new()
        .title(tr!("settings.fileNameTemplates"))
        .items(templates)
        .footer(|_, _| {
          let hint = tr!("settings.templatePlaceholdersHint");
          div()
            .id("settings-template-placeholders")
            .test_support()
            .test_text(hint.clone())
            .child(hint)
        }),
    ])
}
