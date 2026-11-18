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

//! Navigation state: the tabs, the in-app toast, the completion toggles, the
//! open toolbar menu and the version the update banner announces.

use gpui_kit::{Context, EventEmitter};

/// The main tabs, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabType {
  FileList,
  Queue,
  About,
}

impl TabType {
  /// The i18n key of the tab label.
  pub fn label_key(self) -> &'static str {
    match self {
      Self::FileList => "tabs.fileList",
      Self::Queue => "tabs.queue",
      Self::About => "tabs.about",
    }
  }

  /// About is opened from the toolbar and can be closed.
  pub fn is_closable(self) -> bool {
    self == Self::About
  }

  /// A stable name for element ids.
  pub fn name(self) -> &'static str {
    match self {
      Self::FileList => "file-list",
      Self::Queue => "queue",
      Self::About => "about",
    }
  }
}

/// The toolbar's menus. F9 also opens Profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolbarMenu {
  Profile,
  Overflow,
}

impl ToolbarMenu {
  /// A stable name for element ids.
  pub fn name(self) -> &'static str {
    match self {
      Self::Profile => "profile",
      Self::Overflow => "overflow",
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationKind {
  Success,
  Error,
}

/// A top-centre toast: the file as the title, the detail below.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
  pub kind: NotificationKind,
  pub file: String,
  pub detail: String,
}

/// A card's short message (`Command copied`, a launch error): a
/// bottom-centre toast without a title, as the cards' snackbars were.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snackbar {
  pub kind: NotificationKind,
  pub message: String,
}

/// What a drained queue reports through the OS notification and the topmost
/// window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompletionNotice {
  pub title: String,
  /// The file of a one-file batch, else the counts.
  pub line: String,
  pub detail: String,
  /// An extraction of the batch failed: the topmost window shows an alert.
  pub failed: bool,
}

/// A completion to deliver, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
  pub notice: CompletionNotice,
  /// Post it to the OS notification centre.
  pub system: bool,
  /// Show it in the topmost window.
  pub topmost: bool,
  /// Quit after delivering the notifications or acknowledging the topmost window.
  pub quit: bool,
}

/// Events for the window that shows the state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UiEvent {
  /// Open Settings in the main window.
  OpenSettings,
  /// Close the current tab when the window is not showing a dialog.
  CloseActiveTab,
  /// Show a toast in the main window.
  Toast(Toast),
  /// Show a card's message at the bottom of the main window.
  Snackbar(Snackbar),
  /// The queue drained: deliver the enabled completion actions.
  Completion(Completion),
  /// The user accepted the topmost window's Quit action.
  QuitAfterCompletion,
}

pub struct UiState {
  active_tab: TabType,
  show_about: bool,
  /// The queue's completion toggles, off at every start.
  system_notification: bool,
  topmost_notification: bool,
  quit_when_complete: bool,
  /// The main window is closing: no more completions.
  closing: bool,
  open_menu: Option<ToolbarMenu>,
  /// A newer release the banner announces.
  update_available: Option<String>,
}

impl EventEmitter<UiEvent> for UiState {}

impl Default for UiState {
  fn default() -> Self {
    Self::new()
  }
}

impl UiState {
  pub fn new() -> Self {
    Self {
      active_tab: TabType::FileList,
      show_about: false,
      system_notification: false,
      topmost_notification: false,
      quit_when_complete: false,
      closing: false,
      open_menu: None,
      update_available: None,
    }
  }

  pub fn active_tab(&self) -> TabType {
    self.active_tab
  }

  /// The open tabs: File List and Queue always, then About when open.
  pub fn tabs(&self) -> Vec<TabType> {
    let mut tabs = vec![TabType::FileList, TabType::Queue];
    if self.show_about {
      tabs.push(TabType::About);
    }
    tabs
  }

  /// Index of the active tab among [`Self::tabs`].
  pub fn active_index(&self) -> usize {
    self.tabs().iter().position(|tab| *tab == self.active_tab).unwrap_or(0)
  }

  pub fn set_active_tab(&mut self, tab: TabType, cx: &mut Context<Self>) {
    if self.tabs().contains(&tab) && self.active_tab != tab {
      self.active_tab = tab;
      cx.notify();
    }
  }

