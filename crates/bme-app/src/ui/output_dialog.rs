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

//! The output path dialog (`OutputPathDialog.tsx`): an output folder in a
//! field with a clear button, Browse for a folder, and one line under them:
//! a hint, the warning while the folder does not exist (checked 300 ms after
//! the last edit; it is created when an extraction starts), or the error of
//! the writable check that OK runs. An empty field confirms "no override".
//! Enter confirms, Escape cancels.
//!
//! A card's and a group card's Set Output Path and the toolbar's global
//! output path open it.

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::{TestSupportExt as _, TestTextExt as _};
use gpui_kit::component::WindowExt;
use gpui_kit::component::dialog::{DialogEntrance, DialogFooter};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Sizable, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::word_navigation::PATH_CONTEXT;
use super::{confirm_button, hex, space, text, text_button};
use crate::assets::{OUTPUT_DIRECTORY_ICON_NAME, toolbar_icon};

/// How long the field must rest before the existence check runs.
const EXISTS_CHECK_DELAY: Duration = Duration::from_millis(300);

/// Receives the confirmed folder, empty to clear the override.
pub type OnConfirm = Rc<dyn Fn(String, &mut App)>;

/// What the dialog edits and what happens on OK.
#[derive(Clone)]
pub struct OutputDialogRequest {
  pub title: SharedString,
  pub initial: String,
  pub on_confirm: OnConfirm,
}

pub struct OutputPathDialog {
  input: Entity<InputState>,
  request: OutputDialogRequest,
  /// The folder does not exist yet (a warning, not an error).
  missing: bool,
  error: Option<SharedString>,
  check: Option<Task<()>>,
  confirming: Option<Task<()>>,
  /// Confirmed and closed: later Enter presses do nothing.
  done: bool,
  _subscription: Subscription,
}

