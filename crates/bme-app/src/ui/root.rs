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

//! The window content: update banner, toolbar, tabs and status bar, plus what
//! belongs to the whole window: toasts, geometry persistence, live language
//! and theme changes, and the close request.

use std::time::Duration;

use bme_core::config::{DisplayMode, Language, Theme as ThemeName};
use gpui_kit::base::TestSupportExt;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme, WindowExt, v_flex};
use gpui_kit::*;

use super::main_content::MainContent;
use super::status_bar::AppStatusBar;
use super::toolbar::Toolbar;
use super::update_banner::UpdateBanner;
use crate::platform::window as main_window;
use crate::state::ui::{NotificationKind, UiEvent};
use crate::state::{ConfigStore, Stores};

/// How long the window must stay still before its geometry is saved.
const GEOMETRY_SAVE_DELAY: Duration = Duration::from_millis(500);

pub struct AppView {
  stores: Stores,
  banner: Entity<UpdateBanner>,
  toolbar: Entity<Toolbar>,
  main: Entity<MainContent>,
  /// The applied language, theme and display mode, to act only on changes.
  language: Language,
  theme: (ThemeName, DisplayMode),
  save_geometry: Option<Task<()>>,
  _subscriptions: Vec<Subscription>,
}

impl AppView {
  pub fn new(stores: Stores, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let config = stores.config.read(cx).config().clone();
    let (language, theme) = (config.language, (config.theme, config.display_mode));
    let subscriptions = vec![
      cx.observe_in(&stores.config, window, Self::config_changed),
      cx.observe_window_appearance(window, |this, window, cx| {
        if this.theme.1 == DisplayMode::Auto {
          let config = this.stores.config.read(cx).config().clone();
          crate::theme::apply(&config, window.appearance(), cx);
        }
      }),
      cx.observe_window_bounds(window, |this, window, cx| this.schedule_geometry_save(window, cx)),
      cx.subscribe_in(&stores.ui, window, |this, _, event: &UiEvent, window, cx| {
        let notification = |kind: NotificationKind, message: &str| match kind {
          NotificationKind::Success => Notification::success(message.to_string()),
          NotificationKind::Error => Notification::error(message.to_string()),
        };
        let notification = match event {
          UiEvent::CloseActiveTab => {
            if !window.has_active_dialog(cx) {
              this.stores.ui.update(cx, |ui, cx| ui.close_active_tab(cx));
            }
            return;
          }
          UiEvent::Toast(toast) => notification(toast.kind, &toast.detail)
            .title(toast.file.clone())
            .placement(Anchor::TopCenter),
          UiEvent::Snackbar(snackbar) => notification(snackbar.kind, &snackbar.message).placement(Anchor::BottomCenter),
          // The Settings dialog and the queue's completion notices are not
          // part of the shell.
          UiEvent::OpenSettings | UiEvent::Completion(_) | UiEvent::QuitAfterCompletion => return,
        };
        window.push_notification(notification, cx);
      }),
    ];
    let close_stores = stores.clone();
    window.on_window_should_close(cx, move |window, cx| {
      main_window::close_requested(&close_stores, window, cx)
    });
    // The system appearance as this window sees it (more reliable on Linux
    // than the application-wide value used before the window existed).
    if theme.1 == DisplayMode::Auto {
      crate::theme::apply(&config, window.appearance(), cx);
    }
    Self {
      banner: cx.new(|cx| UpdateBanner::new(stores.clone(), cx)),
      toolbar: cx.new(|cx| Toolbar::new(stores.clone(), cx)),
      main: cx.new(|cx| MainContent::new(stores.clone(), cx)),
      stores,
      language,
      theme,
      save_geometry: None,
      _subscriptions: subscriptions,
    }
  }

  pub fn stores(&self) -> &Stores {
    &self.stores
  }

  pub fn main(&self) -> &Entity<MainContent> {
    &self.main
  }

  pub fn toolbar(&self) -> &Entity<Toolbar> {
    &self.toolbar
  }

  /// Applies a changed language, theme or display mode at once.
  fn config_changed(&mut self, config: Entity<ConfigStore>, window: &mut Window, cx: &mut Context<Self>) {
    let config = config.read(cx).config().clone();
    if config.language != self.language {
      self.language = config.language;
      crate::i18n::set_language(config.language);
      cx.refresh_windows();
    }
    let theme = (config.theme, config.display_mode);
    if theme != self.theme {
      self.theme = theme;
      crate::theme::apply(&config, window.appearance(), cx);
    }
  }

  /// Saves the geometry once the window has been still for a moment.
  fn schedule_geometry_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.save_geometry = Some(cx.spawn_in(window, async move |this, cx| {
      cx.background_executor().timer(GEOMETRY_SAVE_DELAY).await;
      let _ = this.update_in(cx, |this, window, cx| {
        main_window::save_geometry(&this.stores, window, cx);
        this.save_geometry = None;
      });
    }));
  }
}

impl Render for AppView {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    v_flex()
      .id("app-root")
      .test_support()
      .size_full()
      .bg(cx.theme().background)
      .text_color(cx.theme().foreground)
      .child(
        v_flex()
          .flex_1()
          .min_h_0()
          .px(super::SHELL_MARGIN)
          .child(self.banner.clone())
          .child(self.toolbar.clone())
          .child(div().flex_1().min_h_0().child(self.main.clone())),
      )
      .child(AppStatusBar)
  }
}