  /// Requests the Settings dialog in the main window.
  pub fn open_settings(&mut self, cx: &mut Context<Self>) {
    cx.emit(UiEvent::OpenSettings);
  }

  /// Opens (or reselects) the About tab.
  pub fn open_about(&mut self, cx: &mut Context<Self>) {
    self.show_about = true;
    self.active_tab = TabType::About;
    cx.notify();
  }

  /// Closes a closable tab; when it was active, its left neighbour becomes
  /// active.
  pub fn close_tab(&mut self, tab: TabType, cx: &mut Context<Self>) {
    let tabs = self.tabs();
    let Some(index) = tabs.iter().position(|open| *open == tab) else {
      return;
    };
    if !tab.is_closable() {
      return;
    }
    if self.active_tab == tab {
      self.active_tab = tabs[index.saturating_sub(1)];
    }
    self.show_about = false;
    cx.notify();
  }

  /// Closes the active tab if it is closable (Ctrl+W).
  pub fn close_active_tab(&mut self, cx: &mut Context<Self>) {
    self.close_tab(self.active_tab, cx);
  }

  /// Moves the selection by `delta` tabs, wrapping around (Ctrl+Tab,
  /// Ctrl+Shift+Tab).
  pub fn select_relative(&mut self, delta: isize, cx: &mut Context<Self>) {
    let tabs = self.tabs();
    if tabs.len() <= 1 {
      return;
    }
    let count = tabs.len() as isize;
    let next = (self.active_index() as isize + delta).rem_euclid(count) as usize;
    self.set_active_tab(tabs[next], cx);
  }

