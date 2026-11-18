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

//! Ctrl+Arrow (Windows, Linux) / Option+Arrow (macOS) stops inside numbers:
//! `123.456` and `1,234.567` are one word to the OS, but users expect each
//! numeric part to be a word. In a path it also stops at every `.` between
//! two letters or digits: `movie.mkv` is one word to the OS, but the name and
//! the extension are words of their own ([`Separators`]).
//!
//! The per-platform rule ([`numeric_word_boundary`], copied with its test
//! table from BatchMkvMerge) adds stops to word jumps that differ per
//! platform: Windows jumps forward to the next word's start, macOS to the
//! word's end. gpui-kit's inputs jump the same way on every platform
//! ([`stock_word_jump`]): forward to the end of the next word, backward to the
//! start of the previous one, over any whitespace, line breaks included. The
//! app therefore uses one rule, [`word_stop`]: the per-platform rule's macOS
//! variant, skipping line breaks as gpui-kit does, and stepping over a
//! separator, which gpui-kit would give a stop of its own when a jump starts
//! beside it. Word deletion uses the same stops ([`word_command`]).
//!
//! Offsets are UTF-8 byte offsets into the text (GPUI's input offsets). Words
//! are UAX #29 word segments.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

/// The arrow key pressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
  Left,
  Right,
}

/// The `.` and `,` that split a word into parts, each a word of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separators {
  /// A `.` or `,` between two digits: the parts of a number.
  Numbers,
  /// Those, and a `.` between two letters or digits, as in the file names of
  /// a path: `Movie.2024.mkv` has the words `Movie`, `2024` and `mkv`.
  Paths,
}

/// How the input's own word jumps run, which the extra stops are fitted to.
#[derive(Clone, Copy)]
struct Jumps {
  /// The space a jump passes before it reaches a word.
  skips: fn(char) -> bool,
  /// Whether a forward jump passes that space too (backward ones always do).
  skips_forward: bool,
  /// Whether a forward jump ends at the end of a number's part (before its
  /// separator) rather than at the start of the next part (after it).
  forward_ends_parts: bool,
}

impl Jumps {
  /// The per-platform rule's: spaces but not line breaks, forward only on
  /// macOS, where a jump ends at a word's end; on Windows it lands on the next
  /// word's start.
  fn per_platform(is_mac: bool) -> Self {
    Self {
      skips: is_horizontal_space,
      skips_forward: is_mac,
      forward_ends_parts: is_mac,
    }
  }

  /// gpui-kit's, the same on every platform: all whitespace, both ways, and
  /// forward jumps end at a word's end.
  const GPUI_KIT: Self = Self {
    skips: char::is_whitespace,
    skips_forward: true,
    forward_ends_parts: true,
  };
}

/// Whitespace other than CR and LF.
fn is_horizontal_space(c: char) -> bool {
  c.is_whitespace() && c != '\r' && c != '\n'
}

/// The char at byte offset `index`, if `index` is a char boundary inside the
/// text.
fn char_at(value: &str, index: usize) -> Option<char> {
  value.get(index..).and_then(|rest| rest.chars().next())
}

/// The word segment containing byte offset `index`, with its start.
fn segment_containing(value: &str, index: usize) -> Option<(usize, &str)> {
  value
    .split_word_bound_indices()
    .find(|(start, segment)| (*start..start + segment.len()).contains(&index))
}

/// Whether byte offset `index` holds a `.` or `,` that `separators` splits a
/// word at.
fn is_separator(value: &str, index: usize, separators: Separators) -> bool {
  // Both are ASCII, so the byte test never matches inside another char.
  let Some(&separator @ (b'.' | b',')) = value.as_bytes().get(index) else {
    return false;
  };
  let before = value[..index].chars().next_back();
  let after = value[index + 1..].chars().next();
  let between = |test: fn(char) -> bool| before.is_some_and(test) && after.is_some_and(test);
  between(char::is_numeric) || (separators == Separators::Paths && separator == b'.' && between(char::is_alphanumeric))
}

