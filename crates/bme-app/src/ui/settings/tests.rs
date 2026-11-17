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

//! Headless tests of the Settings dialog in the application shell: every page
//! mounts, every control writes its config field (checked against the
//! serialised config), Reset All, the profile editor and the Group Mode its
//! operations set, the tool path rows with a stand-in detector, the word stops
//! of the path and template fields, and typing that never reverts.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bme_core::config::{Config, ConfigProfile, Language};
use core::prelude::v1::test;
use gpui_kit::component::input::InputState;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::*;
use serde_json::{Value, json};

use super::profiles::TrackKind;
use super::*;
use crate::tools::{Tool, ToolDetector, ToolStatus};
use crate::ui::tests::Shell;

const SELECT_ALL: &str = if cfg!(target_os = "macos") { "cmd-a" } else { "ctrl-a" };
const WORD_LEFT: &str = if cfg!(target_os = "macos") {
  "alt-left"
} else {
  "ctrl-left"
};

fn config() -> Config {
  Config {
    language: Language::EnUS,
    ..Config::default()
  }
}

type Calls = Arc<Mutex<Vec<(Tool, String, bool)>>>;

/// Finds MKVToolNix only in `/opt/mkvtoolnix` (also from `/opt/apps`, as a
/// path elsewhere, and for Detect as a running application) and
/// BetterMediaInfo only in `/opt/bmi`; records every call.
fn detector() -> (ToolDetector, Calls) {
  let calls = Calls::default();
  let seen = calls.clone();
  let tools = ToolDetector::new(move |tool, path, check_running| {
    seen.lock().unwrap().push((tool, path.to_string(), check_running));
    let found = |path: &str| ToolStatus {
      found: true,
      path: path.to_string(),
    };
    match (tool, path, check_running) {
      (Tool::MkvToolNix, _, true) => found("/run/mkvtoolnix"),
      (Tool::MkvToolNix, "/opt/mkvtoolnix", _) => found("/opt/mkvtoolnix"),
      (Tool::MkvToolNix, "/opt/apps", _) => found("/opt/mkvtoolnix"),
      (Tool::BetterMediaInfo, "/opt/bmi", _) => found("/opt/bmi"),
      _ => ToolStatus {
        found: false,
        path: path.to_string(),
      },
    }
  });
  (tools, calls)
}

/// The shell with the Settings dialog, tall enough to lay out every group.
fn open_with(cx: &mut TestAppContext, config: Config, tools: ToolDetector) -> (Shell, Entity<SettingsView>) {
  let shell = Shell::with_tools(cx, config, tools);
  shell.resize(cx, 1200.0, 3200.0);
  shell.window(cx, |window, _| window.activate_window());
  let view = shell.window(cx, |window, cx| {
    SettingsView::open(&shell.stores, window, cx).expect("open Settings")
  });
  draw(&shell, cx);
  (shell, view)
}

fn open(cx: &mut TestAppContext) -> (Shell, Entity<SettingsView>) {
  open_with(cx, config(), detector().0)
}

fn draw(shell: &Shell, cx: &mut TestAppContext) {
  cx.run_until_parked();
  shell.window(cx, |window, cx| window.render_frame(cx));
}

fn click_at(shell: &Shell, cx: &mut TestAppContext, position: Point<Pixels>) {
  VisualTestContext::from_window(shell.handle, cx).simulate_click(position, Modifiers::none());
  draw(shell, cx);
}

/// Clicks the one element inside the settings with this accessible name.
fn click_label(shell: &Shell, cx: &mut TestAppContext, label: &str) {
  draw(shell, cx);
  let center = shell.window(cx, |window, _| {
    window.within("settings").find_by_label(label).bounds().center()
  });
  click_at(shell, cx, center);
}

/// Clicks the checkbox inside the settings with this accessible name.
fn click_checkbox(shell: &Shell, cx: &mut TestAppContext, label: &str) {
  draw(shell, cx);
  let center = shell.window(cx, |window, _| {
    let checkbox = window
      .within("settings")
      .find_all_by_label(label)
      .into_iter()
      .find(|element| element.role() == Some(Role::CheckBox))
      .unwrap_or_else(|| panic!("missing checkbox {label:?}"));
    assert!(checkbox.bounds().size.width > px(0.0), "{label}");
    checkbox.bounds().center()
  });
  click_at(shell, cx, center);
}

