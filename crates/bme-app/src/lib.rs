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

//! BatchMkvExtract desktop application on gpui-kit.
//!
//! Everything lives in this library so that tests run against it; the binary
//! only calls [`run`]. For now the application is a skeleton: one themed
//! window with the app's icon, name and version, so that the packaging
//! pipeline can be proven before the screens are ported.

pub mod assets;

use gpui_kit::component::{ActiveTheme, Theme, v_flex};
use gpui_kit::*;

/// Product name: window title, menus, notifications; also the name of the
/// config file and its directory.
pub const APP_NAME: &str = bme_core::constants::APP_NAME;
/// Bundle identifier / Windows AppUserModelID; equal to 0.3.0's.
pub const APP_IDENTIFIER: &str = "com.caoccao.batch-mkvextract";
/// Linux desktop id: basename of the installed `.desktop` file, the Wayland
/// app id, `StartupWMClass` and the launcher-progress URI all use it.
pub const DESKTOP_ID: &str = "BatchMkvExtract";
/// The application version, shown in the title and About.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 0.3.0's default window size (`config.window.size`).
const DEFAULT_WINDOW_SIZE: (f32, f32) = (1200.0, 900.0);
/// 0.3.0's smallest window (`tauri.conf.json`).
const MIN_WINDOW_SIZE: (f32, f32) = (600.0, 450.0);

actions!(batch_mkvextract, [Quit]);

/// The window title: `BatchMkvExtract v<version>`.
pub fn window_title() -> SharedString {
  format!("{APP_NAME} v{APP_VERSION}").into()
}

/// Starts the application and blocks until it quits.
pub fn run() {
  gpui_kit::application().with_assets(assets::AppAssets).run(|cx| {
    gpui_kit::init(cx);
    cx.set_app_identity(APP_IDENTIFIER, APP_NAME);
    #[cfg(target_os = "macos")]
    install_app_menu(cx);
    open_main_window(cx);
    // Bring the app to the front also when started from a terminal; GPUI
    // delivers focus events only to the active window.
    cx.activate(true);
    cx.on_window_closed(|cx, _| {
      if cx.windows().is_empty() {
        cx.quit();
      }
    })
    .detach();
  });
}

/// macOS application menu. Elsewhere Ctrl+Q is the app's Clear shortcut and
/// closing the last window quits.
#[cfg(target_os = "macos")]
fn install_app_menu(cx: &mut App) {
  cx.on_action(|_: &Quit, cx| cx.quit());
  cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
  cx.set_menus([Menu::new(APP_NAME).items([MenuItem::action("Quit", Quit)])]);
}

/// Window options of the main window: centred at 0.3.0's default size, its
/// 600×450 minimum, the desktop id as the Wayland app id.
pub fn main_window_options(cx: &App) -> WindowOptions {
  let (width, height) = DEFAULT_WINDOW_SIZE;
  let (min_width, min_height) = MIN_WINDOW_SIZE;
  WindowOptions {
    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
      None,
      size(px(width), px(height)),
      cx,
    ))),
    titlebar: Some(TitlebarOptions {
      title: Some(window_title()),
      ..Default::default()
    }),
    window_min_size: Some(size(px(min_width), px(min_height))),
    app_id: Some(DESKTOP_ID.into()),
    ..Default::default()
  }
}

/// Opens the main window with the [`SkeletonView`].
pub fn open_main_window(cx: &mut App) -> Option<AnyWindowHandle> {
  let options = main_window_options(cx);
  match gpui_kit::open_window(options, cx, |window, cx| {
    Theme::sync_system_appearance(Some(window), cx);
    cx.new(|cx| SkeletonView::new(window, cx))
  }) {
    Ok((handle, _)) => Some(handle),
    Err(error) => {
      log::error!("Failed to open the main window: {error}");
      cx.quit();
      None
    }
  }
}

/// Placeholder content until the real layout lands: the app's icon, name and
/// version in gpui-kit's default theme, light or dark as the OS is.
pub struct SkeletonView {
  _appearance: Subscription,
}

impl SkeletonView {
  pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let appearance = cx.observe_window_appearance(window, |_, window, cx| {
      Theme::sync_system_appearance(Some(window), cx);
    });
    Self {
      _appearance: appearance,
    }
  }
}

impl Render for SkeletonView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    v_flex()
      .size_full()
      .items_center()
      .justify_center()
      .gap_3()
      .bg(cx.theme().background)
      .text_color(cx.theme().foreground)
      .child(img(assets::APP_ICON).size(px(128.0)))
      .child(div().text_2xl().font_weight(FontWeight::SEMIBOLD).child(APP_NAME))
      .child(
        div()
          .text_color(cx.theme().muted_foreground)
          .child(format!("v{APP_VERSION}")),
      )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  // The glob of `gpui_kit` also exports GPUI's `test` macro.
  use core::prelude::v1::test;

  #[test]
  fn the_window_title_names_the_app_and_its_version() {
    assert_eq!(
      window_title().as_ref(),
      format!("BatchMkvExtract v{}", env!("CARGO_PKG_VERSION"))
    );
  }

  /// The test platform records a title only from `set_window_title`, not
  /// from the options a window opens with, so the options are checked.
  #[gpui_kit::test]
  fn the_main_window_opens_with_its_title_and_minimum_size(cx: &mut TestAppContext) {
    let handle = cx.update(|cx| {
      gpui_kit::init(cx);
      let options = main_window_options(cx);
      let title = options.titlebar.as_ref().and_then(|titlebar| titlebar.title.clone());
      assert_eq!(title, Some(window_title()));
      assert_eq!(options.window_min_size, Some(size(px(600.0), px(450.0))));
      assert_eq!(options.app_id.as_deref(), Some(DESKTOP_ID));
      open_main_window(cx).expect("the main window")
    });
    let window = VisualTestContext::from_window(handle, cx);
    window.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
  }
}
