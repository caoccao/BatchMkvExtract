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

//! Headless smoke tests of the application shell.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use bme_core::config::{Config, ConfigWindow, DisplayMode, Language, Theme as ThemeName};
use bme_core::protocol::UpdateCheckResult;
use core::prelude::v1::test;
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::*;

use super::actions;
use super::root::AppView;
use crate::i18n::tests::LOCALE_LOCK;
use crate::platform::close_guard;
use crate::state::ui::{NotificationKind, ToolbarMenu};
use crate::state::{Persistence, Services, Stores, TabType};

/// The shell in a test window, in English. The locale is process-wide, and a
/// test switching it would change the text width under another test's click,
/// so every shell test holds the locale lock while it runs.
pub(super) struct Shell {
  pub(super) handle: AnyWindowHandle,
  pub(super) stores: Stores,
  /// Every config the store wrote, in order.
  pub(super) written: Arc<Mutex<Vec<Config>>>,
  _locale: MutexGuard<'static, ()>,
}

impl Shell {
  fn open(cx: &mut TestAppContext) -> Self {
    Self::with_config(cx, Config::default())
  }

  /// The shell with `config` (its language is set to English).
  pub(super) fn with_config(cx: &mut TestAppContext, config: Config) -> Self {
    let locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let written = Arc::new(Mutex::new(Vec::new()));
    let sink = written.clone();
    let (handle, stores) = cx.update(|cx| {
      gpui_kit::init(cx);
      crate::theme::install(cx);
      let config = Config {
        language: Language::EnUS,
        ..config
      };
      crate::theme::apply(&config, cx.window_appearance(), cx);
      let services = Services {
        persistence: Persistence::new(move |config| sink.lock().unwrap().push(config)),
      };
      let stores = Stores::new(config, services, cx);
      actions::bind_keys(cx);
      actions::install(&stores, cx);
      let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
          point(px(40.0), px(30.0)),
          size(px(1000.0), px(760.0)),
        ))),
        ..Default::default()
      };
      let view_stores = stores.clone();
      let (handle, _) = gpui_kit::open_window(options, cx, |window, cx| {
        cx.new(|cx| AppView::new(view_stores, window, cx))
      })
      .expect("open the main window");
      (handle, stores)
    });
    let shell = Self {
      handle,
      stores,
      written,
      _locale: locale,
    };
    shell.window(cx, |window, cx| window.render_frame(cx));
    shell
  }

  /// Resizes the window's content and draws a frame.
  pub(super) fn resize(&self, cx: &mut TestAppContext, width: f32, height: f32) {
    cx.simulate_window_resize(self.handle, size(px(width), px(height)));
    cx.run_until_parked();
    self.window(cx, |window, cx| window.render_frame(cx));
  }

  pub(super) fn window<R>(&self, cx: &mut TestAppContext, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
    cx.update_window(self.handle, |_, window, cx| f(window, cx))
      .expect("window open")
  }

  pub(super) fn active_tab(&self, cx: &mut TestAppContext) -> TabType {
    self.stores.ui.read_with(cx, |ui, _| ui.active_tab())
  }

  pub(super) fn press(&self, cx: &mut TestAppContext, key: &str) {
    self.window(cx, |window, cx| window.press(key, cx));
    cx.run_until_parked();
  }

  pub(super) fn click(&self, cx: &mut TestAppContext, id: &'static str) {
    self.window(cx, |window, cx| window.click(id, cx));
    cx.run_until_parked();
  }

  /// Whether an element with this accessible name (a tab, a labelled button)
  /// is on screen.
  pub(super) fn shows(&self, cx: &mut TestAppContext, label: &str) -> bool {
    self.window(cx, |window, cx| {
      window.render_frame(cx);
      window.try_find_by_label(label).is_some()
    })
  }

  pub(super) fn disabled(&self, cx: &mut TestAppContext, id: &'static str) -> bool {
    self.window(cx, |window, cx| {
      window.render_frame(cx);
      window.find(id).disabled() == Some(true)
    })
  }

  fn has_dialog(&self, cx: &mut TestAppContext) -> bool {
    self.window(cx, |window, cx| window.has_active_dialog(cx))
  }
}

