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

//! Headless tests of the application shell and the file list's plain boxes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime};

use bme_core::config::{Config, ConfigWindow, DisplayMode, Language, Theme as ThemeName};
use bme_core::extract::{ExtractError, ExtractEvent, ExtractEventSink, ExtractOutcome};
use bme_core::protocol::{MkvTrack, UpdateCheckResult};
use core::prelude::v1::test;
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::*;

use super::actions;
use super::root::AppView;
use crate::domain::status::QueueItemStatus;
use crate::extract_events::{self, ExtractBackend};
use crate::i18n::tests::LOCALE_LOCK;
use crate::platform::close_guard;
use crate::platform::taskbar::Taskbar;
use crate::state::files::tests::{TempDir, all, fixture_source, movie};
use crate::state::ui::{NotificationKind, ToolbarMenu};
use crate::state::{Persistence, Services, Stores, TabType};
use crate::tools::ToolDetector;

/// What the shell's extract queue was asked to do.
#[derive(Default)]
pub(super) struct ExtractCalls {
  pub(super) enqueued: Vec<(String, Vec<String>)>,
  pub(super) cancelled: Vec<String>,
}

/// The shell in a test window, in English. The locale is process-wide, and a
/// test switching it would change the text width under another test's click,
/// so every shell test holds the locale lock while it runs.
///
/// Tracks are listed from fixtures; extractions are recorded, and their
/// events are sent by the test through [`Self::send`] into the same channel
/// the app uses.
pub(super) struct Shell {
  pub(super) handle: AnyWindowHandle,
  pub(super) stores: Stores,
  /// Every config the store wrote, in order.
  pub(super) written: Arc<Mutex<Vec<Config>>>,
  pub(super) extracts: Arc<Mutex<ExtractCalls>>,
  /// Every value the window's taskbar progress was set to.
  pub(super) taskbar: Rc<RefCell<Vec<Option<u8>>>>,
  events: ExtractEventSink,
  _locale: MutexGuard<'static, ()>,
}

impl Shell {
  fn open(cx: &mut TestAppContext) -> Self {
    Self::with_config(cx, Config::default())
  }

  /// The shell with `config` (its language is set to English).
  pub(super) fn with_config(cx: &mut TestAppContext, config: Config) -> Self {
    Self::build(cx, config, HashMap::new())
  }

  /// The shell over track fixtures, with Group Mode off.
  pub(super) fn with_files(cx: &mut TestAppContext, fixtures: HashMap<String, Vec<MkvTrack>>) -> Self {
    let mut config = Config::default();
    config.profiles[0].default_group_mode = false;
    Self::build(cx, config, fixtures)
  }