/// Clicks a menu item, which opens outside the settings.
fn click_menu_item(shell: &Shell, cx: &mut TestAppContext, label: &str) {
  draw(shell, cx);
  let center = shell.window(cx, |window, _| {
    let items: Vec<_> = window
      .find_all_by_label(label)
      .into_iter()
      .filter(|item| item.role() == Some(Role::MenuItem) || item.role() == Some(Role::MenuItemCheckBox))
      .collect();
    assert_eq!(items.len(), 1, "one menu item {label:?}: {items:?}");
    items[0].bounds().center()
  });
  click_at(shell, cx, center);
}

fn click_id(shell: &Shell, cx: &mut TestAppContext, id: impl Into<ElementId>) {
  draw(shell, cx);
  let id = id.into();
  shell.window(cx, |window, cx| window.click(id, cx));
  draw(shell, cx);
}

/// The pages in sidebar order.
const PAGES: [&str; 4] = ["Appearance", "Integration", "Profiles", "Update"];

fn assert_button_gaps(shell: &Shell, cx: &mut TestAppContext, ids: &[&'static str]) {
  shell.window(cx, |window, _| {
    for pair in ids.windows(2) {
      let left = window.find(pair[0]).bounds();
      let right = window.find(pair[1]).bounds();
      assert_eq!(right.left() - left.right(), px(2.0), "{} and {}", pair[0], pair[1]);
    }
  });
}

/// Shows a page: its entry in the sidebar (gpui-kit names the entries by
/// their index).
fn page(shell: &Shell, cx: &mut TestAppContext, title: &str) {
  let index = PAGES.iter().position(|page| *page == title).expect("a page");
  draw(shell, cx);
  let center = shell.window(cx, |window, _| {
    window
      .within("settings")
      .find(SharedString::from(format!("0-{index}")))
      .bounds()
      .center()
  });
  // The label, away from a submenu's caret on the right.
  click_at(shell, cx, center - point(px(40.0), px(0.0)));
}

fn json_of(shell: &Shell, cx: &mut TestAppContext) -> Value {
  shell
    .stores
    .config
    .read_with(cx, |store, _| serde_json::to_value(store.config()).unwrap())
}

/// The leaves that differ, as JSON pointers.
fn changes(before: &Value, after: &Value) -> BTreeMap<String, Value> {
  fn walk(path: String, before: Option<&Value>, after: Option<&Value>, out: &mut BTreeMap<String, Value>) {
    match (before, after) {
      (Some(Value::Object(a)), Some(Value::Object(b))) => {
        for key in a.keys().chain(b.keys()).collect::<std::collections::BTreeSet<_>>() {
          walk(format!("{path}/{key}"), a.get(key), b.get(key), out);
        }
      }
      (Some(Value::Array(a)), Some(Value::Array(b))) => {
        for index in 0..a.len().max(b.len()) {
          walk(format!("{path}/{index}"), a.get(index), b.get(index), out);
        }
      }
      (a, b) if a != b => {
        out.insert(path, b.cloned().unwrap_or(Value::Null));
      }
      _ => {}
    }
  }
  let mut out = BTreeMap::new();
  walk(String::new(), Some(before), Some(after), &mut out);
  out
}

/// Runs `act`, then checks that exactly `expected` changed in the config and
/// that the newest write is the result.
fn assert_writes(
  shell: &Shell,
  cx: &mut TestAppContext,
  what: &str,
  act: impl FnOnce(&Shell, &mut TestAppContext),
  expected: &[(&str, Value)],
) {
  let before = json_of(shell, cx);
  act(shell, cx);
  draw(shell, cx);
  let after = json_of(shell, cx);
  let expected: BTreeMap<String, Value> = expected
    .iter()
    .map(|(path, value)| (path.to_string(), value.clone()))
    .collect();
  assert_eq!(changes(&before, &after), expected, "{what}");
  let written = shell.written.lock().unwrap().last().cloned().expect("a write");
  assert_eq!(serde_json::to_value(written).unwrap(), after, "{what}: persisted");
}

fn focus(shell: &Shell, cx: &mut TestAppContext, input: &Entity<InputState>) {
  let input = input.clone();
  shell.window(cx, |window, cx| input.update(cx, |input, cx| input.focus(window, cx)));
  cx.run_until_parked();
}

/// Replaces the text of a focused field by typing.
fn type_into(shell: &Shell, cx: &mut TestAppContext, input: &Entity<InputState>, text: &str) {
  focus(shell, cx, input);
  shell.window(cx, |window, cx| {
    window.press(SELECT_ALL, cx);
    if text.is_empty() {
      window.press("backspace", cx);
    } else {
      window.input(text, cx);
    }
  });
  draw(shell, cx);
}

fn value(cx: &mut TestAppContext, input: &Entity<InputState>) -> String {
  input.read_with(cx, |input, _| input.value().to_string())
}

/// The caret after each of `count` word-left presses in `input`.
fn word_left_stops(shell: &Shell, cx: &mut TestAppContext, input: &Entity<InputState>, count: usize) -> Vec<usize> {
  (0..count)
    .map(|_| {
      shell.press(cx, WORD_LEFT);
      input.read_with(cx, |input, _| input.cursor())
    })
    .collect()
}

fn texts(shell: &Shell, cx: &mut TestAppContext) -> Vec<String> {
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    gpui_kit::base::test_support::texts(window)
      .into_iter()
      .map(|text| text.text().to_string())
      .collect()
  })
}