#[gpui_kit::test]
fn the_main_window_opens_with_its_title_and_minimum_size(cx: &mut TestAppContext) {
  let options = cx.update(|cx| crate::platform::window::main_window_options(&ConfigWindow::default(), cx));
  let title = options.titlebar.as_ref().and_then(|titlebar| titlebar.title.clone());
  assert_eq!(title, Some(crate::window_title()));
  assert_eq!(options.window_min_size, Some(size(px(600.0), px(450.0))));
  assert_eq!(options.app_id.as_deref(), Some(crate::DESKTOP_ID));
  let Some(WindowBounds::Windowed(bounds)) = options.window_bounds else {
    panic!("a windowed window");
  };
  assert_eq!(bounds.size, size(px(1200.0), px(900.0)), "0.3.0's default size");
}

#[gpui_kit::test]
fn the_shell_mounts_with_the_welcome_page_while_empty(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  shell.window(cx, |window, _| {
    for id in ["app-root", "toolbar", "main-content", "welcome", "status-bar"] {
      window.find(id);
    }
    let root = window.find("app-root").bounds();
    let bar = window.find("status-bar").bounds();
    let links = window.find("status-bar-links").bounds();
    assert_eq!(
      (bar.left(), bar.right(), bar.bottom()),
      (root.left(), root.right(), root.bottom())
    );
    assert!((links.center().x - bar.center().x).abs() <= px(0.5));
    // 0.3.0's order: this app first, then its siblings.
    let cards = [
      "welcome-card-batch-mkv-extract",
      "welcome-card-batch-mkv-merge",
      "welcome-card-better-media-info",
      "welcome-card-bd-master",
    ]
    .map(|id| window.find(id).bounds());
    // They wrap: each card is right of the one before it or on a lower row.
    assert!(
      cards
        .windows(2)
        .all(|pair| (pair[0].top(), pair[0].left()) < (pair[1].top(), pair[1].left())),
      "{cards:?}"
    );
  });
  // The file list cannot take files yet.
  assert!(shell.disabled(cx, "welcome-add-files"));
  assert!(shell.disabled(cx, "welcome-add-folder"));
  shell.window(cx, |window, _| {
    for id in [
      "welcome-card-batch-mkv-extract-github",
      "welcome-card-batch-mkv-merge-github",
      "welcome-card-better-media-info-github",
      "welcome-card-bd-master-github",
    ] {
      assert_eq!(window.find(id).label(), Some("View on GitHub"), "{id}");
    }
  });
  shell.press(cx, "ctrl-2");
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.find("queue-empty");
    assert!(window.try_find("welcome").is_none());
  });
}

#[gpui_kit::test]
fn the_welcome_page_fills_the_tab_in_a_tall_window(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  shell.resize(cx, 1400.0, 1600.0);
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    let toolbar = window.find("toolbar").bounds();
    let file_tab = window.find_by_label("File List").bounds();
    let root = window.find("app-root").bounds();
    let status_bar = window.find("status-bar").bounds();
    let welcome = window.find("welcome").bounds();
    let resume = window.find("toolbar-resume-all").bounds();
    let cancel = window.find("toolbar-cancel-all").bounds();
    assert_eq!(toolbar.top() - root.top(), px(2.0));
    assert_eq!(cancel.left() - resume.right(), px(2.0));
    // The tab starts after the strip's 2 px top margin and 1 px top border.
    assert_eq!(file_tab.top() - toolbar.bottom(), px(3.0));
    assert_eq!(welcome.top() - file_tab.bottom(), px(2.0));
    assert_eq!(welcome.left() - root.left(), px(2.0));
    assert_eq!(root.right() - welcome.right(), px(2.0));
    assert_eq!(status_bar.top() - welcome.bottom(), px(2.0));
  });
}