  fn build(cx: &mut TestAppContext, config: Config, fixtures: HashMap<String, Vec<MkvTrack>>) -> Self {
    let locale = LOCALE_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    crate::i18n::set_language(Language::EnUS);
    let written = Arc::new(Mutex::new(Vec::new()));
    let sink = written.clone();
    let extracts = Arc::new(Mutex::new(ExtractCalls::default()));
    let (enqueue_calls, cancel_calls) = (extracts.clone(), extracts.clone());
    let backend = ExtractBackend::new(
      move |file, args| {
        enqueue_calls.lock().unwrap().enqueued.push((file, args));
        Ok(())
      },
      move |file| {
        cancel_calls.lock().unwrap().cancelled.push(file.to_string());
        Ok(())
      },
    );
    let (events, receiver) = extract_events::channel();
    let (tracks, _) = fixture_source(fixtures);
    let taskbar: Rc<RefCell<Vec<Option<u8>>>> = Rc::default();
    let progress = taskbar.clone();
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
        tracks,
        extracts: backend,
        taskbar: Taskbar::new(move |_| {
          let progress = progress.clone();
          Box::new(move |percent| progress.borrow_mut().push(percent))
        }),
        tools: ToolDetector::system(),
      };
      let stores = Stores::new(config, services, cx);
      extract_events::start(&stores, receiver, cx);
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
      extracts,
      taskbar,
      events,
      _locale: locale,
    };
    shell.window(cx, |window, cx| window.render_frame(cx));
    shell
  }

  /// Sends an extract event as a worker thread of the core would.
  pub(super) fn send(&self, cx: &mut TestAppContext, event: ExtractEvent) {
    (self.events)(event);
    cx.run_until_parked();
  }

  /// The title the window shows.
  pub(super) fn title(&self, cx: &mut TestAppContext) -> String {
    VisualTestContext::from_window(self.handle, cx)
      .window_title()
      .unwrap_or_else(|| crate::window_title().to_string())
  }

  pub(super) fn toasts(&self, cx: &mut TestAppContext) -> usize {
    self.window(cx, |window, cx| window.notifications(cx).len())
  }

  /// Adds files and lets their tracks load and the profile select them. The
  /// fixtures' folders do not exist, so the outputs go to the system's
  /// temporary directory, which does.
  pub(super) fn add_files(&self, cx: &mut TestAppContext, files: &[&str]) {
    let files: Vec<String> = files.iter().map(|file| (*file).to_owned()).collect();
    self.stores.files.update(cx, |store, cx| {
      store.add_files(files, cx);
      store.set_global_output_dir(std::env::temp_dir().to_string_lossy().into_owned(), cx);
    });
    cx.run_until_parked();
    self.window(cx, |window, cx| window.render_frame(cx));
  }

  pub(super) fn enqueued(&self) -> Vec<String> {
    let calls = self.extracts.lock().unwrap();
    calls.enqueued.iter().map(|(file, _)| file.clone()).collect()
  }

  pub(super) fn cancelled(&self) -> Vec<String> {
    self.extracts.lock().unwrap().cancelled.clone()
  }

  pub(super) fn status(&self, cx: &mut TestAppContext, file: &str) -> Option<QueueItemStatus> {
    self
      .stores
      .queue
      .read_with(cx, |queue, _| queue.item(file).map(|item| item.status))
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
  assert!(!shell.disabled(cx, "welcome-add-files"));
  assert!(!shell.disabled(cx, "welcome-add-folder"));
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
fn the_toolbar_enables_what_the_stores_allow(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  for id in [
    "toolbar-global-output",
    "toolbar-extract-all",
    "toolbar-resume-all",
    "toolbar-cancel-all",
    "toolbar-clear-completed",
    "toolbar-clear",
    "toolbar-settings",
  ] {
    assert!(shell.disabled(cx, id), "{id} has nothing to act on");
  }
  for id in [
    "toolbar-quit-when-complete",
    "toolbar-system-notification",
    "toolbar-topmost-notification",
    "toolbar-group-mode",
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
  // Without files the shortcuts have nothing to do, and F10 opens no dialog
  // before Settings exists.
  for key in ["f3", "f4", "ctrl-q", "f10"] {
    shell.press(cx, key);
  }
  assert!(!shell.has_dialog(cx));
  assert!(shell.enqueued().is_empty() && shell.cancelled().is_empty());
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

pub(super) fn movies(files: &[&str]) -> HashMap<String, Vec<MkvTrack>> {
  all(files, movie)
}

pub(super) fn started(file: &str) -> ExtractEvent {
  ExtractEvent::Started {
    file: file.to_string(),
    at: SystemTime::now(),
  }
}

pub(super) fn progress(file: &str, percent: u32) -> ExtractEvent {
  ExtractEvent::Progress {
    file: file.to_string(),
    percent,
  }
}

pub(super) fn finished(file: &str, outcome: ExtractOutcome, error: Option<&str>) -> ExtractEvent {
  ExtractEvent::Finished {
    file: file.to_string(),
    outcome,
    error: error.map(|message| ExtractError::Other(message.to_string())),
    warnings: Vec::new(),
    at: SystemTime::now(),
  }
}

#[gpui_kit::test]
fn dropping_files_and_folders_lists_their_mkv_files(cx: &mut TestAppContext) {
  let dir = TempDir::new("shell-drop");
  let movie = dir.file("movie.mkv");
  dir.file("readme.txt");
  let shell = Shell::open(cx);
  let paths = ExternalPaths(vec![dir.path().to_path_buf()].into());
  let position = point(px(500.0), px(400.0));
  shell.window(cx, |window, cx| {
    window.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Entered { position, paths }), cx);
    window.dispatch_event(PlatformInput::FileDrop(FileDropEvent::Submit { position }), cx);
  });
  cx.run_until_parked();
  shell
    .stores
    .files
    .read_with(cx, |files, _| assert_eq!(files.files(), [movie]));
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.find(("file-entry", 0usize));
    assert!(window.try_find(("file-entry", 1usize)).is_none());
    assert!(window.try_find("welcome").is_none());
  });
}