/// The extra stop inside a word for a word jump from `caret`, or `None` to
/// let the input's own word navigation run.
///
/// The jump passes the space it would pass anyway first, and only a stop at a
/// separator counts: backward it lands on the next part's start; forward on
/// the part's end or the next part's start, as the jumps do. Another `.` or
/// `,` in the same word must not be skipped to reach such a stop.
fn stop_in_word(
  value: &str,
  caret: usize,
  direction: Direction,
  jumps: Jumps,
  separators: Separators,
) -> Option<usize> {
  let left = direction == Direction::Left;
  // `index` walks chars: the one before the caret going left, at it going right.
  let mut index = if left {
    value.get(..caret)?.char_indices().next_back()?.0
  } else {
    caret
  };
  if left || jumps.skips_forward {
    while let Some(c) = char_at(value, index).filter(|c| (jumps.skips)(*c)) {
      if left {
        index = value[..index].char_indices().next_back()?.0;
      } else {
        index += c.len_utf8();
      }
    }
  }
  let (word_start, word) = segment_containing(value, index)?;
  if !word.chars().any(char::is_alphanumeric) {
    return None;
  }

  let mut nearest: Option<usize> = None;
  let mut splits = false;
  for (offset, _) in word.match_indices(['.', ',']) {
    let before = word_start + offset;
    let separator = is_separator(value, before, separators);
    let boundaries: &[usize] = if separator {
      &[before + usize::from(left || !jumps.forward_ends_parts)]
    } else {
      &[before, before + 1]
    };
    for &boundary in boundaries {
      let closer = if left {
        boundary < caret && nearest.is_none_or(|nearest| boundary > nearest)
      } else {
        boundary > caret && nearest.is_none_or(|nearest| boundary < nearest)
      };
      if closer {
        nearest = Some(boundary);
        splits = separator;
      }
    }
  }
  if splits { nearest } else { None }
}

/// The per-platform rule: the extra stop inside a number for a word jump from
/// `caret` that differs per platform, or `None` to let the input's own
/// navigation run.
///
/// Backward navigation skips spaces first (macOS forward does too), line
/// breaks are left alone, and only a stop at a `.` or `,` between two digits
/// counts: backward it lands on the next part's start; forward it lands
/// after the separator on Windows and before it on macOS. Other punctuation in
/// the same word must not be skipped to reach such a stop.
pub fn numeric_word_boundary(value: &str, caret: usize, direction: Direction, is_mac: bool) -> Option<usize> {
  stop_in_word(
    value,
    caret,
    direction,
    Jumps::per_platform(is_mac),
    Separators::Numbers,
  )
}

/// Where gpui-kit's own word jump from `offset` lands: forward at the end of
/// the next word, backward at the start of the previous one, passing any
/// whitespace. Each side of the caret is segmented on its own, so a jump
/// starting beside punctuation stops on its other side. A copy of
/// `previous_start_of_word_at` / `next_end_of_word_at` of gpui-base 0.9.1 for
/// unmasked text without inline tokens.
pub fn stock_word_jump(value: &str, offset: usize, direction: Direction) -> usize {
  let blank = |segment: &str| segment.trim_start().is_empty();
  match direction {
    Direction::Left => value[..offset]
      .split_word_bound_indices()
      .rfind(|(_, segment)| !blank(segment))
      .map_or(0, |(start, _)| start),
    Direction::Right => value[offset..]
      .split_word_bound_indices()
      .find(|(_, segment)| !blank(segment))
      .map_or(value.len(), |(start, segment)| offset + start + segment.len()),
  }
}