  /// Selects the tab at `index` if it exists (Ctrl+1..9).
  pub fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
    if let Some(tab) = self.tabs().get(index).copied() {
      self.set_active_tab(tab, cx);
    }
  }

  /// The toolbar menu that is open.
  pub fn open_menu(&self) -> Option<ToolbarMenu> {
    self.open_menu
  }

  /// Opens a toolbar menu (a click on its button, F9); another open menu
  /// closes.
  pub fn show_menu(&mut self, menu: ToolbarMenu, cx: &mut Context<Self>) {
    if self.open_menu != Some(menu) {
      self.open_menu = Some(menu);
      cx.notify();
    }
  }

  /// Closes `menu` if it is the open one.
  pub fn hide_menu(&mut self, menu: ToolbarMenu, cx: &mut Context<Self>) {
    if self.open_menu == Some(menu) {
      self.open_menu = None;
      cx.notify();
    }
  }

  /// The version the update banner announces.
  pub fn update_available(&self) -> Option<&str> {
    self.update_available.as_deref()
  }

  /// Shows the update banner for `version`, or hides it.
  pub fn set_update_available(&mut self, version: Option<String>, cx: &mut Context<Self>) {
    if self.update_available != version {
      self.update_available = version;
      cx.notify();
    }
  }

  /// Shows a toast in the main window.
  pub fn show_toast(&mut self, kind: NotificationKind, file: String, detail: String, cx: &mut Context<Self>) {
    cx.emit(UiEvent::Toast(Toast { kind, file, detail }));
  }

  /// Shows a card's message at the bottom of the main window.
  pub fn show_snackbar(&mut self, kind: NotificationKind, message: String, cx: &mut Context<Self>) {
    cx.emit(UiEvent::Snackbar(Snackbar { kind, message }));
  }

  /// Whether a drained queue posts an OS notification.
  pub fn system_notification(&self) -> bool {
    self.system_notification
  }

  /// Whether a drained queue shows the topmost window.
  pub fn topmost_notification(&self) -> bool {
    self.topmost_notification
  }

  pub fn quit_when_complete(&self) -> bool {
    self.quit_when_complete
  }

  /// The queue's System Notification toggle. Nothing asks for permission here;
  /// the OS asks on the first notification.
  pub fn toggle_system_notification(&mut self, cx: &mut Context<Self>) {
    self.system_notification = !self.system_notification;
    cx.notify();
  }

  /// The queue's Topmost Notification toggle.
  pub fn toggle_topmost_notification(&mut self, cx: &mut Context<Self>) {
    self.topmost_notification = !self.topmost_notification;
    cx.notify();
  }

  pub fn toggle_quit_when_complete(&mut self, cx: &mut Context<Self>) {
    self.quit_when_complete = !self.quit_when_complete;
    cx.notify();
  }

  /// The main window is closing (its exit dialog said Yes): the extractions it
  /// cancels drain the queue, which must not notify.
  pub fn set_closing(&mut self) {
    self.closing = true;
  }

  /// The window stays open after all: an extraction did not stop, and the
  /// user chose to wait.
  pub fn cancel_closing(&mut self) {
    self.closing = false;
  }

  /// Whether the main window is closing.
  pub fn is_closing(&self) -> bool {
    self.closing
  }

  /// Delivers the enabled completion actions for a drained queue.
  pub fn show_completion(&mut self, notice: CompletionNotice, cx: &mut Context<Self>) {
    if self.closing || !(self.system_notification || self.topmost_notification || self.quit_when_complete) {
      return;
    }
    cx.emit(UiEvent::Completion(Completion {
      notice,
      system: self.system_notification,
      topmost: self.topmost_notification,
      quit: self.quit_when_complete,
    }));
  }

  pub fn request_completion_quit(&self, cx: &mut Context<Self>) {
    if self.quit_when_complete && !self.closing {
      cx.emit(UiEvent::QuitAfterCompletion);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use core::prelude::v1::test;
  use gpui_kit::{AppContext, Entity, TestAppContext};

  fn state(cx: &mut TestAppContext) -> Entity<UiState> {
    cx.new(|_| UiState::new())
  }

  #[gpui_kit::test]
  fn about_opens_after_the_fixed_tabs(cx: &mut TestAppContext) {
    let ui = state(cx);
    ui.update(cx, |ui, cx| {
      assert_eq!(ui.tabs(), [TabType::FileList, TabType::Queue]);
      ui.open_about(cx);
      assert_eq!(ui.tabs(), [TabType::FileList, TabType::Queue, TabType::About]);
      assert_eq!(ui.active_tab(), TabType::About);
      assert_eq!(ui.active_index(), 2);
    });
  }

  #[gpui_kit::test]
  fn closing_the_active_tab_selects_its_left_neighbour(cx: &mut TestAppContext) {
    let ui = state(cx);
    ui.update(cx, |ui, cx| {
      ui.open_about(cx);
      ui.close_tab(TabType::About, cx);
      assert_eq!(ui.active_tab(), TabType::Queue);
    });
  }

  #[gpui_kit::test]
  fn closing_an_inactive_tab_keeps_the_selection(cx: &mut TestAppContext) {
    let ui = state(cx);
    ui.update(cx, |ui, cx| {
      ui.open_about(cx);
      ui.set_active_tab(TabType::FileList, cx);
      ui.close_tab(TabType::About, cx);
      assert_eq!(ui.active_tab(), TabType::FileList);
      assert_eq!(ui.tabs(), [TabType::FileList, TabType::Queue]);
    });
  }

  #[gpui_kit::test]
  fn fixed_tabs_do_not_close(cx: &mut TestAppContext) {
    let ui = state(cx);
    ui.update(cx, |ui, cx| {
      ui.set_active_tab(TabType::Queue, cx);
      ui.close_active_tab(cx);
      ui.close_tab(TabType::FileList, cx);
      assert_eq!(ui.tabs().len(), 2);
      assert_eq!(ui.active_tab(), TabType::Queue);
      // A tab that is not open cannot be selected or closed.
      ui.set_active_tab(TabType::About, cx);
      ui.close_tab(TabType::About, cx);
      assert_eq!(ui.active_tab(), TabType::Queue);
    });
  }

  #[gpui_kit::test]
  fn relative_and_indexed_selection_wrap_and_ignore_missing_tabs(cx: &mut TestAppContext) {
    let ui = state(cx);
    ui.update(cx, |ui, cx| {
      ui.select_relative(-1, cx);
      assert_eq!(ui.active_tab(), TabType::Queue);
      ui.select_relative(1, cx);
      assert_eq!(ui.active_tab(), TabType::FileList);
      ui.open_about(cx);
      ui.select_relative(1, cx);
      assert_eq!(ui.active_tab(), TabType::FileList);
      ui.select_index(1, cx);
      assert_eq!(ui.active_tab(), TabType::Queue);
      ui.select_index(5, cx);
      assert_eq!(ui.active_tab(), TabType::Queue);
    });
  }

  #[gpui_kit::test]
  fn completions_go_only_through_enabled_channels(cx: &mut TestAppContext) {
    let ui = state(cx);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = seen.clone();
    let _subscription =
      cx.update(|cx| cx.subscribe(&ui, move |_, event: &UiEvent, _| sink.borrow_mut().push(event.clone())));
    let notice = CompletionNotice {
      title: "Extraction complete.".into(),
      line: "a.mkv".into(),
      detail: "Completed in 00:00:01".into(),
      failed: false,
    };
    ui.update(cx, |ui, cx| {
      assert!(
        !ui.system_notification() && !ui.topmost_notification() && !ui.quit_when_complete(),
        "off at start"
      );
      ui.show_completion(notice.clone(), cx);
      ui.toggle_topmost_notification(cx);
      ui.show_completion(notice.clone(), cx);
      ui.toggle_system_notification(cx);
      ui.toggle_topmost_notification(cx);
      ui.show_completion(notice.clone(), cx);
      ui.toggle_system_notification(cx);
      ui.toggle_quit_when_complete(cx);
      ui.show_completion(notice.clone(), cx);
      ui.set_closing();
      ui.show_completion(notice.clone(), cx);
    });
    let completion = |system, topmost, quit| {
      UiEvent::Completion(Completion {
        notice: notice.clone(),
        system,
        topmost,
        quit,
      })
    };
    assert_eq!(
      seen.borrow().as_slice(),
      [
        completion(false, true, false),
        completion(true, false, false),
        completion(false, false, true)
      ]
    );
  }

  #[gpui_kit::test]
  fn show_toast_emits_an_event(cx: &mut TestAppContext) {
    let ui = state(cx);
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = seen.clone();
    let _subscription =
      cx.update(|cx| cx.subscribe(&ui, move |_, event: &UiEvent, _| sink.borrow_mut().push(event.clone())));
    ui.update(cx, |ui, cx| {
      ui.show_toast(NotificationKind::Error, "a.mkv".into(), "boom".into(), cx)
    });
    assert_eq!(
      seen.borrow().as_slice(),
      [UiEvent::Toast(Toast {
        kind: NotificationKind::Error,
        file: "a.mkv".into(),
        detail: "boom".into(),
      })]
    );
  }

  #[gpui_kit::test]
  fn one_toolbar_menu_is_open_at_a_time(cx: &mut TestAppContext) {
    let ui = state(cx);
    ui.update(cx, |ui, cx| ui.show_menu(ToolbarMenu::Overflow, cx));
    assert_eq!(ui.read_with(cx, |ui, _| ui.open_menu()), Some(ToolbarMenu::Overflow));
    ui.update(cx, |ui, cx| ui.show_menu(ToolbarMenu::Profile, cx));
    assert_eq!(ui.read_with(cx, |ui, _| ui.open_menu()), Some(ToolbarMenu::Profile));
    // Hiding a menu that is not open changes nothing.
    ui.update(cx, |ui, cx| ui.hide_menu(ToolbarMenu::Overflow, cx));
    assert_eq!(ui.read_with(cx, |ui, _| ui.open_menu()), Some(ToolbarMenu::Profile));
    ui.update(cx, |ui, cx| ui.hide_menu(ToolbarMenu::Profile, cx));
    assert_eq!(ui.read_with(cx, |ui, _| ui.open_menu()), None);
  }

  #[gpui_kit::test]
  fn the_update_banner_shows_and_hides_a_version(cx: &mut TestAppContext) {
    let ui = state(cx);
    assert_eq!(
      ui.read_with(cx, |ui, _| ui.update_available().map(str::to_string)),
      None
    );
    ui.update(cx, |ui, cx| ui.set_update_available(Some("1.0.1".into()), cx));
    assert_eq!(
      ui.read_with(cx, |ui, _| ui.update_available().map(str::to_string)),
      Some("1.0.1".to_string())
    );
    ui.update(cx, |ui, cx| ui.set_update_available(None, cx));
    assert_eq!(
      ui.read_with(cx, |ui, _| ui.update_available().map(str::to_string)),
      None
    );
  }
}