fn profile(shell: &Shell, cx: &mut TestAppContext) -> ConfigProfile {
  shell.stores.config.read_with(cx, |store, _| {
    crate::state::config::active_profile(store.config()).cloned().unwrap()
  })
}

fn group_mode(shell: &Shell, cx: &mut TestAppContext) -> bool {
  shell.stores.files.read_with(cx, |files, _| files.group_mode())
}

fn editor_field(
  cx: &mut TestAppContext,
  view: &Entity<SettingsView>,
  field: impl FnOnce(&profiles::ProfileEditor) -> Entity<InputState>,
) -> Entity<InputState> {
  view.read_with(cx, |view, cx| field(view.profile().read(cx)))
}

#[gpui_kit::test]
fn every_page_and_group_mounts(cx: &mut TestAppContext) {
  let (shell, _) = open(cx);
  let pages: [(&str, &[&str]); 4] = [
    (
      "Appearance",
      &[
        "settings-mode-auto",
        "settings-mode-light",
        "settings-mode-dark",
        "settings-theme-option-Ocean",
      ],
    ),
    ("Integration", &["settings-mkvtoolnix", "settings-better-media-info"]),
    (
      "Profiles",
      &[
        "settings-profile-row",
        "settings-default-group-mode",
        "settings-select-video",
        "settings-select-subtitles",
        "settings-select-chapters",
        "settings-select-attachments",
        "settings-template-video",
        "settings-template-attachments",
      ],
    ),
    ("Update", &[]),
  ];
  for (title, ids) in pages {
    page(&shell, cx, title);
    shell.window(cx, |window, _| {
      for id in ids {
        window.find(SharedString::from(*id));
      }
    });
  }
  // Pages with titled groups list them in the sidebar.
  shell.window(cx, |window, _| {
    for title in ["Integration", "Profiles"] {
      let caret = format!("Toggle {title} submenu");
      assert!(window.within("settings").try_find_by_label(&caret).is_some(), "{caret}");
    }
    for title in ["Appearance", "Update"] {
      let caret = format!("Toggle {title} submenu");
      assert!(window.within("settings").try_find_by_label(&caret).is_none(), "{caret}");
    }
  });
  page(&shell, cx, "Update");
  assert!(shell.shows(cx, "Check New Version: Weekly"));
}

#[gpui_kit::test]
fn appearance_and_update_controls_write_their_fields(cx: &mut TestAppContext) {
  let (shell, _) = open(cx);
  page(&shell, cx, "Appearance");
  assert_button_gaps(
    &shell,
    cx,
    &["settings-mode-auto", "settings-mode-light", "settings-mode-dark"],
  );
  assert_writes(
    &shell,
    cx,
    "mode",
    |shell, cx| click_id(shell, cx, "settings-mode-dark"),
    &[("/displayMode", json!("Dark"))],
  );
  shell.window(cx, |window, _| {
    let origin = window.find("settings-theme-option-Ocean").bounds().origin;
    let step = px(32.0) + BUTTON_GROUP_GAP;
    let quads = window.painted_quads();
    for (index, spec) in crate::theme::THEMES.iter().enumerate() {
      let id = SharedString::from(format!("settings-theme-option-{}", spec.name));
      let swatch = window.find(id);
      let bounds = swatch.bounds();
      assert_eq!(
        bounds.origin,
        origin + point(step * (index % 5) as f32, step * (index / 5) as f32)
      );
      assert_eq!(bounds.size, size(px(32.0), px(32.0)));
      assert_eq!(swatch.checked(), Some(index == 0));
      assert!(
        quads.iter().any(|quad| {
          quad.bounds == bounds.scale(window.scale_factor())
            && quad.background.as_solid() == Some(crate::ui::hex(spec.main))
        }),
        "swatch for {}",
        spec.name
      );
    }
  });
  assert_writes(
    &shell,
    cx,
    "theme",
    |shell, cx| click_id(shell, cx, "settings-theme-option-Aqua"),
    &[("/theme", json!("Aqua"))],
  );
  shell.window(cx, |window, _| {
    assert_eq!(window.find("settings-theme-option-Ocean").checked(), Some(false));
    assert_eq!(window.find("settings-theme-option-Aqua").checked(), Some(true));
  });
  page(&shell, cx, "Update");
  assert_writes(
    &shell,
    cx,
    "check interval",
    |shell, cx| {
      click_label(shell, cx, "Check New Version: Weekly");
      click_menu_item(shell, cx, "Daily");
    },
    &[("/update/checkInterval", json!("Daily"))],
  );
  // Last, as it changes every text: the language.
  page(&shell, cx, "Appearance");
  assert_writes(
    &shell,
    cx,
    "language",
    |shell, cx| {
      click_label(shell, cx, "Language: English (US)");
      click_menu_item(shell, cx, "Deutsch");
    },
    &[("/language", json!("de"))],
  );
  // The dialog follows at once.
  assert!(shell.shows(cx, "Sprache: Deutsch"));
  crate::i18n::set_language(Language::EnUS);
}