#[gpui_kit::test]
fn the_toolbar_opens_about_as_a_tab(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  shell.click(cx, "toolbar-about");
  assert_eq!(shell.active_tab(cx), TabType::About);
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.find("about-author");
    window.find("about-github");
  });
  shell.click(cx, "tab-close-about");
  assert_eq!(shell.active_tab(cx), TabType::Queue);
  assert_eq!(
    shell.stores.ui.read_with(cx, |ui, _| ui.tabs()),
    [TabType::FileList, TabType::Queue]
  );
}

#[gpui_kit::test]
fn the_toolbar_enables_only_what_the_shell_can_do(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  for id in [
    "toolbar-global-output",
    "toolbar-extract-all",
    "toolbar-resume-all",
    "toolbar-cancel-all",
    "toolbar-clear-completed",
    "toolbar-clear",
    "toolbar-group-mode",
    "toolbar-settings",
  ] {
    assert!(shell.disabled(cx, id), "{id} waits for its store");
  }
  for id in [
    "toolbar-quit-when-complete",
    "toolbar-system-notification",
    "toolbar-topmost-notification",
    "toolbar-profile",
    "toolbar-about",
  ] {
    assert!(!shell.disabled(cx, id), "{id}");
  }
  // The completion toggles are session state of the shell.
  shell.click(cx, "toolbar-quit-when-complete");
  shell.click(cx, "toolbar-system-notification");
  shell.click(cx, "toolbar-topmost-notification");
  let toggles = shell.stores.ui.read_with(cx, |ui, _| {
    (
      ui.quit_when_complete(),
      ui.system_notification(),
      ui.topmost_notification(),
    )
  });
  assert_eq!(toggles, (true, true, true));
  shell.click(cx, "toolbar-system-notification");
  assert!(!shell.stores.ui.read_with(cx, |ui, _| ui.system_notification()));
  // The shortcuts of the disabled buttons do nothing yet, and F10 opens no
  // dialog before Settings exists.
  for key in ["f3", "f4", "f8", "ctrl-q", "f10"] {
    shell.press(cx, key);
  }
  assert!(!shell.has_dialog(cx));
  assert_eq!(shell.active_tab(cx), TabType::FileList);
  assert!(shell.written.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn tab_shortcuts_cycle_select_and_close(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  shell.click(cx, "toolbar-about");
  assert_eq!(shell.active_tab(cx), TabType::About);
  shell.press(cx, "ctrl-tab");
  assert_eq!(shell.active_tab(cx), TabType::FileList);
  shell.press(cx, "ctrl-shift-tab");
  assert_eq!(shell.active_tab(cx), TabType::About);
  shell.press(cx, "ctrl-2");
  assert_eq!(shell.active_tab(cx), TabType::Queue);
  shell.press(cx, "ctrl-9");
  assert_eq!(shell.active_tab(cx), TabType::Queue);
  shell.press(cx, "ctrl-w");
  assert_eq!(shell.active_tab(cx), TabType::Queue, "Queue cannot be closed");
  shell.press(cx, "ctrl-3");
  assert_eq!(shell.active_tab(cx), TabType::About);
  shell.press(cx, "ctrl-w");
  assert_eq!(shell.active_tab(cx), TabType::Queue);
  assert_eq!(shell.stores.ui.read_with(cx, |ui, _| ui.tabs().len()), 2);
  shell.press(cx, "ctrl-1");
  assert_eq!(shell.active_tab(cx), TabType::FileList);
}

#[gpui_kit::test]
fn switching_the_language_changes_the_ui_text(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  assert!(shell.shows(cx, "File List"));
  assert!(shell.shows(cx, "Add Files"));
  assert!(shell.shows(cx, "Extract All"));
  assert!(
    shell.shows(cx, "Clear (Ctrl + Q)"),
    "icon buttons are named by their tooltip"
  );
  let (settings_en, settings_de) = if cfg!(target_os = "macos") {
    ("Settings (Cmd + ,)", "Einstellungen (Cmd + ,)")
  } else {
    ("Settings (Ctrl + ,)", "Einstellungen (Strg + ,)")
  };
  assert!(shell.shows(cx, settings_en));
  shell.stores.config.update(cx, |config, cx| {
    config.update(cx, |config| config.language = Language::De)
  });
  cx.run_until_parked();
  assert!(shell.shows(cx, "Dateiliste"));
  assert!(shell.shows(cx, "Dateien hinzufügen"));
  assert!(shell.shows(cx, settings_de));
  assert!(!shell.shows(cx, "File List"));
  let (width, height) = crate::platform::window::MIN_WINDOW_SIZE;
  shell.resize(cx, width as f32, height as f32);
  shell.window(cx, |window, _| {
    let toolbar = window.find("toolbar").bounds();
    let extract = window.find_by_label("Alle extrahieren").bounds();
    let overflow = window.find("toolbar-overflow").bounds();
    assert!(extract.size.width > px(40.0));
    assert!(extract.right() <= overflow.left() && overflow.right() <= toolbar.right());
    assert_eq!(extract.top(), overflow.top());
  });
  shell.stores.config.update(cx, |config, cx| {
    config.update(cx, |config| config.language = Language::EnUS)
  });
  cx.run_until_parked();
  assert!(shell.shows(cx, "File List"));
}

#[gpui_kit::test]
fn switching_the_theme_and_mode_restyles_the_window_at_once(cx: &mut TestAppContext) {
  let shell = Shell::with_config(
    cx,
    Config {
      display_mode: DisplayMode::Light,
      ..Config::default()
    },
  );
  let primary = |shell: &Shell, cx: &mut TestAppContext| {
    shell.window(cx, |window, cx| {
      window.render_frame(cx);
      (cx.theme().primary, cx.theme().is_dark())
    })
  };
  let ocean: Hsla = rgb(crate::theme::spec(ThemeName::Ocean).main).into();
  assert_eq!(primary(&shell, cx), (ocean, false));
  shell.stores.config.update(cx, |config, cx| {
    config.update(cx, |config| config.theme = ThemeName::Rose)
  });
  cx.run_until_parked();
  let rose: Hsla = rgb(crate::theme::spec(ThemeName::Rose).main).into();
  assert_eq!(primary(&shell, cx), (rose, false));
  shell.stores.config.update(cx, |config, cx| {
    config.update(cx, |config| config.display_mode = DisplayMode::Dark)
  });
  cx.run_until_parked();
  assert!(primary(&shell, cx).1, "dark");
  shell.stores.config.update(cx, |config, cx| {
    config.update(cx, |config| config.display_mode = DisplayMode::Light)
  });
  cx.run_until_parked();
  assert!(!primary(&shell, cx).1, "light again");
}

#[gpui_kit::test]
fn a_toast_appears_in_the_window(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  shell.stores.ui.update(cx, |ui, cx| {
    ui.show_toast(
      NotificationKind::Success,
      "/media/a.mkv".into(),
      "Completed in 00:00:01".into(),
      cx,
    )
  });
  cx.run_until_parked();
  let count = shell.window(cx, |window, cx| window.notifications(cx).len());
  assert_eq!(count, 1);
}

#[gpui_kit::test]
fn the_close_guard_asks_only_while_extractions_are_active(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  let confirmed = std::rc::Rc::new(std::cell::Cell::new(false));
  let closes = shell.window(cx, |window, cx| close_guard::should_close(false, window, cx, |_, _| {}));
  assert!(closes);
  assert!(!shell.has_dialog(cx));

  let flag = confirmed.clone();
  let closes = shell.window(cx, |window, cx| {
    close_guard::should_close(true, window, cx, move |_, _| flag.set(true))
  });
  assert!(!closes);
  cx.run_until_parked();
  assert!(shell.has_dialog(cx));
  // A second request while the dialog is open does not stack another one.
  shell.window(cx, |window, cx| close_guard::should_close(true, window, cx, |_, _| {}));
  shell.window(cx, |window, cx| window.render_frame(cx));
  shell.click(cx, "exit-no");
  assert!(!confirmed.get());
  assert!(!shell.has_dialog(cx));

  let flag = confirmed.clone();
  shell.window(cx, |window, cx| {
    close_guard::should_close(true, window, cx, move |_, _| flag.set(true))
  });
  cx.run_until_parked();
  shell.window(cx, |window, cx| window.render_frame(cx));
  shell.click(cx, "exit-yes");
  assert!(confirmed.get());
  assert!(
    cx.update_window(shell.handle, |_, _, _| ()).is_err(),
    "the window closed"
  );
}

#[gpui_kit::test]
fn a_close_request_without_extractions_saves_the_geometry_and_closes(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  let stores = shell.stores.clone();
  let closes = shell.window(cx, |window, cx| {
    crate::platform::window::close_requested(&stores, window, cx)
  });
  assert!(closes);
  assert!(!shell.has_dialog(cx));
  let window = shell
    .stores
    .config
    .read_with(cx, |config, _| config.config().window.clone());
  assert_eq!((window.size.width, window.size.height), (1000, 760));
}

#[gpui_kit::test]
fn the_geometry_is_saved_once_the_window_is_still(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  cx.simulate_window_resize(shell.handle, size(px(900.0), px(700.0)));
  cx.run_until_parked();
  assert!(shell.written.lock().unwrap().is_empty(), "debounced");
  cx.executor().advance_clock(Duration::from_millis(600));
  cx.run_until_parked();
  let written = shell.written.lock().unwrap().last().cloned().expect("saved");
  assert_eq!((written.window.size.width, written.window.size.height), (900, 700));
  // Too small to keep: the last good geometry stays.
  cx.simulate_window_resize(shell.handle, size(px(500.0), px(400.0)));
  cx.executor().advance_clock(Duration::from_millis(600));
  cx.run_until_parked();
  let config = shell
    .stores
    .config
    .read_with(cx, |config, _| config.config().window.clone());
  assert_eq!((config.size.width, config.size.height), (900, 700));
}

#[gpui_kit::test]
fn enter_and_escape_answer_no_in_the_exit_dialog(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  let confirmed = std::rc::Rc::new(std::cell::Cell::new(false));
  for key in ["enter", "escape"] {
    let flag = confirmed.clone();
    shell.window(cx, |window, cx| {
      close_guard::should_close(true, window, cx, move |_, _| flag.set(true))
    });
    cx.run_until_parked();
    shell.window(cx, |window, cx| window.render_frame(cx));
    shell.press(cx, key);
    assert!(!confirmed.get(), "{key} does not confirm");
    assert!(!shell.has_dialog(cx), "{key} closes the dialog");
  }
}

/// The menu items on screen, by accessible name, with their checked state.
fn menu_items(shell: &Shell, cx: &mut TestAppContext) -> Vec<(String, bool)> {
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window
      .elements()
      .into_iter()
      .filter(|element| matches!(element.role(), Some(Role::MenuItem | Role::MenuItemCheckBox)))
      .filter_map(|element| {
        let checked = element.checked().unwrap_or(false);
        element.label().map(|label| (label.to_string(), checked))
      })
      .collect()
  })
}