/// Where a word jump from `caret` stops in a gpui-kit input when a separator
/// decides it, or `None` when the input's own jump is right.
///
/// Each part between `separators` is a word: forward the jump ends at a
/// part's end, backward at a part's start, and the separator gets no stop of
/// its own.
pub fn word_stop(value: &str, caret: usize, direction: Direction, separators: Separators) -> Option<usize> {
  let stop = |caret| stop_in_word(value, caret, direction, Jumps::GPUI_KIT, separators);
  if let Some(stop) = stop(caret) {
    return Some(stop);
  }
  // At a part's edge the jump goes on from the far side of the separator.
  let (separator, past) = match direction {
    Direction::Left => (caret.checked_sub(1)?, caret - 1),
    Direction::Right => (caret, caret + 1),
  };
  if !is_separator(value, separator, separators) {
    return None;
  }
  Some(stop(past).unwrap_or_else(|| stock_word_jump(value, past, direction)))
}

/// A text selection: `range` in byte offsets, `reversed` when the caret is at
/// its start (a backward selection).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
  pub range: Range<usize>,
  pub reversed: bool,
}

impl Selection {
  /// The end the caret is at.
  pub fn caret(&self) -> usize {
    if self.reversed {
      self.range.start
    } else {
      self.range.end
    }
  }

  /// The end that stays when the selection is extended.
  fn anchor(&self) -> usize {
    if self.reversed {
      self.range.end
    } else {
      self.range.start
    }
  }
}

/// What a word key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordCommand {
  /// Ctrl/Option+Arrow.
  Move,
  /// With Shift.
  Select,
  /// Ctrl/Option+Backspace (left) and +Delete (right).
  Delete,
}

/// The selection after a word key that [`word_stop`] redirects, or `None`
/// when the input's own handler is right.
///
/// The jump starts at the caret, as gpui-kit's does. A move collapses at the
/// stop; a selection extends from the anchor to it. A deletion returns the
/// range between the caret and the stop, selected, for the input's own word
/// deletion to remove; a deletion with text selected removes that text, so
/// it is left to the input.
pub fn word_command(
  value: &str,
  selection: &Selection,
  direction: Direction,
  command: WordCommand,
  separators: Separators,
) -> Option<Selection> {
  if command == WordCommand::Delete && !selection.range.is_empty() {
    return None;
  }
  let caret = selection.caret();
  let stop = word_stop(value, caret, direction, separators)?;
  let selection = match command {
    WordCommand::Move => Selection {
      range: stop..stop,
      reversed: false,
    },
    WordCommand::Select => {
      let anchor = selection.anchor();
      Selection {
        range: anchor.min(stop)..anchor.max(stop),
        reversed: stop < anchor,
      }
    }
    WordCommand::Delete => Selection {
      range: caret.min(stop)..caret.max(stop),
      reversed: false,
    },
  };
  Some(selection)
}

#[cfg(test)]
mod tests {
  use super::*;

  // The per-platform rule's test table from before 1.0.0, one case for one, in
  // byte offsets.

  fn nav(value: &str, caret: usize, direction: &str, is_mac: bool) -> Option<usize> {
    let direction = if direction == "left" {
      Direction::Left
    } else {
      Direction::Right
    };
    numeric_word_boundary(value, caret, direction, is_mac)
  }

  #[test]
  fn windows_splits_a_number_into_two_words() {
    for separator in [".", ","] {
      let value = format!("123{separator}456");
      assert_eq!(nav(&value, 0, "right", false), Some(4));
      assert_eq!(nav(&value, 7, "left", false), Some(4));
      // Once in the last/first part, native navigation finishes the movement.
      assert_eq!(nav(&value, 4, "right", false), None);
      assert_eq!(nav(&value, 4, "left", false), None);
    }
  }

  #[test]
  fn macos_splits_a_number_into_two_words() {
    for separator in [".", ","] {
      let value = format!("123{separator}456");
      assert_eq!(nav(&value, 0, "right", true), Some(3));
      assert_eq!(nav(&value, 7, "left", true), Some(4));
      assert_eq!(nav(&value, 3, "right", true), None);
      assert_eq!(nav(&value, 4, "left", true), None);
    }
  }