#[gpui_kit::test]
fn reset_all_puts_a_page_back_to_its_defaults(cx: &mut TestAppContext) {
  let (shell, _) = open(cx);
  page(&shell, cx, "Appearance");
  // Nothing differs yet: no Reset All.
  assert!(shell.window(cx, |window, _| window.within("settings").try_find("reset").is_none()));
  click_id(&shell, cx, "settings-mode-light");
  click_id(&shell, cx, "settings-theme-option-Aqua");
  assert_writes(
    &shell,
    cx,
    "reset appearance",
    |shell, cx| click_id(shell, cx, "reset"),
    &[("/displayMode", json!("Auto")), ("/theme", json!("Ocean"))],
  );
  page(&shell, cx, "Update");
  click_label(&shell, cx, "Check New Version: Weekly");
  click_menu_item(&shell, cx, "Monthly");
  assert_writes(
    &shell,
    cx,
    "reset update",
    |shell, cx| click_id(shell, cx, "reset"),
    &[("/update/checkInterval", json!("Weekly"))],
  );
  // Profiles have their own Reset only.
  page(&shell, cx, "Profiles");
  click_checkbox(&shell, cx, "Attachments");
  assert!(profile(&shell, cx).select_attachments);
  assert!(shell.window(cx, |window, _| window.within("settings").try_find("reset").is_none()));
}

#[gpui_kit::test]
fn profiles_are_added_switched_deleted_and_reset_after_asking(cx: &mut TestAppContext) {
  let (shell, view) = open(cx);
  page(&shell, cx, "Profiles");
  assert_button_gaps(
    &shell,
    cx,
    &[
      "settings-profile-add",
      "settings-profile-delete",
      "settings-profile-reset",
    ],
  );
  let new_name = editor_field(cx, &view, |editor| editor.new_name().clone());
  type_into(&shell, cx, &new_name, " Anime ");
  assert_writes(
    &shell,
    cx,
    "add",
    |shell, cx| click_id(shell, cx, "settings-profile-add"),
    &[
      ("/activeProfile", json!("Anime")),
      // A new profile: Default's templates, nothing selected on its own.
      (
        "/profiles/1",
        serde_json::to_value(ConfigProfile::named("Anime")).unwrap(),
      ),
    ],
  );
  assert_eq!(value(cx, &new_name), "");
  // A taken or empty name cannot be added.
  for name in ["Default", "  "] {
    type_into(&shell, cx, &new_name, name);
    shell.window(cx, |window, _| {
      assert_eq!(window.find("settings-profile-add").disabled(), Some(true), "{name:?}");
    });
  }
  // The active profile from the select.
  assert_writes(
    &shell,
    cx,
    "switch",
    |shell, cx| {
      click_label(shell, cx, "Active Profile: Anime");
      click_menu_item(shell, cx, "Default");
    },
    &[("/activeProfile", json!("Default"))],
  );
  shell.window(cx, |window, _| {
    assert_eq!(
      window.find("settings-profile-delete").disabled(),
      Some(true),
      "Default stays"
    );
  });
  shell
    .stores
    .config
    .update(cx, |store, cx| store.set_active_profile("Anime", cx));
  draw(&shell, cx);
  // Delete asks; No keeps the profile.
  click_id(&shell, cx, "settings-profile-delete");
  click_id(&shell, cx, "settings-profile-delete-no");
  assert_eq!(profile(&shell, cx).name, "Anime");
  assert_writes(
    &shell,
    cx,
    "delete",
    |shell, cx| {
      click_id(shell, cx, "settings-profile-delete");
      click_id(shell, cx, "settings-profile-delete-yes");
    },
    &[("/activeProfile", json!("Default")), ("/profiles/1", Value::Null)],
  );
  // Reset asks and puts the active profile back to a fresh one of its name.
  click_id(&shell, cx, "settings-select-video");
  assert!(profile(&shell, cx).select_video);
  click_id(&shell, cx, "settings-profile-reset");
  shell.press(cx, "escape");
  assert!(profile(&shell, cx).select_video, "Escape answers No");
  assert_writes(
    &shell,
    cx,
    "reset",
    |shell, cx| {
      click_id(shell, cx, "settings-profile-reset");
      click_id(shell, cx, "settings-profile-reset-yes");
    },
    &[("/profiles/0/selectVideo", json!(false))],
  );
}

