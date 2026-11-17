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

//! The topmost completion window: an undecorated 440×200 window above every
//! other application's, centred on the work area of the main window's display,
//! showing the title, the file or counts, the detail and the dismissal
//! buttons. A later completion updates the open window in place.
//!
//! The window is GPUI's `PopUp` kind (an `NSPanel` above other applications
//! on macOS, `WS_EX_TOPMOST` on Windows, override-redirect on X11). On
//! Wayland a `PopUp` is an ordinary window, so it opens as an overlay
//! layer-shell surface there, and as a `PopUp` where the compositor has no
//! layer-shell (GNOME).

use gpui_kit::assets::IconName;
use gpui_kit::base::{TestSupportExt as _, TestTextExt as _};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;

use crate::state::ui::{CompletionNotice, UiState};
use crate::ui::{confirm_button, text_button};

pub const KEY_CONTEXT: &str = "TopmostNotification";
/// The window's size.
pub const SIZE: (f32, f32) = (440.0, 200.0);

actions!(
  topmost_notification,
  [
    /// Escape: close the notification and leave the app open.
    Dismiss,
    /// Enter: accept the notification's primary action.
    Accept,
  ]
);

pub fn bind_keys(cx: &mut App) {
  cx.bind_keys([
    KeyBinding::new("escape", Dismiss, Some(KEY_CONTEXT)),
    KeyBinding::new("enter", Accept, Some(KEY_CONTEXT)),
  ]);
}

/// The window's content.
pub struct TopmostNotification {
  notice: CompletionNotice,
  quit: Option<WeakEntity<UiState>>,
  focus: FocusHandle,
}

impl TopmostNotification {
  fn new(notice: CompletionNotice, quit: Option<WeakEntity<UiState>>, cx: &mut Context<Self>) -> Self {
    Self {
      notice,
      quit,
      focus: cx.focus_handle(),
    }
  }

  pub fn notice(&self) -> &CompletionNotice {
    &self.notice
  }

  fn accept(&self, window: &mut Window, cx: &mut Context<Self>) {
    window.remove_window();
    if let Some(ui) = &self.quit {
      let _ = ui.update(cx, |ui, cx| ui.request_completion_quit(cx));
    }
  }
}

impl Render for TopmostNotification {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let notice = &self.notice;
    let (icon, color) = if notice.failed {
      (IconName::CircleAlert, theme.danger)
    } else {
      (IconName::CircleCheck, theme.success)
    };
    let line = notice.line.clone();
    let quit = self.quit.is_some();
    v_flex()
      .id("topmost-notification")
      .test_support()
      .track_focus(&self.focus)
      .key_context(KEY_CONTEXT)
      .on_action(|_: &Dismiss, window, _| window.remove_window())
      .on_action(cx.listener(|this, _: &Accept, window, cx| this.accept(window, cx)))
      .size_full()
      .p(super::space(2.0))
      .gap(super::space(1.25))
      .bg(theme.popover)
      .text_color(theme.popover_foreground)
      .border_1()
      .border_color(theme.border)
      .child(
        h_flex()
          .gap(super::space(1.0))
          .child(Icon::new(icon).size(px(24.0)).flex_none().text_color(color))
          .child(
            div()
              .min_w_0()
              .text_size(super::text::h6())
              .font_weight(FontWeight::MEDIUM)
              .child(notice.title.clone())
              .test_text(notice.title.clone()),
          ),
      )
      .child(
        div()
          .id("topmost-line")
          .w_full()
          .truncate()
          .text_size(super::text::body2())
          .font_weight(FontWeight::MEDIUM)
          .child(notice.line.clone())
          .test_text(notice.line.clone())
          .tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(line.clone()).build(window, cx)),
      )
      .child(
        div()
          .text_size(super::text::body2())
          .child(notice.detail.clone())
          .test_text(notice.detail.clone()),
      )
      .child(div().flex_1())
      .child(
        h_flex()
          .justify_end()
          .gap(super::space(1.0))
          .children(quit.then(|| {
            text_button("topmost-keep-open")
              .small()
              .outline()
              .label(tr!("notification.okButDoNotQuit"))
              .on_click(|_, window, _| window.remove_window())
          }))
          .child(
            confirm_button("topmost-close")
              .small()
              .label(if quit {
                tr!("notification.quit")
              } else {
                tr!("tabs.close")
              })
              .on_click(cx.listener(|this, _, window, cx| this.accept(window, cx))),
          ),
      )
  }
}

