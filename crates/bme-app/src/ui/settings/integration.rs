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

//! The Integration page: one group per tool with its path row. The text is
//! checked 250 ms after the last edit and stored when the field loses the
//! focus; Browse picks a folder and stores it; Detect also looks for a running
//! application. A detection that finds the tool somewhere else (a running
//! application, a versioned macOS bundle, the folder of an executable) puts
//! that path into the field and the config.

use std::time::Duration;

use gpui_kit::assets::IconName;
use gpui_kit::base::{TestSupportExt, TestTextExt};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::setting::{SettingField, SettingGroup, SettingItem, SettingPage};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, h_flex, v_flex};
use gpui_kit::*;

use super::{BUTTON_GROUP_GAP, FIELD_MIN_WIDTH};
use crate::state::Stores;
use crate::tools::{Tool, ToolStatus};
use crate::ui::text_button;
use crate::ui::word_navigation::PATH_CONTEXT;

/// How long the text must stay unchanged before it is checked.
pub const DETECT_DELAY: Duration = Duration::from_millis(250);

pub struct ToolPathRow {
  stores: Stores,
  tool: Tool,
  /// The window of the field, for edits from a task.
  window: AnyWindowHandle,
  input: Entity<InputState>,
  /// The last detection's outcome; `None` before the first one.
  found: Option<bool>,
  /// The path the config held when the field was last synced from it.
  synced: String,
  /// Each detection's number; a slower, older one is ignored.
  generation: u64,
  pending: Option<Task<()>>,
  _subscriptions: Vec<Subscription>,
}

impl ToolPathRow {
  pub fn new(stores: Stores, tool: Tool, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let path = tool.path(stores.config.read(cx).config()).to_string();
    let input = cx.new(|cx| InputState::new(window, cx).default_value(path.clone()));
    let subscriptions = vec![
      cx.subscribe(&input, |this, _, event: &InputEvent, cx| match event {
        InputEvent::Change => this.schedule(DETECT_DELAY, false, cx),
        InputEvent::Blur => this.persist_text(cx),
        _ => {}
      }),
      cx.observe_in(&stores.config, window, |this, _, window, cx| this.sync(window, cx)),
    ];
    let mut row = Self {
      stores,
      tool,
      window: window.window_handle(),
      input,
      found: None,
      synced: path,
      generation: 0,
      pending: None,
      _subscriptions: subscriptions,
    };
    // The stored path is checked once it is shown.
    row.schedule(DETECT_DELAY, false, cx);
    row
  }

  pub fn input(&self) -> &Entity<InputState> {
    &self.input
  }

  /// The last detection's outcome.
  pub fn found(&self) -> Option<bool> {
    self.found
  }

  /// The field's text, trimmed.
  fn text(&self, cx: &App) -> String {
    self.input.read(cx).value().trim().to_string()
  }

  fn configured(&self, cx: &App) -> String {
    self.tool.path(self.stores.config.read(cx).config()).to_string()
  }

  /// Stores `path` unless the config holds it already.
  fn persist(&self, path: String, cx: &mut App) {
    if path != self.configured(cx) {
      let tool = self.tool;
      self
        .stores
        .config
        .update(cx, |store, cx| store.update(cx, |config| tool.set_path(config, path)));
    }
  }

  /// The field lost the focus: its trimmed text is stored.
  fn persist_text(&mut self, cx: &mut Context<Self>) {
    self.persist(self.text(cx), cx);
  }