/// Add Files and Add Folder open the file dialog, and what is chosen goes
/// through the same `.mkv` filter as a drop.
#[gpui_kit::test]
fn add_files_and_add_folder_take_what_the_dialog_returns(cx: &mut TestAppContext) {
  let dir = TempDir::new("shell-dialog");
  let (a, b) = (dir.file("a.mkv"), dir.file("b.mkv"));
  let ignored = dir.file("c.mp4");
  let shell = Shell::open(cx);
  shell.click(cx, "welcome-add-files");
  assert!(cx.did_prompt_for_paths());
  let chosen = vec![b.clone().into(), ignored.into()];
  cx.simulate_path_prompt_response(move |options| {
    assert!(options.files && options.multiple && !options.directories);
    Some(chosen)
  });
  cx.run_until_parked();
  assert_eq!(
    shell.stores.files.read_with(cx, |files, _| files.files().to_vec()),
    std::slice::from_ref(&b)
  );
  shell.stores.files.update(cx, |files, cx| files.clear_files(cx));
  shell.window(cx, |window, cx| window.render_frame(cx));
  shell.click(cx, "welcome-add-folder");
  let folder = dir.path().to_path_buf();
  cx.simulate_path_prompt_response(move |options| {
    assert!(options.directories && !options.files);
    Some(vec![folder])
  });
  cx.run_until_parked();
  assert_eq!(
    shell.stores.files.read_with(cx, |files, _| files.files().to_vec()),
    [a, b]
  );
  // A cancelled dialog adds nothing.
  shell.stores.files.update(cx, |files, cx| files.clear_files(cx));
  shell.window(cx, |window, cx| window.render_frame(cx));
  shell.click(cx, "welcome-add-files");
  cx.simulate_path_prompt_response(|_| None);
  cx.run_until_parked();
  assert!(!shell.stores.files.read_with(cx, |files, _| files.has_files()));
}

#[gpui_kit::test]
fn clear_empties_the_list_from_the_toolbar_and_ctrl_q(cx: &mut TestAppContext) {
  let shell = Shell::open(cx);
  shell.add_files(cx, &["/media/a.mkv"]);
  assert!(!shell.disabled(cx, "toolbar-clear"));
  // Nothing loaded, so nothing selected, and nothing extracts.
  assert!(shell.disabled(cx, "toolbar-extract-all"));
  assert!(shell.disabled(cx, "toolbar-cancel-all"));
  shell.click(cx, "toolbar-clear");
  assert!(!shell.stores.files.read_with(cx, |files, _| files.has_files()));
  shell.add_files(cx, &["/media/b.mkv"]);
  shell.press(cx, "ctrl-q");
  assert!(!shell.stores.files.read_with(cx, |files, _| files.has_files()));
  assert!(shell.shows(cx, "Add Files"), "the Welcome page is back");
}

/// Group Mode starts as the profile's `defaultGroupMode`; the toolbar button
/// and F8 switch it for the session and the file list regroups.
#[gpui_kit::test]
fn group_mode_toggles_from_the_toolbar_and_f8(cx: &mut TestAppContext) {
  let (a, b) = ("/media/a.mkv", "/media/b.mkv");
  let shell = Shell::with_files(cx, movies(&[a, b]));
  let group_mode =
    |shell: &Shell, cx: &mut TestAppContext| shell.stores.files.read_with(cx, |files, _| files.group_mode());
  assert!(!group_mode(&shell, cx));
  shell.add_files(cx, &[a, b]);
  let entries =
    |shell: &Shell, cx: &mut TestAppContext| shell.stores.files.read_with(cx, |files, _| files.entries().len());
  assert_eq!(entries(&shell, cx), 2);
  shell.click(cx, "toolbar-group-mode");
  assert!(group_mode(&shell, cx));
  assert_eq!(entries(&shell, cx), 1, "one group card");
  let checked = shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.find("toolbar-group-mode").checked()
  });
  assert_eq!(checked, Some(true));
  shell.window(cx, |window, _| {
    window.find(("file-entry", 0usize));
    assert!(window.try_find(("file-entry", 1usize)).is_none());
  });
  shell.press(cx, "f8");
  assert!(!group_mode(&shell, cx));
  assert_eq!(entries(&shell, cx), 2);
  // A profile operation sets it again (Default's value is off here).
  shell.press(cx, "f8");
  shell
    .stores
    .config
    .update(cx, |config, cx| config.set_active_profile("Default", cx));
  cx.run_until_parked();
  assert!(!group_mode(&shell, cx));
  assert!(
    shell
      .written
      .lock()
      .unwrap()
      .iter()
      .all(|config| !config.profiles[0].default_group_mode)
  );
}

