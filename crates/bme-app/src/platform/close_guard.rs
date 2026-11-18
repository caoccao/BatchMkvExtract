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

//! The exit confirmation: closing the main window while an extraction is
//! waiting or running asks first.

use std::rc::Rc;

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::{App, ParentElement, Task, Window};

use crate::ui::{confirm_button, text_button};

/// Decides a close request. Without active extractions the window closes.
/// With them, a dialog asks and the request is refused for now: No keeps the
/// window; Yes runs `on_confirm`, which cancels the extractions and resolves
/// to whether they all stopped, and the window closes once they did. When
/// one did not stop in time, a second dialog offers to wait (`on_wait`; the
/// window stays) or to quit anyway. Enter and Escape answer No and Wait, so
/// a stray key never stops the extractions or leaves one running; gpui-kit's
/// own OK button would take Enter.
pub fn should_close(
  has_active_extracts: bool,
  window: &mut Window,
  cx: &mut App,
  on_confirm: impl Fn(&mut Window, &mut App) -> Task<bool> + 'static,
  on_wait: impl Fn(&mut App) + 'static,
) -> bool {
  if !has_active_extracts {
    return true;
  }
  if !window.has_active_dialog(cx) {
    let on_confirm = Rc::new(on_confirm);
    let on_wait: Rc<dyn Fn(&mut App)> = Rc::new(on_wait);
    window.open_alert_dialog(cx, move |dialog, _, _| {
      let (on_confirm, on_wait) = (on_confirm.clone(), on_wait.clone());
      dialog
        .title(tr!("confirm.exitTitle"))
        .description(tr!("confirm.exitMessage"))
        // Enter (OK) and Escape (Cancel) only close the dialog.
        .on_ok(|_, _, _| true)
        .on_cancel(|_, _, _| true)
        .footer(
          DialogFooter::new()
            .child(
              text_button("exit-no")
                .outline()
                .label(tr!("confirm.no"))
                .on_click(|_, window, cx| window.close_dialog(cx)),
            )
            .child(
              confirm_button("exit-yes")
                .danger()
                .label(tr!("confirm.yes"))
                .on_click(move |_, window, cx| {
                  window.close_dialog(cx);
                  let stopped = on_confirm(window, cx);
                  let handle = window.window_handle();
                  let on_wait = on_wait.clone();
                  cx.spawn(async move |cx| {
                    let stopped = stopped.await;
                    let _ = handle.update(cx, |_, window, cx| {
                      if stopped {
                        window.remove_window();
                      } else {
                        still_running(window, cx, on_wait);
                      }
                    });
                  })
                  .detach();
                }),
            ),
        )
    });
  }
  false
}

/// An extraction did not stop after Yes: wait for it (the window stays), or
/// quit and leave its `mkvextract` running.
fn still_running(window: &mut Window, cx: &mut App, on_wait: Rc<dyn Fn(&mut App)>) {
  window.open_alert_dialog(cx, move |dialog, _, _| {
    let (enter, escape, wait) = (on_wait.clone(), on_wait.clone(), on_wait.clone());
    dialog
      .title(tr!("confirm.stillRunningTitle"))
      .description(tr!("confirm.stillRunningMessage"))
      // Enter (OK) and Escape (Cancel) wait.
      .on_ok(move |_, _, cx| {
        enter(cx);
        true
      })
      .on_cancel(move |_, _, cx| {
        escape(cx);
        true
      })
      .footer(
        DialogFooter::new()
          .child(
            text_button("exit-wait")
              .outline()
              .label(tr!("confirm.wait"))
              .on_click(move |_, window, cx| {
                wait(cx);
                window.close_dialog(cx);
              }),
          )
          .child(
            confirm_button("exit-quit-anyway")
              .danger()
              .label(tr!("confirm.quitAnyway"))
              .on_click(|_, window, _| window.remove_window()),
          ),
      )
  });
}