impl OutputPathDialog {
  /// Opens the dialog for `request` with the field focused and selected.
  pub fn open(request: OutputDialogRequest, window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| Self::new(request, window, cx));
    let title = view.read(cx).request.title.clone();
    let focus = view.read(cx).input.read(cx).focus_handle(cx);
    window.open_dialog(cx, move |dialog, window, cx| {
      let (enter, ok) = (view.clone(), view.clone());
      let theme = cx.theme();
      let icon = toolbar_icon(OUTPUT_DIRECTORY_ICON_NAME).with_size(space(4.0));
      dialog
        .w((window.viewport_size().width * 0.6).max(px(480.0)))
        .centered()
        .entrance(DialogEntrance::None)
        .px(space(2.5))
        .title(
          h_flex()
            .gap(space(1.5))
            .pr(space(3.0))
            .pb(space(1.0))
            .child(
              h_flex()
                .size(space(4.0))
                .flex_shrink_0()
                .justify_center()
                .rounded(theme.radius)
                .bg(theme.primary.opacity(0.12))
                .child(icon.text_color(theme.primary)),
            )
            .child(div().text_size(text::h6()).child(title.clone())),
        )
        // The dialog's Enter: the same as OK, which closes the dialog itself
        // once the folder is accepted.
        .on_ok(move |_, window, cx| {
          enter.update(cx, |this, cx| this.confirm(window, cx));
          false
        })
        .on_cancel(|_, _, _| true)
        .child(view.clone())
        .footer(
          DialogFooter::new()
            .border_t_1()
            .border_color(theme.border)
            .pt(space(1.5))
            .child(
              div()
                .grid()
                .grid_cols(2)
                .gap(space(1.0))
                .child(
                  text_button("output-cancel")
                    .outline()
                    .label(tr!("extract.cancel"))
                    .on_click(|_, window, cx| window.close_dialog(cx)),
                )
                .child(
                  confirm_button("output-ok")
                    .label(tr!("extract.ok"))
                    .on_click(move |_, window, cx| ok.update(cx, |this, cx| this.confirm(window, cx))),
                ),
            ),
        )
    });
    window.focus(&focus, cx);
  }

  fn new(request: OutputDialogRequest, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let input = cx.new(|cx| {
      let mut state = InputState::new(window, cx);
      state.set_value(request.initial.clone(), window, cx);
      state
    });
    let subscription = cx.subscribe_in(&input, window, Self::on_input_event);
    let mut dialog = Self {
      input,
      request,
      missing: false,
      error: None,
      check: None,
      confirming: None,
      done: false,
      _subscription: subscription,
    };
    dialog.schedule_check(cx);
    dialog
  }

  pub fn value(&self, cx: &App) -> String {
    self.input.read(cx).value().trim().to_string()
  }

  pub fn is_missing(&self) -> bool {
    self.missing
  }

  pub fn error(&self) -> Option<&SharedString> {
    self.error.as_ref()
  }

  fn on_input_event(
    &mut self,
    input: &Entity<InputState>,
    event: &InputEvent,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    match event {
      InputEvent::Focus => {
        input.update(cx, |state, cx| state.select_all(window, cx));
        cx.notify();
      }
      InputEvent::Blur => cx.notify(),
      InputEvent::Change => {
        self.error = None;
        self.schedule_check(cx);
        cx.notify();
      }
      InputEvent::PressEnter { .. } => self.confirm(window, cx),
    }
  }

  /// Checks 300 ms after the last edit whether the folder exists.
  fn schedule_check(&mut self, cx: &mut Context<Self>) {
    let value = self.value(cx);
    if value.is_empty() {
      self.missing = false;
      self.check = None;
      return;
    }
    self.check = Some(cx.spawn(async move |this, cx| {
      cx.background_executor().timer(EXISTS_CHECK_DELAY).await;
      let exists = cx
        .background_spawn(async move { bme_core::services::output_path_exists(value) })
        .await;
      let _ = this.update(cx, |this, cx| {
        // An error is not a missing folder: no warning for it.
        this.missing = matches!(exists, Ok(false));
        cx.notify();
      });
    }));
  }

  fn set_value(&mut self, value: String, window: &mut Window, cx: &mut Context<Self>) {
    self.input.update(cx, |state, cx| state.set_value(value, window, cx));
    self.error = None;
    self.schedule_check(cx);
    cx.notify();
  }

  /// Browse: a folder picker; the chosen folder replaces the field.
  fn browse(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
      files: false,
      directories: true,
      multiple: false,
      prompt: None,
    });
    cx.spawn_in(window, async move |this, cx| {
      let chosen = match paths.await {
        Ok(Ok(Some(paths))) => Ok(paths.into_iter().next()),
        Ok(Ok(None)) | Err(_) => Ok(None),
        Ok(Err(error)) => Err(error),
      };
      let _ = this.update_in(cx, |this, window, cx| match chosen {
        Ok(Some(path)) => this.set_value(path.to_string_lossy().into_owned(), window, cx),
        Ok(None) => {}
        Err(error) => {
          this.error = Some(error.to_string().into());
          cx.notify();
        }
      });
    })
    .detach();
  }

  /// OK / Enter: an empty field clears the override; any other folder must
  /// be writable (or creatable) first.
  fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.done || self.confirming.is_some() {
      return;
    }
    let value = self.value(cx);
    if value.is_empty() {
      self.done = true;
      (self.request.on_confirm)(String::new(), cx);
      window.close_dialog(cx);
      return;
    }
    let checked = value.clone();
    self.confirming = Some(cx.spawn_in(window, async move |this, cx| {
      let writable = cx
        .background_spawn(async move { bme_core::services::check_output_dir_writable(checked) })
        .await;
      let _ = this.update_in(cx, |this, window, cx| {
        this.confirming = None;
        match writable {
          Ok(true) => {
            this.done = true;
            (this.request.on_confirm)(value, cx);
            window.close_dialog(cx);
          }
          Ok(false) => this.error = Some(tr!("extract.outputPathNotWritable")),
          Err(error) => this.error = Some(error.to_string().into()),
        }
        cx.notify();
      });
    }));
  }
}

impl Render for OutputPathDialog {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let (message, color) = match (&self.error, self.missing) {
      (Some(error), _) => (error.clone(), theme.danger),
      (None, true) => (tr!("extract.outputPathDoesNotExist"), theme.danger),
      (None, false) => {
        let gray = if theme.is_dark() { hex(0xa6a6a6) } else { hex(0x6e6e6e) };
        (tr!("extract.outputPathHint"), gray)
      }
    };
    let ring = theme.ring;
    let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
    v_flex()
      .id("output-dialog")
      .test_support()
      .gap(space(1.0))
      .child(
        h_flex()
          .gap(space(1.0))
          .child(
            div().flex_1().min_w_0().key_context(PATH_CONTEXT).child(
              Input::new(&self.input)
                .id("output-path")
                .aria_label(tr!("extract.outputPath"))
                .cleanable(true)
                .focus_bordered(false)
                .when(focused, |input| input.border_color(ring)),
            ),
          )
          .child(
            text_button("output-browse")
              .outline()
              .flex_shrink_0()
              .label(tr!("extract.browse"))
              .on_click(cx.listener(|this, _, window, cx| this.browse(window, cx))),
          ),
      )
      .child(
        div()
          .id("output-status")
          .w_full()
          .h(space(2.0))
          .flex_shrink_0()
          .line_height(space(2.0))
          .text_size(text::body2())
          .text_color(color)
          .truncate()
          .when(self.error.is_some(), |row| row.role(Role::Alert))
          .child(message.clone())
          .when(self.error.is_some() || self.missing, |row| {
            let message = message.clone();
            row.tooltip(move |window, cx| Tooltip::new(message.clone()).max_w(px(400.0)).build(window, cx))
          })
          .test_text(message),
      )
  }
}