/// The topmost window, kept to be reused while it is open.
#[derive(Default)]
pub struct TopmostWindow {
  window: Option<AnyWindowHandle>,
  view: Option<Entity<TopmostNotification>>,
  /// Windows opened so far; a reused window does not count.
  opened: usize,
}

impl TopmostWindow {
  /// The window, while it is (or was last) open.
  pub fn handle(&self) -> Option<AnyWindowHandle> {
    self.window
  }

  pub fn opened(&self) -> usize {
    self.opened
  }

  /// Shows `notice`: in the open window, brought to the front, or in a new
  /// one centred on the display of `main`.
  pub fn show(&mut self, notice: CompletionNotice, quit: Option<WeakEntity<UiState>>, main: &Window, cx: &mut App) {
    if let (Some(handle), Some(view)) = (self.window, self.view.clone()) {
      let reused = handle.update(cx, |_, window, cx| {
        view.update(cx, |view, cx| {
          view.notice = notice.clone();
          view.quit = quit.clone();
          cx.notify();
        });
        window.activate_window();
        view.read(cx).focus.clone().focus(window, cx);
      });
      if reused.is_ok() {
        return;
      }
    }
    let display = main.display(cx);
    let center = display
      .as_ref()
      .map(|display| display.visible_bounds().center())
      .unwrap_or_else(|| main.bounds().center());
    let options = |kind| WindowOptions {
      window_bounds: Some(WindowBounds::Windowed(Bounds::centered_at(
        center,
        size(px(SIZE.0), px(SIZE.1)),
      ))),
      titlebar: None,
      kind,
      is_resizable: false,
      is_minimizable: false,
      is_movable: false,
      focus: true,
      show: true,
      display_id: display.as_ref().map(|display| display.id()),
      app_id: Some(crate::DESKTOP_ID.into()),
      ..Default::default()
    };
    let build = |notice: CompletionNotice, quit: Option<WeakEntity<UiState>>| {
      move |window: &mut Window, cx: &mut App| {
        let view = cx.new(|cx| TopmostNotification::new(notice, quit, cx));
        view.read(cx).focus.clone().focus(window, cx);
        view
      }
    };
    let mut opened = gpui_kit::open_window(options(window_kind(cx)), cx, build(notice.clone(), quit.clone()));
    if opened.is_err() && window_kind(cx) != WindowKind::PopUp {
      // A Wayland compositor without layer-shell.
      opened = gpui_kit::open_window(options(WindowKind::PopUp), cx, build(notice, quit));
    }
    match opened {
      Ok((handle, view)) => {
        self.window = Some(handle);
        self.view = Some(view);
        self.opened += 1;
      }
      Err(error) => log::error!("Failed to open the topmost notification: {error}"),
    }
  }
}

/// `PopUp`, except on Wayland: an overlay layer-shell surface.
fn window_kind(cx: &App) -> WindowKind {
  #[cfg(target_os = "linux")]
  {
    if cx.compositor_name() == "Wayland" {
      use gpui_kit::layer_shell::{KeyboardInteractivity, Layer, LayerShellOptions};
      return WindowKind::LayerShell(LayerShellOptions {
        namespace: crate::DESKTOP_ID.to_string(),
        layer: Layer::Overlay,
        keyboard_interactivity: KeyboardInteractivity::OnDemand,
        ..Default::default()
      });
    }
  }
  #[cfg(not(target_os = "linux"))]
  let _ = cx;
  WindowKind::PopUp
}