#[gpui_kit::test]
fn the_profile_operations_set_group_mode(cx: &mut TestAppContext) {
  let (shell, view) = open(cx);
  page(&shell, cx, "Profiles");
  // Default's defaultGroupMode starts the session with Group Mode on.
  assert!(group_mode(&shell, cx));
  assert_writes(
    &shell,
    cx,
    "default group mode",
    |shell, cx| click_checkbox(shell, cx, "Default group mode"),
    &[("/profiles/0/defaultGroupMode", json!(false))],
  );
  assert!(!group_mode(&shell, cx), "the checkbox sets Group Mode");
  click_checkbox(&shell, cx, "Default group mode");
  assert!(group_mode(&shell, cx));
  // Each operation sets Group Mode from the profile, over a session toggle.
  let new_name = editor_field(cx, &view, |editor| editor.new_name().clone());
  type_into(&shell, cx, &new_name, "Anime");
  click_id(&shell, cx, "settings-profile-add");
  assert!(!group_mode(&shell, cx), "a new profile groups nothing");
  shell.press(cx, "f8");
  assert!(group_mode(&shell, cx));
  click_label(&shell, cx, "Active Profile: Anime");
  click_menu_item(&shell, cx, "Anime");
  assert!(!group_mode(&shell, cx), "also when the profile was active");
  click_label(&shell, cx, "Active Profile: Anime");
  click_menu_item(&shell, cx, "Default");
  assert!(group_mode(&shell, cx));
  shell
    .stores
    .config
    .update(cx, |store, cx| store.set_active_profile("Anime", cx));
  assert!(!group_mode(&shell, cx));
  click_id(&shell, cx, "settings-profile-delete");
  click_id(&shell, cx, "settings-profile-delete-yes");
  assert_eq!(profile(&shell, cx).name, "Default");
  assert!(group_mode(&shell, cx), "Default's after a delete");
  shell.press(cx, "f8");
  assert!(!group_mode(&shell, cx));
  click_id(&shell, cx, "settings-profile-reset");
  click_id(&shell, cx, "settings-profile-reset-yes");
  assert!(group_mode(&shell, cx), "a fresh Default groups");
}

#[gpui_kit::test]
fn track_selection_writes_the_active_profile(cx: &mut TestAppContext) {
  let (shell, view) = open(cx);
  page(&shell, cx, "Profiles");
  let checkboxes = [
    ("Video", "/profiles/0/selectVideo", true),
    ("Audio", "/profiles/0/selectAudio", true),
    ("Subtitles", "/profiles/0/selectSubtitle", false),
    ("Chapters", "/profiles/0/selectChapters", true),
    ("Attachments", "/profiles/0/selectAttachments", true),
  ];
  for (label, path, value) in checkboxes {
    assert_writes(
      &shell,
      cx,
      label,
      |shell, cx| click_checkbox(shell, cx, label),
      &[(path, json!(value))],
    );
  }
  let filters = [
    (TrackKind::Video, "jpn, eng", "/profiles/0/videoLanguages"),
    (TrackKind::Audio, "eng", "/profiles/0/audioLanguages"),
    (TrackKind::Subtitles, "", "/profiles/0/subtitleLanguages"),
  ];
  for (kind, text, path) in filters {
    let input = editor_field(cx, &view, |editor| editor.filter(kind).unwrap().clone());
    assert_writes(
      &shell,
      cx,
      kind.name(),
      |shell, cx| type_into(shell, cx, &input, text),
      &[(path, json!(text))],
    );
  }
  assert!(view.read_with(cx, |view, cx| {
    view.profile().read(cx).filter(TrackKind::Chapters).is_none()
  }));
}

