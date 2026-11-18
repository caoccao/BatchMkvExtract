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

//! The macOS menu bar: the application menu with Quit (Cmd+Q) and an Edit
//! menu whose items are the system edit actions, so Cmd+Z/X/C/V/A reach the
//! focused text input. Elsewhere there is no menu bar: Ctrl+Q is the app's
//! Clear shortcut and closing the last window quits.

use gpui_kit::App;

#[cfg(target_os = "macos")]
pub fn install(cx: &mut App) {
  use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
  use gpui_kit::{KeyBinding, Menu, MenuItem, OsAction};

  use crate::ui::actions::{OpenSettings, Quit};

  cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
  cx.set_menus([
    Menu::new(crate::APP_NAME).items([
      MenuItem::action(tr!("settings.title"), OpenSettings),
      MenuItem::separator(),
      MenuItem::action(format!("Quit {}", crate::APP_NAME), Quit),
    ]),
    Menu::new("Edit").items([
      MenuItem::os_action("Undo", Undo, OsAction::Undo),
      MenuItem::os_action("Redo", Redo, OsAction::Redo),
      MenuItem::separator(),
      MenuItem::os_action("Cut", Cut, OsAction::Cut),
      MenuItem::os_action("Copy", Copy, OsAction::Copy),
      MenuItem::os_action("Paste", Paste, OsAction::Paste),
      MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
    ]),
  ]);
}

#[cfg(not(target_os = "macos"))]
pub fn install(_cx: &mut App) {}