fn click_menu_item(shell: &Shell, cx: &mut TestAppContext, label: &str) {
  let center = shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window
      .find_all_by_label(label)
      .into_iter()
      .find(|element| matches!(element.role(), Some(Role::MenuItem | Role::MenuItemCheckBox)))
      .expect("the menu item")
      .bounds()
      .center()
  });
  VisualTestContext::from_window(shell.handle, cx).simulate_click(center, Modifiers::none());
  cx.run_until_parked();
}

fn open_menu(shell: &Shell, cx: &mut TestAppContext) -> Option<ToolbarMenu> {
  shell.stores.ui.read_with(cx, |ui, _| ui.open_menu())
}

#[gpui_kit::test]
fn toolbar_overflow_exposes_hidden_commands_and_menu_shortcuts(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  shell.window(cx, |window, _| {
    window.activate_window();
    assert!(window.try_find("toolbar-overflow").is_none());
  });
  shell
    .stores
    .config
    .update(cx, |store, cx| store.add_profile("Anime", cx));

  shell.resize(cx, 600.0, 450.0);
  shell.click(cx, "toolbar-overflow");
  assert_eq!(open_menu(&shell, cx), Some(ToolbarMenu::Overflow));
  let items = menu_items(&shell, cx);
  assert!(items.iter().any(|(label, _)| label == "About"));
  assert!(!items.iter().any(|(label, _)| label == "Extract All"));
  click_menu_item(&shell, cx, "About");
  assert_eq!(open_menu(&shell, cx), None);
  assert_eq!(shell.active_tab(cx), TabType::About);

  shell.resize(cx, 500.0, 450.0);
  shell.window(cx, |window, _| assert!(window.try_find("toolbar-profile").is_none()));
  shell.click(cx, "toolbar-overflow");
  click_menu_item(&shell, cx, "Profile (F9)");
  click_menu_item(&shell, cx, "Default");
  assert_eq!(open_menu(&shell, cx), None);
  assert_eq!(
    shell
      .stores
      .config
      .read_with(cx, |store, _| store.config().active_profile.clone()),
    "Default"
  );

  shell.press(cx, "f9");
  assert_eq!(open_menu(&shell, cx), Some(ToolbarMenu::Profile));
  assert_eq!(
    menu_items(&shell, cx),
    [("Default".to_string(), true), ("Anime".to_string(), false)]
  );
  shell.press(cx, "escape");
  assert_eq!(open_menu(&shell, cx), None);

  shell.resize(cx, 400.0, 450.0);
  shell.window(cx, |window, _| {
    assert!(window.try_find("toolbar-quit-when-complete").is_none())
  });
  shell.click(cx, "toolbar-overflow");
  assert!(menu_items(&shell, cx).contains(&("Quit when complete".to_string(), false)));
  click_menu_item(&shell, cx, "Quit when complete");
  assert!(shell.stores.ui.read_with(cx, |ui, _| ui.quit_when_complete()));
  shell.click(cx, "toolbar-overflow");
  assert!(menu_items(&shell, cx).contains(&("Quit when complete".to_string(), true)));
  shell.press(cx, "escape");

  shell.click(cx, "toolbar-overflow");
  shell.resize(cx, 1000.0, 450.0);
  assert_eq!(open_menu(&shell, cx), None);
  shell.window(cx, |window, _| {
    assert!(window.try_find("toolbar-overflow").is_none());
    window.find("toolbar-about");
  });
}

