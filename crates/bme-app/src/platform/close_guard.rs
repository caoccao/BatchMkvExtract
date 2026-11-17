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

use gpui_kit::component::WindowExt;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::{App, ParentElement, Window};

use crate::ui::{confirm_button, text_button};

/// Decides a close request. Without active extractions the window closes.
/// With them, a dialog asks and the request is refused for now: No keeps the
/// window, Yes runs `on_confirm` (which cancels the extractions), then closes
/// it. Enter and Escape answer No, so a stray key never stops the
/// extractions; gpui-kit's own OK button would take Enter.
pub fn should_close(
  has_active_extracts: bool,
  window: &mut Window,
  cx: &mut App,
  on_confirm: impl Fn(&mut Window, &mut App) + 'static,
) -> bool {
  if !has_active_extracts {
    return true;
  }
  if !window.has_active_dialog(cx) {
    let on_confirm = std::rc::Rc::new(on_confirm);
    window.open_alert_dialog(cx, move |dialog, _, _| {
      let on_confirm = on_confirm.clone();
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
                  on_confirm(window, cx);
                  window.remove_window();
                }),
            ),
        )
    });
  }
  false
}