#[gpui_kit::test]
fn templates_write_the_active_profile_and_follow_another_profile(cx: &mut TestAppContext) {
  let (shell, view) = open(cx);
  page(&shell, cx, "Profiles");
  assert!(
    texts(&shell, cx).iter().any(
      |text| text == "Placeholders: {file_name}, {track_id}, {track_number}, {language}, {codec_name}, {track_name}"
    ),
    "the placeholders below the templates"
  );
  let paths = [
    (TrackKind::Video, "/profiles/0/videoTemplate"),
    (TrackKind::Audio, "/profiles/0/audioTemplate"),
    (TrackKind::Subtitles, "/profiles/0/subtitleTemplate"),
    (TrackKind::Chapters, "/profiles/0/chaptersTemplate"),
    (TrackKind::Attachments, "/profiles/0/attachmentsTemplate"),
  ];
  for (kind, path) in paths {
    let input = editor_field(cx, &view, |editor| editor.template(kind).clone());
    assert_eq!(value(cx, &input), kind.template(&ConfigProfile::default()));
    let text = format!("{{file_name}}_{}", kind.name());
    assert_writes(
      &shell,
      cx,
      kind.name(),
      |shell, cx| type_into(shell, cx, &input, &text),
      &[(path, json!(text))],
    );
  }
  // Another active profile fills in its templates, also into the field being
  // edited; typing then edits that profile.
  let video = editor_field(cx, &view, |editor| editor.template(TrackKind::Video).clone());
  focus(&shell, cx, &video);
  shell
    .stores
    .config
    .update(cx, |store, cx| assert!(store.add_profile("Anime", cx)));
  draw(&shell, cx);
  assert_eq!(value(cx, &video), "{file_name}.{track_id}.{language}");
  type_into(&shell, cx, &video, "{track_name}");
  assert_eq!(profile(&shell, cx).video_template, "{track_name}");
  assert_eq!(
    json_of(&shell, cx)["profiles"][0]["videoTemplate"],
    json!("{file_name}_video")
  );
  shell
    .stores
    .config
    .update(cx, |store, cx| store.set_active_profile("Default", cx));
  draw(&shell, cx);
  assert_eq!(value(cx, &video), "{file_name}_video");
  // Reset puts the fresh templates back into the fields.
  click_id(&shell, cx, "settings-profile-reset");
  click_id(&shell, cx, "settings-profile-reset-yes");
  for (kind, _) in paths {
    let input = editor_field(cx, &view, |editor| editor.template(kind).clone());
    assert_eq!(value(cx, &input), kind.template(&ConfigProfile::default()), "{kind:?}");
  }
}

#[gpui_kit::test]
fn rapid_typing_never_reverts_while_writes_are_pending(cx: &mut TestAppContext) {
  let (shell, view) = open(cx);
  page(&shell, cx, "Profiles");
  let input = editor_field(cx, &view, |editor| editor.template(TrackKind::Audio).clone());
  type_into(&shell, cx, &input, "");
  let writes_before = shell.written.lock().unwrap().len();
  let text: String = "abcdefghij".repeat(5);
  for (index, character) in text.char_indices() {
    // No run until parked: every write is still pending while typing on.
    shell.window(cx, |window, cx| window.input(&character.to_string(), cx));
    let typed = &text[..=index];
    assert_eq!(value(cx, &input), typed);
    assert_eq!(profile(&shell, cx).audio_template, typed);
    if index % 10 == 9 {
      // Some frames in between, as a user typing would see.
      shell.window(cx, |window, cx| window.render_frame(cx));
      assert_eq!(value(cx, &input), typed);
    }
  }
  cx.run_until_parked();
  draw(&shell, cx);
  assert_eq!(value(cx, &input), text);
  let written = shell.written.lock().unwrap();
  assert!(written.len() > writes_before);
  assert_eq!(written.last().unwrap().profiles[0].audio_template, text);
}

