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

//! Word keys stop inside numbers in every text input of the app:
//! Ctrl+Left/Right on Windows and Linux, Option+Left/Right on macOS, with
//! Shift, and the word deletions. The stops are
//! [`crate::domain::word_navigation`]'s.
//!
//! A keystroke interceptor sees the key before gpui-kit's input does: it asks
//! the keymap which action the key would run, so the platform's own word
//! bindings decide (Linux also selects by word with Alt+Shift+Arrow), and
//! acts only on a word action in a focused `Input` or `Textarea`, including
//! gpui-kit's own (the settings search, a select's search) and those in
//! dialogs. Where a separator decides the stop it sets the selection and
//! consumes the key; everything else runs gpui-kit's own handler. A deletion
//! selects the range to delete and lets gpui-kit's word deletion remove it,
//! so the edit keeps gpui-kit's undo step (undo brings the part back
//! selected). A field that holds a path sits in an element with the key
//! context [`PATH_CONTEXT`], and its words also split at a `.` between two
//! letters or digits.

use gpui_kit::component::WindowExt;
use gpui_kit::component::input::{
  AnyInputState, DeleteToNextWordEnd, DeleteToPreviousWordStart, InputState, MoveToNextWord, MoveToPreviousWord,
  SelectToNextWordEnd, SelectToPreviousWordStart, TextareaState,
};
use gpui_kit::*;

use crate::domain::word_navigation::{Direction, Selection, Separators, WordCommand, word_command};

/// gpui-kit's key context of its text inputs.
const INPUT_CONTEXT: &str = "Input";

/// The key context of an element around a field that holds a path.
pub const PATH_CONTEXT: &str = "PathField";

/// Installs the interceptor for the whole application.
pub fn install(cx: &mut App) {
  cx.intercept_keystrokes(|event, window, cx| {
    if on_keystroke(event, window, cx) {
      cx.stop_propagation();
    }
  })
  .detach();
}

/// `true` when the key was handled here.
fn on_keystroke(event: &KeystrokeEvent, window: &mut Window, cx: &mut App) -> bool {
  let Some((direction, command)) = word_key(event, cx) else {
    return false;
  };
  let separators = separators(event);
  match window.focused_input(cx) {
    Some(AnyInputState::Input(state)) => redirect(&state, direction, command, separators, window, cx),
    Some(AnyInputState::Textarea(state)) => redirect(&state, direction, command, separators, window, cx),
    _ => false,
  }
}

/// Where the focused field's words split: in a path also at its dots.
fn separators(event: &KeystrokeEvent) -> Separators {
  if event.context_stack.iter().any(|context| context.contains(PATH_CONTEXT)) {
    Separators::Paths
  } else {
    Separators::Numbers
  }
}

/// The word action the keymap runs for the key in a text input, if any.
fn word_key(event: &KeystrokeEvent, cx: &App) -> Option<(Direction, WordCommand)> {
  if !event
    .context_stack
    .iter()
    .any(|context| context.contains(INPUT_CONTEXT))
  {
    return None;
  }
  let (bindings, _) = cx
    .key_bindings()
    .borrow()
    .bindings_for_input(std::slice::from_ref(&event.keystroke), &event.context_stack);
  let action = bindings.first()?.action().as_any();
  let key = if action.is::<MoveToPreviousWord>() {
    (Direction::Left, WordCommand::Move)
  } else if action.is::<MoveToNextWord>() {
    (Direction::Right, WordCommand::Move)
  } else if action.is::<SelectToPreviousWordStart>() {
    (Direction::Left, WordCommand::Select)
  } else if action.is::<SelectToNextWordEnd>() {
    (Direction::Right, WordCommand::Select)
  } else if action.is::<DeleteToPreviousWordStart>() {
    (Direction::Left, WordCommand::Delete)
  } else if action.is::<DeleteToNextWordEnd>() {
    (Direction::Right, WordCommand::Delete)
  } else {
    return None;
  };
  Some(key)
}

/// Sets the selection a separator decides; `true` when the key is consumed.
fn redirect<T: TextField>(
  state: &Entity<T>,
  direction: Direction,
  command: WordCommand,
  separators: Separators,
  window: &mut Window,
  cx: &mut App,
) -> bool {
  // A composition in progress keeps its keys.
  if state
    .update(cx, |state, cx| state.marked_text_range(window, cx))
    .is_some()
  {
    return false;
  }
  let Some((value, selection)) = state.read(cx).snapshot(command) else {
    return false;
  };
  let Some(next) = word_command(&value, &selection, direction, command, separators) else {
    return false;
  };
  state.update(cx, |state, cx| state.select(&next, cx));
  // gpui-kit's own word deletion removes what is now selected.
  command != WordCommand::Delete
}