  #[test]
  fn navigation_works_inside_and_alongside_the_separator() {
    for separator in [".", ","] {
      let value = format!("123{separator}456");
      assert_eq!(nav(&value, 1, "right", false), Some(4));
      assert_eq!(nav(&value, 3, "right", false), Some(4));
      assert_eq!(nav(&value, 6, "left", false), Some(4));
      assert_eq!(nav(&value, 3, "left", false), None);
      assert_eq!(nav(&value, 2, "right", true), Some(3));
    }
  }

  #[test]
  fn repeated_and_mixed_numeric_separators_each_add_a_stop() {
    let value = "1,234.567";
    assert_eq!(nav(value, 0, "right", false), Some(2));
    assert_eq!(nav(value, 2, "right", false), Some(6));
    assert_eq!(nav(value, 0, "right", true), Some(1));
    assert_eq!(nav(value, 1, "right", true), Some(5));
    assert_eq!(nav(value, 9, "left", false), Some(6));
    assert_eq!(nav(value, 6, "left", false), Some(2));
  }

  #[test]
  fn spaces_retain_native_platform_behavior_and_newlines_remain_native() {
    assert_eq!(nav(" 123.456", 0, "right", false), None);
    assert_eq!(nav(" 123.456", 0, "right", true), Some(4));
    assert_eq!(nav("123.456  ", 9, "left", false), Some(4));
    assert_eq!(nav("123.456\n", 8, "left", false), None);
    assert_eq!(nav("\n123.456", 0, "right", true), None);
  }

  #[test]
  fn ordinary_text_and_punctuation_continue_to_use_native_navigation() {
    for value in ["", "123456", "abc.def", "can't", "你好世界", "123..456"] {
      let carets = value.char_indices().map(|(byte, _)| byte).chain([value.len()]);
      for caret in carets {
        for direction in ["left", "right"] {
          for is_mac in [false, true] {
            assert_eq!(
              nav(value, caret, direction, is_mac),
              None,
              "{value:?} {caret} {direction} {is_mac}"
            );
          }
        }
      }
    }
    // Do not skip another word or punctuation to reach a numeric separator.
    assert_eq!(nav("hello 123.456", 0, "right", false), None);
    assert_eq!(nav("abc.def123.456", 0, "right", false), None);
    assert_eq!(nav("abc.def123.456", 4, "right", false), Some(11));
    assert_eq!(nav("123.456.abc", 11, "left", false), None);
  }

  #[test]
  fn unicode_numbers_are_supported() {
    // Full-width digits and CJK take 3 bytes, the emoji and the mathematical
    // digits 4.
    assert_eq!(nav("字幕 １２３.４５６", 7, "right", false), Some(17));
    assert_eq!(nav("字幕 １２３.４５６", 26, "left", true), Some(17));
    assert_eq!(nav("🎬 123.456", 5, "right", false), Some(9));
    assert_eq!(nav("𝟙𝟚.𝟛𝟜", 17, "left", false), Some(9));
  }

  // Beyond that table: multi-byte text, and carets at and past the edges.

  #[test]
  fn works_on_byte_offsets() {
    let value = "字幕 １２３.４５６";
    let dot = value.find('.').unwrap();
    assert_eq!(numeric_word_boundary(value, 7, Direction::Right, false), Some(dot + 1));
    assert_eq!(
      numeric_word_boundary(value, value.len(), Direction::Left, false),
      Some(dot + 1)
    );
    assert_eq!(
      numeric_word_boundary(value, value.len() + 1, Direction::Left, false),
      None
    );
    assert_eq!(numeric_word_boundary("123.456", 0, Direction::Left, false), None);
  }

  // The gpui-kit rule of the app.

  /// The offsets a key pressed again and again stops at: `word_stop` where a
  /// separator decides, otherwise gpui-kit's own jump (`separators` `None`:
  /// always).
  fn walk(value: &str, start: usize, direction: Direction, separators: Option<Separators>) -> Vec<usize> {
    let mut stops = Vec::new();
    let mut caret = start;
    loop {
      let next = separators
        .and_then(|separators| word_stop(value, caret, direction, separators))
        .unwrap_or_else(|| stock_word_jump(value, caret, direction));
      if next == caret {
        return stops;
      }
      stops.push(next);
      caret = next;
    }
  }