#[gpui_kit::test]
fn tool_paths_are_detected_after_typing_stored_on_blur_and_found_elsewhere(cx: &mut TestAppContext) {
  let (tools, calls) = detector();
  let (shell, view) = open_with(cx, config(), tools);
  page(&shell, cx, "Integration");
  assert_button_gaps(
    &shell,
    cx,
    &["settings-mkvtoolnix-browse", "settings-mkvtoolnix-detect"],
  );
  assert_button_gaps(
    &shell,
    cx,
    &["settings-better-media-info-browse", "settings-better-media-info-detect"],
  );
  cx.executor().advance_clock(Duration::from_millis(300));
  draw(&shell, cx);
  let row = view.read_with(cx, |view, _| view.tool_row(Tool::MkvToolNix).clone());
  let input = row.read_with(cx, |row, _| row.input().clone());
  // The stored default path was checked once it showed.
  assert_eq!(row.read_with(cx, |row, _| row.found()), Some(false));
  assert!(
    texts(&shell, cx)
      .iter()
      .any(|text| text == "mkvmerge or mkvextract not found under MKVToolNix path.")
  );
  // Typing: checked 250 ms after the last key, not stored yet.
  calls.lock().unwrap().clear();
  type_into(&shell, cx, &input, " /opt/mkvtoolnix ");
  cx.executor().advance_clock(Duration::from_millis(100));
  cx.run_until_parked();
  assert!(calls.lock().unwrap().is_empty(), "debounced");
  cx.executor().advance_clock(Duration::from_millis(200));
  draw(&shell, cx);
  assert_eq!(
    calls.lock().unwrap().as_slice(),
    [(Tool::MkvToolNix, "/opt/mkvtoolnix".to_string(), false)]
  );
  assert_eq!(row.read_with(cx, |row, _| row.found()), Some(true));
  assert!(
    texts(&shell, cx)
      .iter()
      .any(|text| text == "mkvmerge and mkvextract found under MKVToolNix path.")
  );
  assert_eq!(
    json_of(&shell, cx)["integration"]["mkvToolNixPath"],
    json!(Tool::MkvToolNix.default_path())
  );
  // Leaving the field stores the trimmed text.
  let other = view.read_with(cx, |view, cx| {
    view.tool_row(Tool::BetterMediaInfo).read(cx).input().clone()
  });
  assert_writes(
    &shell,
    cx,
    "blur",
    |shell, cx| focus(shell, cx, &other),
    &[("/integration/mkvToolNixPath", json!("/opt/mkvtoolnix"))],
  );
  // A path where the tool is found elsewhere fills in that path.
  type_into(&shell, cx, &input, "/opt/apps");
  assert_writes(
    &shell,
    cx,
    "found elsewhere",
    |_, cx| {
      cx.executor().advance_clock(Duration::from_millis(300));
      cx.run_until_parked();
    },
    &[],
  );
  assert_eq!(value(cx, &input), "/opt/mkvtoolnix");
  // Detect looks for a running application at once.
  calls.lock().unwrap().clear();
  assert_writes(
    &shell,
    cx,
    "detect",
    |shell, cx| click_id(shell, cx, "settings-mkvtoolnix-detect"),
    &[("/integration/mkvToolNixPath", json!("/run/mkvtoolnix"))],
  );
  assert_eq!(
    calls.lock().unwrap().as_slice(),
    [(Tool::MkvToolNix, "/opt/mkvtoolnix".to_string(), true)]
  );
  assert_eq!(value(cx, &input), "/run/mkvtoolnix");
  // Browse stores the chosen folder.
  assert_writes(
    &shell,
    cx,
    "browse",
    |shell, cx| {
      click_id(shell, cx, "settings-better-media-info-browse");
      assert!(cx.did_prompt_for_paths());
      cx.simulate_path_prompt_response(|_| Some(vec![std::path::PathBuf::from("/opt/bmi")]));
      cx.executor().advance_clock(Duration::from_millis(300));
      draw(shell, cx);
    },
    &[("/integration/betterMediaInfoPath", json!("/opt/bmi"))],
  );
  let bmi = view.read_with(cx, |view, _| view.tool_row(Tool::BetterMediaInfo).clone());
  assert_eq!(bmi.read_with(cx, |row, _| row.found()), Some(true));
  assert!(texts(&shell, cx).iter().any(|text| text == "BetterMediaInfo found"));
  // Reset All puts both paths back.
  assert_writes(
    &shell,
    cx,
    "reset all",
    |shell, cx| {
      click_id(shell, cx, "reset");
      cx.executor().advance_clock(Duration::from_millis(300));
      draw(shell, cx);
    },
    &[
      ("/integration/betterMediaInfoPath", json!("")),
      ("/integration/mkvToolNixPath", json!(Tool::MkvToolNix.default_path())),
    ],
  );
}

#[gpui_kit::test]
fn word_keys_stop_at_every_dot_of_a_tool_path(cx: &mut TestAppContext) {
  let (shell, view) = open(cx);
  page(&shell, cx, "Integration");
  let input = view.read_with(cx, |view, cx| view.tool_row(Tool::MkvToolNix).read(cx).input().clone());
  type_into(&shell, cx, &input, "/Applications/MKVToolNix.app");
  assert_eq!(word_left_stops(&shell, cx, &input, 2), [25, 14]);
}

