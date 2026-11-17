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

//! The single-row toolbar and its menus. Actions that do not fit are shown
//! in an overflow menu. Profile also opens with F9.
//!
//! Every button of the toolbar is drawn; those whose store does not exist yet
//! (the global output path, the file list's and the queue's actions, Group
//! Mode) and Settings are disabled until it does.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::popover::Popover;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Side, Sizable, h_flex};
use gpui_kit::*;

use crate::assets::{OUTPUT_DIRECTORY_ICON_NAME, QUIT_ICON_NAME, toolbar_icon};
use crate::state::ui::ToolbarMenu;
use crate::state::{Stores, TabType};

const BUTTON_SIZE: Pixels = px(40.0);
const BUTTON_GAP: Pixels = px(2.0);
const OVERFLOW_WIDTH: Pixels = px(24.0);
// The row gap supplies 2 pt of the spacing on each side of the divider.
const DIVIDER_WIDTH: Pixels = px(5.0);

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolbarAction {
  OutputPath,
  ExtractAll,
  ResumeAll,
  CancelAll,
  ClearCompleted,
  ClearFiles,
  QuitWhenComplete,
  SystemNotification,
  TopmostNotification,
  GroupMode,
  Settings,
  About,
  Menu(ToolbarMenu),
}

impl ToolbarAction {
  fn run(self, stores: &Stores, _window: &mut Window, cx: &mut App) {
    match self {
      Self::QuitWhenComplete => stores.ui.update(cx, |ui, cx| ui.toggle_quit_when_complete(cx)),
      Self::SystemNotification => stores.ui.update(cx, |ui, cx| ui.toggle_system_notification(cx)),
      Self::TopmostNotification => stores.ui.update(cx, |ui, cx| ui.toggle_topmost_notification(cx)),
      Self::Settings => stores.ui.update(cx, |ui, cx| ui.open_settings(cx)),
      Self::About => stores.ui.update(cx, |ui, cx| ui.open_about(cx)),
      Self::Menu(menu) => stores.ui.update(cx, |ui, cx| ui.show_menu(menu, cx)),
      // Disabled until their stores exist.
      Self::OutputPath
      | Self::ExtractAll
      | Self::ResumeAll
      | Self::CancelAll
      | Self::ClearCompleted
      | Self::ClearFiles
      | Self::GroupMode => {}
    }
  }
}

/// A button's presentation and command are shared by the toolbar and its menu.
#[derive(Clone, PartialEq)]
struct ToolbarButton {
  id: &'static str,
  icon: &'static str,
  label: SharedString,
  tooltip: Option<SharedString>,
  color: Option<Hsla>,
  enabled: bool,
  checked: Option<bool>,
  action: ToolbarAction,
}

impl ToolbarButton {
  fn icon(&self) -> Icon {
    let icon = toolbar_icon(self.icon);
    match self.color.filter(|_| self.enabled) {
      Some(color) => icon.text_color(color),
      None => icon,
    }
  }

  fn button(&self, extract_width: Pixels) -> Button {
    let tooltip = self.tooltip.as_ref().unwrap_or(&self.label).clone();
    let button = Button::new(self.id)
      .ghost()
      .h(BUTTON_SIZE)
      .tooltip(tooltip.clone())
      .disabled(!self.enabled);
    if self.action == ToolbarAction::ExtractAll {
      button
        .with_size(BUTTON_SIZE)
        .w(extract_width)
        .icon(self.icon())
        .label(self.label.clone())
    } else {
      let button = button
        .w(BUTTON_SIZE)
        .px_0()
        .child(self.icon().with_size(px(32.0)))
        .accessibility_label(tooltip);
      match self.checked {
        Some(checked) => button.toggled(checked),
        None => button,
      }
    }
  }
}

/// Only Extract All has a label in the row. A custom-size gpui-kit Button
/// uses a base-size label, a 75% icon, a half-rem gap and 20% padding on each
/// side.
fn extract_button_width(label: &SharedString, window: &Window) -> Pixels {
  let rem = window.rem_size();
  let run = window.text_style().to_run(label.len());
  let text = window
    .text_system()
    .layout_line(label.as_ref(), rem, &[run], None)
    .width;
  BUTTON_SIZE * 1.15 + rem * 0.5 + text.ceil() + px(1.0)
}