  #[test]
  fn the_stock_jump_is_gpui_kits() {
    // Recorded from gpui-base 0.9.1's `InputState` with Option+Arrow.
    let stock = |value, start, direction| walk(value, start, direction, None);
    assert_eq!(stock("123.456", 0, Direction::Right), [7]);
    assert_eq!(stock("123.456", 7, Direction::Left), [0]);
    assert_eq!(stock("123.456", 3, Direction::Right), [4, 7]);
    assert_eq!(stock("123.456", 4, Direction::Left), [3, 0]);
    assert_eq!(stock("Movie.2024.1080p", 0, Direction::Right), [5, 6, 16]);
    assert_eq!(stock("Movie.2024.1080p", 16, Direction::Left), [6, 5, 0]);
    assert_eq!(stock("abc 123.456 def", 0, Direction::Right), [3, 11, 15]);
    assert_eq!(stock("abc 123.456 def", 15, Direction::Left), [12, 4, 0]);
    assert_eq!(stock("abc.def", 0, Direction::Right), [7]);
    assert_eq!(stock("abc.def", 3, Direction::Right), [4, 7]);
    assert_eq!(stock("hello world", 0, Direction::Right), [5, 11]);
    assert_eq!(stock("hello world", 11, Direction::Left), [6, 0]);
    assert_eq!(stock("abc\n123.456", 3, Direction::Right), [11]);
    assert_eq!(stock("abc\n123.456", 4, Direction::Left), [0]);
    assert_eq!(stock("123.456\n", 8, Direction::Left), [0]);
    assert_eq!(stock("C:/Videos/movie.mkv", 19, Direction::Left), [10, 9, 3, 2, 1, 0]);
    assert_eq!(stock("x264.mkv", 0, Direction::Right), [4, 5, 8]);
  }

  #[test]
  fn each_part_of_a_number_is_a_word() {
    let jumps = |value, start, direction| walk(value, start, direction, Some(Separators::Numbers));
    assert_eq!(jumps("123.456", 0, Direction::Right), [3, 7]);
    assert_eq!(jumps("123.456", 7, Direction::Left), [4, 0]);
    assert_eq!(jumps("1,234.567", 0, Direction::Right), [1, 5, 9]);
    assert_eq!(jumps("1,234.567", 9, Direction::Left), [6, 2, 0]);
    assert_eq!(jumps("Movie.2024.1080p", 0, Direction::Right), [5, 6, 10, 16]);
    assert_eq!(jumps("Movie.2024.1080p", 16, Direction::Left), [11, 6, 5, 0]);
    assert_eq!(jumps("abc 123.456 def", 0, Direction::Right), [3, 7, 11, 15]);
    assert_eq!(jumps("abc 123.456 def", 15, Direction::Left), [12, 8, 4, 0]);
    // A jump starting beside the separator passes it.
    assert_eq!(jumps("123.456", 3, Direction::Right), [7]);
    assert_eq!(jumps("123.456", 4, Direction::Left), [0]);
    // Line breaks are passed like spaces, as gpui-kit's jumps pass them.
    assert_eq!(jumps("abc\n123.456", 3, Direction::Right), [7, 11]);
    assert_eq!(jumps("abc\n123.456", 11, Direction::Left), [8, 4, 0]);
    assert_eq!(jumps("123.456\n", 8, Direction::Left), [4, 0]);
    // Other punctuation in the word leaves the jump to gpui-kit.
    assert_eq!(jumps("abc.def123.456", 0, Direction::Right), [14]);
    assert_eq!(jumps("abc.def123.456", 4, Direction::Right), [10, 14]);
    assert_eq!(jumps("abc.def123.456", 14, Direction::Left), [11, 0]);
  }