#[gpui_kit::test]
fn f9_opens_the_profile_menu_and_a_choice_activates_the_profile(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  shell.window(cx, |window, _| window.activate_window());
  shell.stores.config.update(cx, |store, cx| {
    store.add_profile("Anime", cx);
    store.set_active_profile("Default", cx);
  });
  shell.press(cx, "f9");
  assert_eq!(open_menu(&shell, cx), Some(ToolbarMenu::Profile));
  assert_eq!(
    menu_items(&shell, cx),
    [("Default".to_string(), true), ("Anime".to_string(), false)]
  );
  click_menu_item(&shell, cx, "Anime");
  assert_eq!(open_menu(&shell, cx), None);
  assert_eq!(
    shell
      .stores
      .config
      .read_with(cx, |store, _| store.config().active_profile.clone()),
    "Anime"
  );
  cx.run_until_parked();
  assert_eq!(shell.written.lock().unwrap().last().unwrap().active_profile, "Anime");
  // A click on the button opens it again with the new profile checked.
  shell.click(cx, "toolbar-profile");
  assert_eq!(open_menu(&shell, cx), Some(ToolbarMenu::Profile));
  assert_eq!(menu_items(&shell, cx)[1], ("Anime".to_string(), true));
  shell.press(cx, "escape");
  assert_eq!(open_menu(&shell, cx), None);
}