#[gpui_kit::test]
fn word_keys_split_a_template_as_a_file_name(cx: &mut TestAppContext) {
  let (shell, view) = open(cx);
  page(&shell, cx, "Profiles");
  let text = "{file_name}.eng.forced";
  // `eng.forced` is one word to the OS; in a template, as in a path, its
  // parts are words of their own.
  let template = editor_field(cx, &view, |editor| editor.template(TrackKind::Subtitles).clone());
  type_into(&shell, cx, &template, text);
  assert_eq!(word_left_stops(&shell, cx, &template, 2), [16, 12]);
  // A language filter keeps the OS's words.
  let filter = editor_field(cx, &view, |editor| editor.filter(TrackKind::Subtitles).unwrap().clone());
  type_into(&shell, cx, &filter, text);
  assert_eq!(word_left_stops(&shell, cx, &filter, 1), [12]);
}

#[gpui_kit::test]
fn the_settings_shortcut_opens_one_fixed_width_dialog_over_the_current_tab(cx: &mut TestAppContext) {
  use crate::ui::actions::SETTINGS_SHORTCUT;

  let shell = Shell::with_config(cx, config());
  shell.resize(cx, 1200.0, 800.0);
  shell.press(cx, "ctrl-2");
  shell.press(cx, SETTINGS_SHORTCUT);
  let dialog_width = |cx: &mut TestAppContext| {
    draw(&shell, cx);
    shell.window(cx, |window, _| window.find("surface").bounds().size.width)
  };
  assert_eq!(dialog_width(cx), DIALOG_WIDTH);
  shell.resize(cx, 1040.0, 760.0);
  assert_eq!(dialog_width(cx), DIALOG_WIDTH);
  shell.resize(cx, 600.0, 450.0);
  assert!(dialog_width(cx) < px(600.0));
  page(&shell, cx, "Update");
  assert!(shell.shows(cx, "Check New Version: Weekly"));
  shell.press(cx, SETTINGS_SHORTCUT);
  assert!(shell.shows(cx, "Check New Version: Weekly"));
  shell.press(cx, "escape");
  assert!(!shell.has_dialog(cx));
  assert_eq!(shell.active_tab(cx), crate::state::TabType::Queue);
  shell.press(cx, "f10");
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.within("settings").find("settings-mode-auto");
  });
  assert!(!shell.shows(cx, "Check New Version: Weekly"));
  shell.press(cx, "escape");
  // The toolbar button and the macOS menu item's action open it too.
  shell.resize(cx, 1200.0, 800.0);
  shell.click(cx, "toolbar-settings");
  assert!(shell.has_dialog(cx));
  shell.press(cx, "escape");
  shell.window(cx, |window, cx| {
    window.dispatch_action(Box::new(crate::ui::actions::OpenSettings), cx)
  });
  cx.run_until_parked();
  assert!(shell.has_dialog(cx));
  assert_eq!(shell.active_tab(cx), crate::state::TabType::Queue);
}

/// How far below the top of `row` the element `id` starts.
fn offset_in_row(shell: &Shell, cx: &mut TestAppContext, row: &'static str, id: &'static str) -> Pixels {
  draw(shell, cx);
  shell.window(cx, |window, _| {
    window.find(id).bounds().top() - window.find(row).bounds().top()
  })
}

#[gpui_kit::test]
fn rows_with_buttons_wrap_them_below_the_field_when_narrow(cx: &mut TestAppContext) {
  let (shell, _) = open_with(
    cx,
    Config {
      language: Language::De,
      ..Config::default()
    },
    detector().0,
  );
  let rows = [
    ("Integration", "settings-mkvtoolnix", "settings-mkvtoolnix-browse"),
    ("Profiles", "settings-profile-row", "settings-profile-add"),
  ];
  for (title, row, button) in rows {
    page(&shell, cx, title);
    assert!(
      offset_in_row(&shell, cx, row, button) < px(10.0),
      "{button} beside the field when wide"
    );
  }
  // Narrower than the field's minimum and the buttons (the test platform's
  // text is narrower than a real font's, so not the 600 px minimum window);
  // tall enough for every group.
  shell.resize(cx, 480.0, 3200.0);
  for (title, row, button) in rows {
    page(&shell, cx, title);
    assert!(
      offset_in_row(&shell, cx, row, button) > px(24.0),
      "{button} below the field when narrow"
    );
  }
}