  /// Follows a path changed elsewhere (Reset All, a detection) unless the
  /// field is being edited.
  fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let configured = self.configured(cx);
    if configured == self.synced {
      return;
    }
    self.synced = configured.clone();
    if self.input.read(cx).focus_handle(cx).is_focused(window) || self.text(cx) == configured {
      return;
    }
    self
      .input
      .update(cx, |input, cx| input.set_value(configured, window, cx));
    self.schedule(DETECT_DELAY, false, cx);
  }

  /// Checks the field's text after `delay` (none for Detect), replacing a
  /// check still waiting.
  fn schedule(&mut self, delay: Duration, check_running: bool, cx: &mut Context<Self>) {
    self.generation += 1;
    let (generation, tool, tools) = (self.generation, self.tool, self.stores.tools.clone());
    self.pending = Some(cx.spawn(async move |this, cx| {
      if !delay.is_zero() {
        cx.background_executor().timer(delay).await;
      }
      let Ok(path) = this.read_with(cx, |this, cx| this.text(cx)) else {
        return;
      };
      let source = path.clone();
      let status = cx
        .background_executor()
        .spawn(async move { tools.detect(tool, &source, check_running) })
        .await;
      let _ = this.update(cx, |this, cx| {
        if this.generation == generation {
          this.apply(status, &path, cx);
        }
      });
    }));
  }

  /// Applies a detection: the outcome, and a path found elsewhere into the
  /// field and the config.
  fn apply(&mut self, status: ToolStatus, source: &str, cx: &mut Context<Self>) {
    self.found = Some(status.found);
    self.pending = None;
    if status.found && !status.path.is_empty() && status.path != source {
      self.set_text(status.path.clone(), cx);
      self.persist(status.path, cx);
    }
    cx.notify();
  }

  /// Puts `path` into the field without emitting a change.
  fn set_text(&mut self, path: String, cx: &mut Context<Self>) {
    let input = self.input.clone();
    let _ = cx.update_window(self.window, |_, window, cx| {
      input.update(cx, |input, cx| input.set_value(path, window, cx))
    });
  }

  /// Detect: now, a running application first.
  pub fn detect_now(&mut self, cx: &mut Context<Self>) {
    self.schedule(Duration::ZERO, true, cx);
  }

  /// A folder picked by Browse: into the field and the config, then checked.
  pub fn choose(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
    self
      .input
      .update(cx, |input, cx| input.set_value(path.clone(), window, cx));
    self.persist(path, cx);
    self.schedule(DETECT_DELAY, false, cx);
  }

  fn browse(&mut self, cx: &mut Context<Self>) {
    let chosen = cx.prompt_for_paths(PathPromptOptions {
      files: false,
      directories: true,
      multiple: false,
      prompt: None,
    });
    cx.spawn(async move |this, cx| {
      let path = match chosen.await {
        Ok(Ok(Some(paths))) => paths.into_iter().next(),
        Ok(Err(error)) => {
          log::error!("The folder dialog failed: {error}");
          None
        }
        _ => None,
      };
      let Some(path) = path else {
        return;
      };
      let path = path.to_string_lossy().into_owned();
      let Ok(window) = this.read_with(cx, |this, _| this.window) else {
        return;
      };
      let _ = cx.update_window(window, |_, window, cx| {
        let _ = this.update(cx, |this, cx| this.choose(path, window, cx));
      });
    })
    .detach();
  }

  /// Reset All: the platform's default path.
  pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.choose(self.tool.default_path(), window, cx);
  }
}

impl Render for ToolPathRow {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let name = self.tool.name();
    let (found_key, missing_key) = match self.tool {
      Tool::MkvToolNix => ("settings.mkvToolNixFound", "settings.mkvToolNixNotFound"),
      Tool::BetterMediaInfo => ("settings.betterMediaInfoFound", "settings.betterMediaInfoNotFound"),
    };
    let theme = cx.theme();
    let (success, danger) = (theme.success, theme.danger);
    let caption = self.found.map(|found| {
      let text = if found { tr!(found_key) } else { tr!(missing_key) };
      div()
        .id(SharedString::from(format!("settings-{name}-status")))
        .test_support()
        .test_text(text.clone())
        .text_xs()
        .text_color(if found { success } else { danger })
        .child(text)
    });
    v_flex()
      .id(SharedString::from(format!("settings-{name}")))
      .test_support()
      .w_full()
      .gap(px(6.0))
      .child(
        // The buttons move below the field when it would get narrower than
        // FIELD_MIN_WIDTH.
        h_flex()
          .w_full()
          .flex_wrap()
          .gap(px(8.0))
          .child(
            div()
              .flex_1()
              .min_w(px(FIELD_MIN_WIDTH))
              .key_context(PATH_CONTEXT)
              .child(Input::new(&self.input).small().w_full()),
          )
          .child(
            h_flex()
              .flex_none()
              .gap(BUTTON_GROUP_GAP)
              .child(
                text_button(SharedString::from(format!("settings-{name}-browse")))
                  .outline()
                  .small()
                  .label(tr!("settings.browse"))
                  .on_click(cx.listener(|this, _, _, cx| this.browse(cx))),
              )
              .child(
                text_button(SharedString::from(format!("settings-{name}-detect")))
                  .outline()
                  .small()
                  .label(tr!("settings.detect"))
                  .on_click(cx.listener(|this, _, _, cx| this.detect_now(cx))),
              ),
          ),
      )
      .children(caption)
  }
}

fn tool_item(stores: &Stores, row: &Entity<ToolPathRow>, tool: Tool) -> SettingItem {
  let title = match tool {
    Tool::MkvToolNix => tr!("settings.mkvToolNixPath"),
    Tool::BetterMediaInfo => tr!("settings.betterMediaInfoPath"),
  };
  let (render_row, reset_row, store) = (row.clone(), row.clone(), stores.config.clone());
  SettingItem::new(
    title,
    SettingField::render(move |_, _, _| render_row.clone())
      .on_reset(
        move |cx| tool.path(store.read(cx).config()) != tool.default_path(),
        move |window, cx| reset_row.update(cx, |row, cx| row.reset(window, cx)),
      )
      .w_full(),
  )
  .layout(Axis::Vertical)
}

pub fn page(stores: &Stores, mkvtoolnix: &Entity<ToolPathRow>, better_media_info: &Entity<ToolPathRow>) -> SettingPage {
  SettingPage::new(tr!("settings.integration"))
    .icon(Icon::new(IconName::Blocks))
    .groups([
      SettingGroup::new()
        .title(tr!("settings.mkvToolNix"))
        .item(tool_item(stores, mkvtoolnix, Tool::MkvToolNix)),
      SettingGroup::new()
        .title(tr!("settings.betterMediaInfo"))
        .item(tool_item(stores, better_media_info, Tool::BetterMediaInfo)),
    ])
}