fn update_shell(cx: &mut TestAppContext, config: Config, result: anyhow::Result<UpdateCheckResult>) -> Shell {
  let shell = Shell::with_config(cx, config);
  let result = Arc::new(Mutex::new(Some(result)));
  let source = crate::updates::UpdateSource::new(move |version| {
    assert_eq!(version, crate::APP_VERSION);
    result.lock().unwrap().take().expect("one check")
  });
  cx.update(|cx| crate::updates::start(&shell.stores, source, cx));
  cx.run_until_parked();
  shell.window(cx, |window, cx| window.render_frame(cx));
  shell
}

fn found(version: &str) -> anyhow::Result<UpdateCheckResult> {
  Ok(UpdateCheckResult {
    has_update: true,
    latest_version: Some(version.to_string()),
  })
}

fn banner(shell: &Shell, cx: &mut TestAppContext) -> Option<String> {
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.try_find("update-banner")?;
    gpui_kit::base::test_support::texts(window)
      .into_iter()
      .map(|text| text.text().to_string())
      .find(|text| text.contains("is available"))
  })
}

#[gpui_kit::test]
fn a_new_release_shows_the_banner_and_records_the_check(cx: &mut TestAppContext) {
  let shell = update_shell(cx, Config::default(), found("9.9.0"));
  assert_eq!(
    banner(&shell, cx),
    Some("A new version 9.9.0 is available.".to_string())
  );
  let config = shell.stores.config.read_with(cx, |store, _| store.config().clone());
  assert!(config.update.last_checked > 0);
  assert_eq!(config.update.last_version, "9.9.0");
  assert_eq!(
    shell.written.lock().unwrap().last().unwrap().update.last_version,
    "9.9.0"
  );
  // Closing without the box only hides it.
  shell.click(cx, "update-close");
  assert_eq!(banner(&shell, cx), None);
  assert!(
    shell
      .stores
      .config
      .read_with(cx, |store, _| store.config().update.ignore_version.is_empty())
  );
}