/// The input states a word key can land in.
trait TextField: EntityInputHandler {
  /// The text and selection, or `None` when the stops do not apply.
  fn snapshot(&self, command: WordCommand) -> Option<(SharedString, Selection)>;

  fn select(&mut self, selection: &Selection, cx: &mut Context<Self>);
}

macro_rules! text_field {
  ($($state:ty),*) => {$(
    impl TextField for $state {
      fn snapshot(&self, command: WordCommand) -> Option<(SharedString, Selection)> {
        // gpui-kit makes a masked value one word; a read-only one deletes nothing.
        if self.presentation().is_masked() || (command == WordCommand::Delete && !self.is_editable()) {
          return None;
        }
        let selection = Selection {
          range: self.selected_range(),
          reversed: self.is_selection_reversed(),
        };
        Some((self.value(), selection))
      }

      fn select(&mut self, selection: &Selection, cx: &mut Context<Self>) {
        self.set_selected_range_with_direction(selection.range.clone(), selection.reversed, cx);
      }
    }
  )*};
}

text_field!(InputState, TextareaState);

#[cfg(test)]
mod tests {
  use std::ops::Range;

  use core::prelude::v1::test;
  use gpui_kit::component::input::{Input, MoveToEnd, Textarea};
  use gpui_kit::component::v_flex;
  use gpui_kit::test::TestWindowExt;

  use super::*;
  use crate::domain::word_navigation::stock_word_jump;

  /// The word modifier: Option on macOS, Ctrl on Windows and Linux.
  const WORD: &str = if cfg!(target_os = "macos") { "alt" } else { "ctrl" };
  const UNDO: &str = if cfg!(target_os = "macos") { "cmd-z" } else { "ctrl-z" };

  fn key(name: &str) -> String {
    format!("{WORD}-{name}")
  }

  struct Fields {
    input: Entity<InputState>,
    area: Entity<TextareaState>,
    masked: Entity<InputState>,
    path: Entity<InputState>,
  }

