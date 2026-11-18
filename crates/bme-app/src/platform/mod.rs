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

//! The window and the OS around it: window geometry, the close guard, the
//! macOS menus, file dialogs, launch arguments and the taskbar progress.

pub mod close_guard;
pub mod dialogs;
pub mod menus;
pub mod taskbar;
pub mod window;

use gpui_kit::{AnyWindowHandle, App};

use crate::state::{Stores, files};
use crate::ui::actions::Quit;

/// Adds the `.mkv` files and folders named on the command line
/// (`get_launch_args`).
pub fn ingest_launch_args(stores: &Stores, cx: &mut App) {
  files::add_paths(
    &stores.files,
    Some(&stores.ui),
    bme_core::services::get_launch_args(),
    cx,
  );
}

/// Adds the files and folders the OS asked the app to open as URLs (macOS:
/// "Open With", a drop on the Dock icon, also while the app runs), as if they
/// were dropped. A URL that is not a local file is left out.
pub fn ingest_open_urls(stores: &Stores, urls: Vec<String>, cx: &mut App) {
  let paths: Vec<std::path::PathBuf> = urls
    .iter()
    .filter_map(|url| {
      let path = bme_core::services::path_from_file_url(url);
      if path.is_none() {
        log::warn!("Not a local file: {url}");
      }
      path
    })
    .collect();
  files::add_paths(&stores.files, Some(&stores.ui), paths, cx);
}

/// Quit (Cmd+Q on macOS) asks like closing the main window does: the window
/// geometry is saved and an active extraction needs a confirmation first.
///
/// A key press is dispatched while its window is being updated, and a window
/// cannot be updated from inside its own update, so the close request is
/// deferred until the dispatch is over. Done inline, the update failed and
/// read as "the window is gone", which quit without asking.
pub fn install_quit(handle: AnyWindowHandle, stores: &Stores, cx: &mut App) {
  let stores = stores.clone();
  cx.on_action(move |_: &Quit, cx| {
    let stores = stores.clone();
    cx.defer(move |cx| {
      let closed = handle.update(cx, |_, window, cx| window::close_requested(&stores, window, cx));
      // The window is gone already, or it agreed to close.
      if closed.unwrap_or(true) {
        cx.quit();
      }
    });
  });
}