/// Keep the largest prefix that fits, reserving the chevron only if needed.
/// Separators belong between visible controls, so a trailing one moves with
/// the hidden controls.
fn visible_buttons(items: &[Option<ToolbarButton>], extract_width: Pixels, available: Pixels) -> usize {
  let width = |item: &Option<ToolbarButton>| match item {
    Some(button) if button.action == ToolbarAction::ExtractAll => extract_width,
    Some(_) => BUTTON_SIZE,
    None => DIVIDER_WIDTH,
  };
  let total =
    items.iter().fold(px(0.0), |total, item| total + width(item)) + BUTTON_GAP * items.len().saturating_sub(1) as f32;
  if total <= available {
    return items.len();
  }
  let mut used = OVERFLOW_WIDTH;
  let mut visible = 0;
  for (index, item) in items.iter().enumerate() {
    used += width(item) + BUTTON_GAP;
    if used > available {
      break;
    }
    if item.is_some() {
      visible = index + 1;
    }
  }
  visible
}

/// The open menus' entities; a menu is built when it opens and dropped when
/// it closes, so it always lists the current choices.
type Menus = Rc<RefCell<HashMap<ToolbarMenu, Entity<PopupMenu>>>>;

pub struct Toolbar {
  stores: Stores,
  menus: Menus,
  overflow: Vec<Option<ToolbarButton>>,
  _subscriptions: Vec<Subscription>,
}

impl Toolbar {
  pub fn new(stores: Stores, cx: &mut Context<Self>) -> Self {
    let subscriptions = vec![
      cx.observe(&stores.config, |_, _, cx| cx.notify()),
      cx.observe(&stores.ui, |this, ui, cx| {
        let open = ui.read(cx).open_menu();
        this.menus.borrow_mut().retain(|menu, _| Some(*menu) == open);
        cx.notify();
      }),
    ];
    Self {
      stores,
      menus: Menus::default(),
      overflow: Vec::new(),
      _subscriptions: subscriptions,
    }
  }

  /// The menu's entity while it is open (tests).
  pub fn menu(&self, menu: ToolbarMenu) -> Option<Entity<PopupMenu>> {
    self.menus.borrow().get(&menu).cloned()
  }
}

/// The choices for Profile, used on the row and in an overflow submenu.
fn menu_choices(menu: ToolbarMenu, popup: PopupMenu, stores: &Stores, cx: &App) -> PopupMenu {
  let config = stores.config.read(cx).config().clone();
  let store = stores.config.clone();
  match menu {
    ToolbarMenu::Profile => config.profiles.iter().fold(popup, |popup, profile| {
      let (name, store) = (profile.name.clone(), store.clone());
      popup.item(
        PopupMenuItem::new(profile.name.clone())
          .checked(profile.name == config.active_profile)
          .on_click(move |_, _, cx| store.update(cx, |store, cx| store.set_active_profile(&name, cx))),
      )
    }),
    ToolbarMenu::Overflow => popup,
  }
}

fn build_menu(
  stores: &Stores,
  menu: ToolbarMenu,
  hidden: &[Option<ToolbarButton>],
  window: &mut Window,
  cx: &mut App,
) -> Entity<PopupMenu> {
  PopupMenu::build(window, cx, |popup, window, cx| {
    if menu != ToolbarMenu::Overflow {
      return menu_choices(menu, popup, stores, cx);
    }
    hidden.iter().fold(popup.check_side(Side::Right), |popup, item| {
      let Some(button) = item else {
        return popup.separator();
      };
      let stores = stores.clone();
      if let ToolbarAction::Menu(menu) = button.action
        && button.enabled
      {
        return popup.submenu_with_icon(
          Some(button.icon()),
          button.label.clone(),
          window,
          cx,
          move |popup, _, cx| menu_choices(menu, popup, &stores, cx),
        );
      }
      let action = button.action;
      popup.item(
        PopupMenuItem::new(button.label.clone())
          .icon(button.icon())
          .checked(button.checked.unwrap_or(false))
          .disabled(!button.enabled)
          .on_click(move |_, window, cx| {
            let stores = stores.clone();
            window.defer(cx, move |window, cx| action.run(&stores, window, cx));
          }),
      )
    })
  })
}