  impl Render for Fields {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      v_flex()
        .size_full()
        .child(Input::new(&self.input))
        .child(Textarea::new(&self.area))
        .child(Input::new(&self.masked))
        .child(div().key_context(PATH_CONTEXT).child(Input::new(&self.path)))
    }
  }

  /// Which field a test types into.
  #[derive(Clone, Copy)]
  enum Field {
    Input,
    Area,
    Masked,
    Path,
  }

  struct Harness {
    handle: AnyWindowHandle,
    fields: Entity<Fields>,
  }

  impl Harness {
    /// A window with the fields; `numbers` installs the interceptor, without
    /// it the keys run gpui-kit's own handlers.
    fn open(cx: &mut TestAppContext, numbers: bool) -> Self {
      let (handle, fields) = cx.update(|cx| {
        gpui_kit::init(cx);
        if numbers {
          install(cx);
        }
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
          let input = cx.new(|cx| InputState::new(window, cx));
          let area = cx.new(|cx| TextareaState::new(window, cx));
          let masked = cx.new(|cx| InputState::new(window, cx).masked(true));
          let path = cx.new(|cx| InputState::new(window, cx));
          cx.new(|_| Fields {
            input,
            area,
            masked,
            path,
          })
        })
        .expect("open the window")
      });
      cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
      })
      .unwrap();
      Self { handle, fields }
    }

    fn window<R>(&self, cx: &mut TestAppContext, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
      cx.update_window(self.handle, |_, window, cx| f(window, cx)).unwrap()
    }

    /// Focuses `field`, types `text` and puts the caret at `caret`.
    fn type_in(&self, cx: &mut TestAppContext, field: Field, text: &str, caret: usize) {
      let fields = self.fields.clone();
      self.window(cx, |window, cx| {
        let fields = fields.read(cx);
        match field {
          Field::Input => fields.input.clone().update(cx, |state, cx| state.focus(window, cx)),
          Field::Area => fields.area.clone().update(cx, |state, cx| state.focus(window, cx)),
          Field::Masked => fields.masked.clone().update(cx, |state, cx| state.focus(window, cx)),
          Field::Path => fields.path.clone().update(cx, |state, cx| state.focus(window, cx)),
        }
      });
      cx.run_until_parked();
      self.window(cx, |window, cx| {
        window.render_frame(cx);
        window.input(text, cx);
      });
      cx.run_until_parked();
      self.select(cx, field, caret..caret, false);
    }

    fn select(&self, cx: &mut TestAppContext, field: Field, range: Range<usize>, reversed: bool) {
      let fields = self.fields.clone();
      self.window(cx, |window, cx| {
        let fields = fields.read(cx);
        match field {
          Field::Input | Field::Masked | Field::Path => {
            let state = match field {
              Field::Input => &fields.input,
              Field::Masked => &fields.masked,
              _ => &fields.path,
            };
            state.clone().update(cx, |state, cx| {
              state.set_selected_range_with_direction(range, reversed, cx)
            });
          }
          Field::Area => {
            fields.area.clone().update(cx, |state, cx| {
              state.set_selected_range_with_direction(range, reversed, cx)
            });
          }
        }
        window.render_frame(cx);
      });
    }

    /// The focused field's text and selection, as gpui-kit reports them.
    fn state(&self, cx: &mut TestAppContext) -> (String, Selection) {
      self.window(cx, |window, cx| match window.focused_input(cx) {
        Some(AnyInputState::Input(state)) => {
          let state = state.read(cx);
          (
            state.value().to_string(),
            snapshot(state.selected_range(), state.is_selection_reversed()),
          )
        }
        Some(AnyInputState::Textarea(state)) => {
          let state = state.read(cx);
          (
            state.value().to_string(),
            snapshot(state.selected_range(), state.is_selection_reversed()),
          )
        }
        _ => panic!("no focused field"),
      })
    }

    fn press(&self, cx: &mut TestAppContext, name: &str) -> (String, Selection) {
      self.window(cx, |window, cx| {
        window.press(name, cx);
        window.render_frame(cx);
      });
      cx.run_until_parked();
      self.state(cx)
    }

    /// The caret after each of `count` presses of `name`, as `a` or `a..b`
    /// (`r` when reversed).
    fn presses(&self, cx: &mut TestAppContext, name: &str, count: usize) -> Vec<String> {
      (0..count)
        .map(|_| {
          let (_, selection) = self.press(cx, name);
          let Selection { range, reversed } = selection;
          if range.is_empty() {
            range.start.to_string()
          } else {
            format!("{}..{}{}", range.start, range.end, if reversed { "r" } else { "" })
          }
        })
        .collect()
    }

    /// The text after each of `count` presses of `name`.
    fn edits(&self, cx: &mut TestAppContext, name: &str, count: usize) -> Vec<String> {
      (0..count).map(|_| self.press(cx, name).0).collect()
    }
  }

  fn snapshot(range: Range<usize>, reversed: bool) -> Selection {
    Selection { range, reversed }
  }

  #[gpui_kit::test]
  fn the_parts_of_a_typed_title_are_words(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    harness.type_in(cx, Field::Input, "Movie.2024.1080p", 16);
    assert_eq!(harness.presses(cx, &key("left"), 5), ["11", "6", "5", "0", "0"]);
    assert_eq!(harness.presses(cx, &key("right"), 5), ["5", "6", "10", "16", "16"]);
    assert_eq!(
      harness.presses(cx, &key("shift-left"), 5),
      ["11..16r", "6..16r", "5..16r", "0..16r", "0..16r"]
    );
    harness.select(cx, Field::Input, 0..0, false);
    assert_eq!(
      harness.presses(cx, &key("shift-right"), 5),
      ["0..5", "0..6", "0..10", "0..16", "0..16"]
    );
    // Shift back over the anchor turns the selection round.
    harness.select(cx, Field::Input, 6..10, false);
    assert_eq!(harness.presses(cx, &key("shift-left"), 2), ["6", "5..6r"]);
  }

  #[gpui_kit::test]
  fn a_path_stops_at_every_dot_of_a_file_name(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    let path = "C:/Videos/Movie.2024.1080p.mkv";
    harness.type_in(cx, Field::Path, path, path.len());
    assert_eq!(harness.presses(cx, &key("left"), 6), ["27", "21", "16", "10", "9", "3"]);
    harness.select(cx, Field::Path, 10..10, false);
    assert_eq!(harness.presses(cx, &key("right"), 5), ["15", "20", "26", "30", "30"]);
    assert_eq!(harness.presses(cx, &key("shift-left"), 2), ["27..30r", "21..30r"]);
    harness.select(cx, Field::Path, 30..30, false);
    assert_eq!(
      harness.edits(cx, &key("backspace"), 3),
      [
        "C:/Videos/Movie.2024.1080p.",
        "C:/Videos/Movie.2024.1080p",
        "C:/Videos/Movie.2024."
      ]
    );
    // The same text outside a path field keeps `1080p.mkv` one word.
    harness.type_in(cx, Field::Input, path, path.len());
    assert_eq!(harness.presses(cx, &key("left"), 2), ["16", "15"]);
  }

  #[gpui_kit::test]
  fn text_without_numbers_moves_as_gpui_kit_does(cx: &mut TestAppContext) {
    let texts = ["hello world", "can't stop.now", "abc.def  ghi", "你好 世界"];
    let mut runs = Vec::new();
    for numbers in [false, true] {
      let harness = Harness::open(cx, numbers);
      let mut run = Vec::new();
      for text in texts {
        harness.type_in(cx, Field::Input, text, text.len());
        run.push(harness.presses(cx, &key("left"), 5));
        run.push(harness.presses(cx, &key("right"), 5));
        run.push(harness.presses(cx, &key("shift-left"), 5));
        harness.select(cx, Field::Input, 0..text.len(), false);
        harness.press(cx, "backspace");
      }
      runs.push(run);
    }
    assert_eq!(runs[0], runs[1]);
  }

  #[gpui_kit::test]
  fn the_copy_of_gpui_kits_jump_matches_it(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, false);
    for text in [
      "Movie.2024.1080p",
      "abc 123.456 def",
      "1,234.567 x",
      "字幕 １２３.４５６",
    ] {
      harness.type_in(cx, Field::Input, text, 0);
      for (direction, name) in [(Direction::Right, "right"), (Direction::Left, "left")] {
        for _ in 0..6 {
          let caret = harness.state(cx).1.caret();
          let (_, after) = harness.press(cx, &key(name));
          assert_eq!(
            after.caret(),
            stock_word_jump(text, caret, direction),
            "{text:?} {caret} {name}"
          );
        }
      }
      harness.select(cx, Field::Input, 0..text.len(), false);
      harness.press(cx, "backspace");
    }
  }

  #[gpui_kit::test]
  fn word_deletion_removes_one_part(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    harness.type_in(cx, Field::Input, "Movie.2024.1080p", 16);
    assert_eq!(
      harness.edits(cx, &key("backspace"), 5),
      ["Movie.2024.", "Movie.2024", "Movie.", "Movie", ""]
    );
    harness.type_in(cx, Field::Input, "123.456 def", 0);
    assert_eq!(
      harness.edits(cx, &key("delete"), 4),
      [".456 def", "456 def", " def", ""]
    );
    // One undo step brings the part back, selected.
    harness.type_in(cx, Field::Input, "1,234.567", 9);
    assert_eq!(harness.press(cx, &key("backspace")).0, "1,234.");
    let (text, selection) = harness.press(cx, UNDO);
    assert_eq!(text, "1,234.567");
    assert_eq!(selection.range, 6..9);
    // Selected text deletes itself.
    harness.select(cx, Field::Input, 0..2, false);
    assert_eq!(harness.press(cx, &key("backspace")).0, "234.567");
  }

  #[gpui_kit::test]
  fn a_textarea_passes_line_breaks(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    harness.type_in(cx, Field::Area, "abc\n123.456", 3);
    assert_eq!(harness.presses(cx, &key("right"), 3), ["7", "11", "11"]);
    assert_eq!(harness.presses(cx, &key("left"), 4), ["8", "4", "0", "0"]);
  }

  #[gpui_kit::test]
  fn a_masked_value_stays_one_word(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    harness.type_in(cx, Field::Masked, "123.456", 0);
    assert_eq!(harness.presses(cx, &key("right"), 1), ["7"]);
  }

  #[gpui_kit::test]
  fn a_key_bound_to_another_action_is_left_alone(cx: &mut TestAppContext) {
    let harness = Harness::open(cx, true);
    // A later binding in the same context wins, as a user binding would.
    let right = if cfg!(target_os = "macos") {
      "alt-right"
    } else {
      "ctrl-right"
    };
    cx.update(|cx| cx.bind_keys([KeyBinding::new(right, MoveToEnd, Some(INPUT_CONTEXT))]));
    harness.type_in(cx, Field::Input, "123.456 def", 0);
    assert_eq!(harness.presses(cx, &key("right"), 1), ["11"]);
  }
}