#[gpui_kit::test]
fn skipping_a_version_hides_it_from_the_next_start(cx: &mut TestAppContext) {
  let shell = update_shell(cx, Config::default(), found("9.9.0"));
  shell.window(cx, |window, _| window.activate_window());
  shell.click(cx, "update-skip");
  shell.click(cx, "update-close");
  assert_eq!(banner(&shell, cx), None);
  let config = shell.stores.config.read_with(cx, |store, _| store.config().clone());
  assert_eq!(config.update.ignore_version, "9.9.0");
  drop(shell);
  // The next start reuses the check, and the version stays hidden.
  let shell = update_shell(cx, config.clone(), Err(anyhow::anyhow!("not asked")));
  assert_eq!(banner(&shell, cx), None);
  // A cached newer version that was not skipped shows without asking.
  let mut config = config;
  config.update.ignore_version.clear();
  drop(shell);
  let shell = update_shell(cx, config, Err(anyhow::anyhow!("not asked")));
  assert_eq!(
    banner(&shell, cx),
    Some("A new version 9.9.0 is available.".to_string())
  );
}

#[gpui_kit::test]
fn a_failed_check_shows_nothing_and_records_nothing(cx: &mut TestAppContext) {
  let shell = update_shell(cx, Config::default(), Err(anyhow::anyhow!("offline")));
  assert_eq!(banner(&shell, cx), None);
  let config = shell.stores.config.read_with(cx, |store, _| store.config().clone());
  assert_eq!(config.update.last_checked, 0, "the next start tries again");
  assert!(shell.written.lock().unwrap().is_empty());
}