  #[test]
  fn the_first_stops_are_the_per_platform_macos_rule() {
    for (value, caret, direction) in [
      ("123.456", 0, Direction::Right),
      ("123.456", 7, Direction::Left),
      ("1,234.567", 1, Direction::Right),
      ("1,234.567", 6, Direction::Left),
      (" 123.456", 0, Direction::Right),
      ("123.456  ", 9, Direction::Left),
      ("字幕 １２３.４５６", 7, Direction::Right),
    ] {
      assert_eq!(
        word_stop(value, caret, direction, Separators::Numbers),
        numeric_word_boundary(value, caret, direction, true),
        "{value:?} {caret} {direction:?}"
      );
    }
  }

  #[test]
  fn each_part_of_a_file_name_in_a_path_is_a_word() {
    let jumps = |value, start, direction| walk(value, start, direction, Some(Separators::Paths));
    assert_eq!(jumps("movie.mkv", 0, Direction::Right), [5, 9]);
    assert_eq!(jumps("movie.mkv", 9, Direction::Left), [6, 0]);
    // The dots of a number, and those gpui-kit gives a stop of their own.
    let path = "C:/Videos/Movie.2024.1080p.mkv";
    assert_eq!(jumps(path, 0, Direction::Right), [1, 2, 3, 9, 10, 15, 20, 26, 30]);
    assert_eq!(jumps(path, 30, Direction::Left), [27, 21, 16, 10, 9, 3, 2, 1, 0]);
    assert_eq!(jumps("x264.mkv", 0, Direction::Right), [4, 8]);
    assert_eq!(jumps("x264.mkv", 8, Direction::Left), [5, 0]);
    // A number's `,` too; a word jump still passes spaces and a quote.
    assert_eq!(jumps("1,234.mkv", 0, Direction::Right), [1, 5, 9]);
    assert_eq!(jumps("My Movie.mkv", 12, Direction::Left), [9, 3, 0]);
    assert_eq!(jumps("can't.mkv", 0, Direction::Right), [5, 9]);
    // Outside a path a dot between letters splits nothing.
    let text = |value, start, direction| walk(value, start, direction, Some(Separators::Numbers));
    assert_eq!(text("movie.mkv", 0, Direction::Right), [9]);
    assert_eq!(text(path, 30, Direction::Left), [16, 15, 10, 9, 3, 2, 1, 0]);
  }

  #[test]
  fn other_dots_in_a_path_keep_gpui_kits_jumps() {
    for value in [".config", "../movies", "movie.", "a .b", "a. b", "C:/a b/c-d_e", "a..b"] {
      let carets = value.char_indices().map(|(byte, _)| byte).chain([value.len()]);
      for caret in carets {
        for direction in [Direction::Left, Direction::Right] {
          assert_eq!(
            word_stop(value, caret, direction, Separators::Paths),
            None,
            "{value:?} {caret} {direction:?}"
          );
        }
      }
    }
  }

  #[test]
  fn text_without_numbers_keeps_gpui_kits_jumps() {
    for value in [
      "",
      "hello world",
      "abc.def",
      "can't",
      "你好世界",
      "123456",
      "123..456",
      "1. Intro",
      "v1.",
      ".5",
      "line one\nline two",
    ] {
      let carets = value.char_indices().map(|(byte, _)| byte).chain([value.len()]);
      for caret in carets {
        for direction in [Direction::Left, Direction::Right] {
          assert_eq!(
            word_stop(value, caret, direction, Separators::Numbers),
            None,
            "{value:?} {caret} {direction:?}"
          );
        }
      }
    }
  }

