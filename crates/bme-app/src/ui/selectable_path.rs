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

//! A path shown as text that can be selected and copied but not edited: a
//! read-only gpui-base `Textarea` of one line without wrapping, whose context
//! menu has Copy and Select All. The file cards and the queue rows show their
//! paths with it.

use std::rc::Rc;

use gpui_kit::base::input::{Copy, InputAccessibility, InputEditorStyle, SelectAll, Textarea, TextareaState};
use gpui_kit::base::{ObservedElement, ObservedTextElement, TestSupportExt as _, TestTextExt as _};
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::native_menu::NativeMenu;
use gpui_kit::*;

/// The state of a field showing `path`.
pub fn state(path: impl Into<SharedString>, window: &mut Window, cx: &mut App) -> Entity<TextareaState> {
  let path = path.into();
  cx.new(|cx| {
    let mut state = TextareaState::new(window, cx)
      .rows(1)
      .soft_wrap(false)
      .default_value(path);
    state.set_readonly(true, cx);
    state.on_context_menu(Rc::new(|_, capabilities, position, window, cx| {
      NativeMenu::new()
        .menu_with_disabled(
          tr!("gpui_component.Input.Copy"),
          !capabilities.is_copyable(),
          Box::new(Copy),
        )
        .menu(tr!("gpui_component.Input.Select All"), Box::new(SelectAll))
        .show(position, window, cx);
    }));
    state
  })
}

/// The field `id` drawing `state`, which shows `path`, named `label` for
/// screen readers. It takes the width its parent gives it, one line of
/// `body2` text high.
pub fn element(
  id: impl Into<ElementId>,
  state: &Entity<TextareaState>,
  label: SharedString,
  path: SharedString,
  cx: &mut App,
) -> ObservedTextElement<ObservedElement<Stateful<Div>>> {
  let selection = crate::theme::selection_highlight(cx.theme());
  state.update(cx, |state, _| {
    state.set_editor_style(InputEditorStyle {
      selection,
      ..Default::default()
    });
    state.set_accessibility(InputAccessibility::new(Some(Role::MultilineTextInput)).with_label(Some(label)));
  });
  div()
    .id(id)
    .test_support()
    .flex_1()
    .min_w_0()
    .ml_0()
    .pl_0()
    .h(super::text::body2() * 1.25)
    .line_height(relative(1.25))
    .child(Textarea::new(state))
    .test_text(path)
}