#[gpui_kit::test]
fn extract_all_enqueues_every_file_and_the_title_counts_the_queue(cx: &mut TestAppContext) {
  let (a, b) = ("/media/a.mkv", "/media/b.mkv");
  let shell = Shell::with_files(cx, movies(&[a, b]));
  let base = crate::window_title().to_string();
  assert!(shell.disabled(cx, "toolbar-extract-all"));
  shell.add_files(cx, &[a, b]);
  assert!(
    !shell.disabled(cx, "toolbar-extract-all"),
    "the profile selected tracks"
  );
  shell.click(cx, "toolbar-extract-all");
  assert_eq!(shell.enqueued(), [a, b], "in file list order");
  let args = shell.extracts.lock().unwrap().enqueued[0].1.clone();
  assert_eq!(args[0], "tracks");
  assert!(
    args[1].starts_with("3:") && args[1].ends_with("a.3.eng.srt"),
    "{args:?}"
  );
  assert_eq!(shell.status(cx, a), Some(QueueItemStatus::Waiting));
  assert_eq!(shell.title(cx), format!("{base} - 0/2"));
  assert!(!shell.disabled(cx, "toolbar-cancel-all"));
  assert!(
    shell.disabled(cx, "toolbar-clear"),
    "no clearing while extractions wait"
  );
  assert_eq!(shell.taskbar.borrow().last(), Some(&Some(0)));

  shell.send(cx, started(a));
  shell.send(cx, progress(a, 50));
  assert_eq!(
    shell.taskbar.borrow().last(),
    Some(&Some(25)),
    "the whole queue's progress"
  );
  shell.send(cx, finished(a, ExtractOutcome::Completed, None));
  assert_eq!(shell.title(cx), format!("{base} - 1/2"));
  assert_eq!(shell.toasts(cx), 1, "a success toast");

  shell.send(cx, started(b));
  shell.send(
    cx,
    finished(b, ExtractOutcome::Failed, Some("mkvextract exited with code 2")),
  );
  assert_eq!(shell.status(cx, b), Some(QueueItemStatus::Failed));
  assert_eq!(shell.title(cx), base, "the plain title once nothing is active");
  assert_eq!(shell.toasts(cx), 2, "an error toast");
  assert_eq!(
    shell.taskbar.borrow().last(),
    Some(&None),
    "hidden once nothing is active"
  );
  assert!(!shell.disabled(cx, "toolbar-clear"));
  assert!(shell.disabled(cx, "toolbar-cancel-all"));
}

#[gpui_kit::test]
fn f3_extracts_once_and_f4_cancels(cx: &mut TestAppContext) {
  let a = "/media/a.mkv";
  let shell = Shell::with_files(cx, movies(&[a]));
  shell.press(cx, "f3");
  assert!(shell.enqueued().is_empty(), "nothing listed yet");
  shell.add_files(cx, &[a]);
  shell.press(cx, "f3");
  shell.press(cx, "f3");
  assert_eq!(shell.enqueued(), [a], "a waiting file is not enqueued again");
  shell.press(cx, "ctrl-q");
  assert!(
    shell.stores.files.read_with(cx, |files, _| files.has_files()),
    "Ctrl+Q waits too"
  );
  shell.press(cx, "f4");
  assert_eq!(shell.cancelled(), [a]);
  let requested = shell
    .stores
    .queue
    .read_with(cx, |queue, _| queue.item(a).map(|item| item.cancel_requested));
  assert_eq!(requested, Some(true));
  shell.send(cx, finished(a, ExtractOutcome::Cancelled, None));
  assert_eq!(shell.status(cx, a), Some(QueueItemStatus::Cancelled));
  assert_eq!(shell.toasts(cx), 0, "a cancelled extraction shows no toast");
  shell.press(cx, "f4");
  assert_eq!(shell.cancelled(), [a], "nothing active to cancel");
  shell.press(cx, "ctrl-q");
  assert!(!shell.stores.files.read_with(cx, |files, _| files.has_files()));
  assert_eq!(shell.status(cx, a), None, "clearing empties the queue too");
}