/// `trigger` with its menu below it, open while [`UiState`] says so; a
/// click on the trigger opens it, a choice, Escape or a click elsewhere
/// closes it.
///
/// [`UiState`]: crate::state::UiState
fn menu_popover(
  stores: &Stores,
  menus: &Menus,
  menu: ToolbarMenu,
  trigger: Button,
  hidden: &[Option<ToolbarButton>],
  cx: &App,
) -> Popover {
  let open = stores.ui.read(cx).open_menu() == Some(menu);
  let ui = stores.ui.clone();
  let (stores, menus) = (stores.clone(), menus.clone());
  let anchor = if hidden.is_empty() {
    Anchor::TopLeft
  } else {
    Anchor::TopRight
  };
  let hidden = hidden.to_vec();
  Popover::new(SharedString::from(format!("toolbar-menu-{}", menu.name())))
    .anchor(anchor)
    .appearance(false)
    .overlay_closable(false)
    .trigger(trigger)
    .open(open)
    .on_open_change(move |open, _, cx| {
      ui.update(cx, |ui, cx| {
        if *open {
          ui.show_menu(menu, cx);
        } else {
          ui.hide_menu(menu, cx);
        }
      })
    })
    .content(move |_, window, cx| {
      if let Some(popup) = menus.borrow().get(&menu).cloned() {
        return popup;
      }
      let popup = build_menu(&stores, menu, &hidden, window, cx);
      popup.focus_handle(cx).focus(window, cx);
      let (ui, closed) = (stores.ui.clone(), menus.clone());
      window
        .subscribe(&popup, cx, move |_, _: &DismissEvent, _, cx| {
          closed.borrow_mut().remove(&menu);
          ui.update(cx, |ui, cx| ui.hide_menu(menu, cx));
        })
        .detach();
      menus.borrow_mut().insert(menu, popup.clone());
      popup
    })
}

fn divider(cx: &App) -> Div {
  div()
    .flex_none()
    .w(px(1.0))
    .h(px(20.0))
    .mx((DIVIDER_WIDTH - px(1.0)) / 2.0)
    .bg(cx.theme().border)
}

impl Render for Toolbar {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let (primary, muted, success, danger) = (theme.primary, theme.muted_foreground, theme.success, theme.danger);
    let has_profiles = !self.stores.config.read(cx).config().profiles.is_empty();
    let ui = self.stores.ui.read(cx);
    let active_tab = ui.active_tab();
    let quit = ui.quit_when_complete();
    let (system, topmost) = (ui.system_notification(), ui.topmost_notification());
    let highlight = |on: bool| on.then_some(primary);
    let settings_tooltip = if cfg!(target_os = "macos") {
      tr!("toolbar.settingsMac")
    } else {
      tr!("toolbar.settings")
    };