  #[test]
  fn unicode_numbers_split_too() {
    // The CJK and full-width chars take 3 bytes, the emoji 4.
    let jumps = |value, start, direction| walk(value, start, direction, Some(Separators::Numbers));
    assert_eq!(jumps("字幕 １２３.４５６", 0, Direction::Right), [3, 6, 16, 26]);
    assert_eq!(jumps("字幕 １２３.４５６", 26, Direction::Left), [17, 7, 3, 0]);
    assert_eq!(jumps("🎬 123.456", 0, Direction::Right), [4, 8, 12]);
    assert_eq!(jumps("🎬 123.456", 12, Direction::Left), [9, 5, 0]);
  }

  fn selection(range: Range<usize>, reversed: bool) -> Selection {
    Selection { range, reversed }
  }

  #[test]
  fn moves_from_the_caret() {
    let value = "1,234.567";
    let command = |range, reversed, direction| {
      word_command(
        value,
        &selection(range, reversed),
        direction,
        WordCommand::Move,
        Separators::Numbers,
      )
    };
    assert_eq!(command(0..0, false, Direction::Right), Some(selection(1..1, false)));
    // A selection collapses at the stop after its caret, as gpui-kit's moves.
    assert_eq!(command(0..2, false, Direction::Right), Some(selection(5..5, false)));
    assert_eq!(command(2..6, true, Direction::Right), Some(selection(5..5, false)));
    assert_eq!(command(9..9, false, Direction::Left), Some(selection(6..6, false)));
    assert_eq!(command(6..6, false, Direction::Right), None);
    assert_eq!(
      word_command(
        "abc",
        &selection(0..0, false),
        Direction::Right,
        WordCommand::Move,
        Separators::Numbers
      ),
      None
    );
  }

  #[test]
  fn extends_the_selection_from_its_anchor() {
    let value = "1,234.567";
    let command = |range, reversed, direction| {
      word_command(
        value,
        &selection(range, reversed),
        direction,
        WordCommand::Select,
        Separators::Numbers,
      )
    };
    assert_eq!(command(0..0, false, Direction::Right), Some(selection(0..1, false)));
    assert_eq!(command(0..1, false, Direction::Right), Some(selection(0..5, false)));
    assert_eq!(command(9..9, false, Direction::Left), Some(selection(6..9, true)));
    assert_eq!(command(6..9, true, Direction::Left), Some(selection(2..9, true)));
    // Back to the anchor collapses; past it the selection turns round.
    assert_eq!(command(2..6, false, Direction::Left), Some(selection(2..2, false)));
    assert_eq!(command(5..6, false, Direction::Left), Some(selection(2..5, true)));
    // No stop after the caret: gpui-kit extends the selection itself.
    assert_eq!(command(6..9, true, Direction::Right), None);
  }

  #[test]
  fn deletes_up_to_the_stop() {
    let command = |value, range, direction| {
      word_command(
        value,
        &selection(range, false),
        direction,
        WordCommand::Delete,
        Separators::Numbers,
      )
    };
    assert_eq!(
      command("1,234.567", 9..9, Direction::Left),
      Some(selection(6..9, false))
    );
    assert_eq!(
      command("1,234.567", 0..0, Direction::Right),
      Some(selection(0..1, false))
    );
    assert_eq!(command("123.456", 4..4, Direction::Left), Some(selection(0..4, false)));
    assert_eq!(command("123.456", 3..3, Direction::Right), Some(selection(3..7, false)));
    // Selected text deletes itself, and words without numbers as gpui-kit.
    assert_eq!(command("1,234.567", 0..2, Direction::Left), None);
    assert_eq!(command("hello world", 11..11, Direction::Left), None);
    // In a path, the extension alone.
    assert_eq!(command("movie.mkv", 9..9, Direction::Left), None);
    assert_eq!(
      word_command(
        "movie.mkv",
        &selection(9..9, false),
        Direction::Left,
        WordCommand::Delete,
        Separators::Paths
      ),
      Some(selection(6..9, false))
    );
  }

  #[test]
  fn the_caret_is_the_moving_end() {
    assert_eq!(selection(2..6, false).caret(), 6);
    assert_eq!(selection(2..6, true).caret(), 2);
    assert_eq!(selection(4..4, false).caret(), 4);
  }
}