/// Resume All and Clear Completed act on the queue's finished rows.
#[gpui_kit::test]
fn resume_all_and_clear_completed_from_the_toolbar(cx: &mut TestAppContext) {
  let (a, b) = ("/media/a.mkv", "/media/b.mkv");
  let shell = Shell::with_files(cx, movies(&[a, b]));
  shell.add_files(cx, &[a, b]);
  shell.click(cx, "toolbar-extract-all");
  shell.send(cx, finished(a, ExtractOutcome::Completed, None));
  shell.send(cx, finished(b, ExtractOutcome::Failed, Some("broken")));
  assert!(!shell.disabled(cx, "toolbar-resume-all"));
  assert!(!shell.disabled(cx, "toolbar-clear-completed"));
  shell.click(cx, "toolbar-resume-all");
  assert_eq!(shell.enqueued(), [a, b, b], "only the failed file again");
  assert!(shell.disabled(cx, "toolbar-resume-all"));
  shell.click(cx, "toolbar-clear-completed");
  assert_eq!(shell.status(cx, a), None);
  assert_eq!(shell.status(cx, b), Some(QueueItemStatus::Waiting));
  assert!(shell.disabled(cx, "toolbar-clear-completed"));
}

/// The plain boxes follow loading and extracting: a box per file, a failed
/// listing keeps its box, and a progress event repaints without a new list.
#[gpui_kit::test]
fn the_boxes_follow_loading_and_extracting(cx: &mut TestAppContext) {
  let (a, bad) = ("/media/a.mkv", "/media/bad.mkv");
  let shell = Shell::with_files(cx, movies(&[a]));
  shell.stores.files.update(cx, |files, cx| {
    files.add_files(vec![a.into(), bad.into()], cx);
    files.set_global_output_dir(std::env::temp_dir().to_string_lossy().into_owned(), cx);
  });
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.find(("file-entry", 0usize));
    window.find(("file-entry", 1usize));
  });
  cx.run_until_parked();
  shell.press(cx, "f3");
  shell.send(cx, started(a));
  shell.send(cx, progress(a, 42));
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.find(("file-entry", 1usize));
  });
  shell.stores.files.read_with(cx, |files, _| {
    assert!(matches!(files.load_state(a), crate::state::files::LoadState::Loaded));
    assert!(matches!(
      files.load_state(bad),
      crate::state::files::LoadState::Failed(_)
    ));
  });
  assert_eq!(shell.enqueued(), [a], "a file whose tracks failed has no selection");
}

#[gpui_kit::test]
fn closing_while_an_extraction_runs_asks_and_yes_cancels_it(cx: &mut TestAppContext) {
  let a = "/media/a.mkv";
  let shell = Shell::with_files(cx, movies(&[a]));
  shell.add_files(cx, &[a]);
  shell.press(cx, "f3");
  shell.send(cx, started(a));
  let stores = shell.stores.clone();
  let closes = shell.window(cx, |window, cx| {
    crate::platform::window::close_requested(&stores, window, cx)
  });
  assert!(!closes);
  cx.run_until_parked();
  shell.window(cx, |window, cx| window.render_frame(cx));
  shell.click(cx, "exit-yes");
  assert_eq!(shell.cancelled(), [a]);
  assert!(
    cx.update_window(shell.handle, |_, _, _| ()).is_err(),
    "the window closed"
  );
}

/// Quit by keyboard (macOS Cmd+Q) while an extraction runs asks first, like
/// the title bar: the key is dispatched while its window is being updated.
#[gpui_kit::test]
fn quitting_by_keyboard_while_an_extraction_runs_asks_first(cx: &mut TestAppContext) {
  let a = "/media/a.mkv";
  let shell = Shell::with_files(cx, movies(&[a]));
  cx.update(|cx| {
    cx.bind_keys([KeyBinding::new("cmd-q", actions::Quit, None)]);
    crate::platform::install_quit(shell.handle, &shell.stores, cx);
  });
  shell.add_files(cx, &[a]);
  shell.press(cx, "f3");
  shell.send(cx, started(a));
  shell.press(cx, "cmd-q");
  assert!(
    shell.window(cx, |window, cx| window.has_active_dialog(cx)),
    "the exit confirmation is open"
  );
  shell.window(cx, |window, cx| window.render_frame(cx));
  shell.click(cx, "exit-no");
  assert!(shell.cancelled().is_empty());
  assert!(
    cx.update_window(shell.handle, |_, _, _| ()).is_ok(),
    "No keeps the window"
  );
}