    let item = |id, icon, label, color, enabled, action| ToolbarButton {
      id,
      icon,
      label,
      tooltip: None,
      color,
      enabled,
      checked: None,
      action,
    };
    let extract_label = tr!("group.extractAll");
    let extract_width = extract_button_width(&extract_label, window);
    let items = [
      Some(item(
        "toolbar-global-output",
        OUTPUT_DIRECTORY_ICON_NAME,
        tr!("toolbar.setGlobalOutputPath"),
        Some(muted),
        false,
        ToolbarAction::OutputPath,
      )),
      None,
      Some(ToolbarButton {
        tooltip: Some(tr!("toolbar.extractAll")),
        ..item(
          "toolbar-extract-all",
          "extract-all",
          extract_label,
          None,
          false,
          ToolbarAction::ExtractAll,
        )
      }),
      Some(item(
        "toolbar-resume-all",
        "resume-all",
        tr!("queue.resumeAll"),
        Some(success),
        false,
        ToolbarAction::ResumeAll,
      )),
      Some(item(
        "toolbar-cancel-all",
        "cancel-all",
        tr!("toolbar.cancelAll"),
        Some(danger),
        false,
        ToolbarAction::CancelAll,
      )),
      None,
      Some(item(
        "toolbar-clear-completed",
        "clear-completed",
        tr!("queue.clearCompleted"),
        None,
        false,
        ToolbarAction::ClearCompleted,
      )),
      Some(item(
        "toolbar-clear",
        "clear-list",
        tr!("toolbar.clear"),
        None,
        false,
        ToolbarAction::ClearFiles,
      )),
      None,
      Some(ToolbarButton {
        checked: Some(quit),
        ..item(
          "toolbar-quit-when-complete",
          QUIT_ICON_NAME,
          tr!("queue.quitWhenComplete"),
          highlight(quit),
          true,
          ToolbarAction::QuitWhenComplete,
        )
      }),
      Some(ToolbarButton {
        checked: Some(system),
        ..item(
          "toolbar-system-notification",
          "system-notification",
          tr!("queue.systemNotificationTooltip"),
          highlight(system),
          true,
          ToolbarAction::SystemNotification,
        )
      }),
      Some(ToolbarButton {
        checked: Some(topmost),
        ..item(
          "toolbar-topmost-notification",
          "topmost-notification",
          tr!("queue.topmostNotificationTooltip"),
          highlight(topmost),
          true,
          ToolbarAction::TopmostNotification,
        )
      }),
      None,
      Some(ToolbarButton {
        checked: Some(false),
        ..item(
          "toolbar-group-mode",
          "group-mode",
          tr!("toolbar.groupByFile"),
          None,
          false,
          ToolbarAction::GroupMode,
        )
      }),
      Some(item(
        "toolbar-profile",
        "profile",
        tr!("toolbar.profile"),
        None,
        has_profiles,
        ToolbarAction::Menu(ToolbarMenu::Profile),
      )),
      Some(item(
        "toolbar-settings",
        crate::assets::SETTINGS_ICON_NAME,
        settings_tooltip,
        None,
        false,
        ToolbarAction::Settings,
      )),
      None,
      Some(item(
        "toolbar-about",
        "about",
        tr!("toolbar.about"),
        highlight(active_tab == TabType::About),
        true,
        ToolbarAction::About,
      )),
    ];
    let available = window.viewport_size().width - super::SHELL_MARGIN * 2.0;
    let (visible, hidden) = items.split_at(visible_buttons(&items, extract_width, available));
    if self.overflow.as_slice() != hidden {
      self.overflow = hidden.to_vec();
      self.menus.borrow_mut().remove(&ToolbarMenu::Overflow);
    }

    let row = h_flex()
      .id("toolbar")
      .test_support()
      .w_full()
      .h(BUTTON_SIZE)
      .mt(super::SHELL_MARGIN)
      .gap(BUTTON_GAP)
      .children(visible.iter().map(|item| {
        let Some(item) = item else {
          return divider(cx).into_any_element();
        };
        let button = item.button(extract_width);
        match item.action {
          ToolbarAction::Menu(menu) => {
            menu_popover(&self.stores, &self.menus, menu, button, &[], cx).into_any_element()
          }
          action => {
            let stores = self.stores.clone();
            button
              .on_click(move |_, window, cx| action.run(&stores, window, cx))
              .into_any_element()
          }
        }
      }));
    let open_menu = self.stores.ui.read(cx).open_menu();
    if hidden.is_empty() {
      if open_menu == Some(ToolbarMenu::Overflow) {
        let ui = self.stores.ui.clone();
        cx.defer(move |cx| ui.update(cx, |ui, cx| ui.hide_menu(ToolbarMenu::Overflow, cx)));
      }
      return row;
    }

    // A shortcut can open a hidden Profile menu at the chevron.
    let menu = open_menu
      .filter(|menu| {
        hidden
          .iter()
          .flatten()
          .any(|item| item.action == ToolbarAction::Menu(*menu))
      })
      .unwrap_or(ToolbarMenu::Overflow);
    let label = tr!("toolbar.moreActions");
    let button = Button::new("toolbar-overflow")
      .ghost()
      .w(OVERFLOW_WIDTH)
      .h(BUTTON_SIZE)
      .px_0()
      .child(Icon::new(IconName::ChevronsRight).with_size(px(16.0)))
      .accessibility_label(label.clone())
      .tooltip(label);
    row.child(menu_popover(&self.stores, &self.menus, menu, button, hidden, cx))
  }
}
