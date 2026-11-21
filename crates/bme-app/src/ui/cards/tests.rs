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

//! Headless tests of the cards, their track tables, the output path dialog
//! and the toolbar's global output path.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use bme_core::extract::ExtractOutcome;
use bme_core::extract_args::parent_dir;
use bme_core::protocol::TrackCounts;
use core::prelude::v1::test;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::*;

use super::group_card::file_list_width;
use super::*;
use crate::domain::status::QueueItem;
use crate::state::files::tests::{TempDir, s};
use crate::ui::file_list::CardView;
use crate::ui::tests::{Shell, finished, movies, progress, started};
use crate::ui::track_table::TrackTable;

/// Select All in a text field: Cmd+A on macOS, Ctrl+A elsewhere.
const SELECT_ALL: &str = if cfg!(target_os = "macos") { "cmd-a" } else { "ctrl-a" };

const A: &str = "/media/a.mkv";
const B: &str = "/media/b.mkv";

/// A shell listing `files` (each the test movie, loaded and auto-selected),
/// its window active so that focus events arrive.
fn shell(cx: &mut TestAppContext, files: &[&str]) -> Shell {
  shell_over(cx, files, files)
}

/// [`shell`] over the movies `fixtures`, listing `files`.
fn shell_over(cx: &mut TestAppContext, fixtures: &[&str], files: &[&str]) -> Shell {
  let shell = Shell::with_files(cx, movies(fixtures));
  shell.add_files(cx, files);
  shell.window(cx, |window, _| window.activate_window());
  cx.run_until_parked();
  shell
}

/// [`shell`] with Group Mode on.
fn group_shell(cx: &mut TestAppContext, files: &[&str]) -> Shell {
  group_shell_over(cx, files, files)
}

fn group_shell_over(cx: &mut TestAppContext, fixtures: &[&str], files: &[&str]) -> Shell {
  let shell = shell_over(cx, fixtures, files);
  shell
    .stores
    .files
    .update(cx, |files, cx| files.set_group_mode(true, cx));
  cx.run_until_parked();
  shell.window(cx, |window, cx| window.render_frame(cx));
  shell
}

/// The card showing `file`.
fn card(shell: &Shell, cx: &mut TestAppContext, file: &str) -> CardView {
  shell.window(cx, |window, cx| window.render_frame(cx));
  cx.read(|cx| {
    shell
      .view
      .read(cx)
      .main()
      .read(cx)
      .file_list()
      .read(cx)
      .card_of(file, cx)
      .cloned()
      .unwrap_or_else(|| panic!("no card for {file}"))
  })
}

/// The texts painted inside `scope` in a fresh frame.
fn texts_in(shell: &Shell, cx: &mut TestAppContext, scope: impl Into<ElementId>) -> Vec<String> {
  let scope = scope.into();
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    gpui_kit::base::test_support::texts(window)
      .into_iter()
      .filter(|text| text.path().contains(&scope))
      .map(|text| text.text().to_string())
      .collect()
  })
}

/// The texts painted inside the file-list entry `index`.
fn texts(shell: &Shell, cx: &mut TestAppContext, index: usize) -> Vec<String> {
  texts_in(shell, cx, ("file-entry", index))
}

/// Whether `id` inside the file-list entry `index` is on screen and its
/// disabled state.
fn control(shell: &Shell, cx: &mut TestAppContext, index: usize, id: impl Into<ElementId>) -> Option<bool> {
  let id = id.into();
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window
      .within(("file-entry", index))
      .try_find(id)
      .map(|control| control.disabled() == Some(true))
  })
}

fn assert_action_sizes(shell: &Shell, cx: &mut TestAppContext, ids: &[&'static str]) {
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    let card = window.within(("file-entry", 0usize));
    for &id in ids {
      assert_eq!(card.find(id).bounds().size, size(px(24.0), px(24.0)), "{id}");
    }
  });
}

fn click_in(shell: &Shell, cx: &mut TestAppContext, index: usize, id: impl Into<ElementId>) {
  let id = id.into();
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.within(("file-entry", index)).click(id, cx);
  });
  cx.run_until_parked();
}

fn clipboard(cx: &mut TestAppContext) -> Option<String> {
  cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
}

fn selected_ids(shell: &Shell, cx: &mut TestAppContext, file: &str) -> Vec<String> {
  shell.stores.files.read_with(cx, |files, _| {
    files.selected_ids(file).map(<[String]>::to_vec).unwrap_or_default()
  })
}

fn output_dir(shell: &Shell, cx: &mut TestAppContext, file: &str) -> Option<String> {
  shell
    .stores
    .files
    .read_with(cx, |files, _| files.output_dir(file).map(str::to_string))
}

/// Draws a frame without a refresh, as the app does: cached views that
/// were not notified are reused.
fn draw(shell: &Shell, cx: &mut TestAppContext) {
  shell.window(cx, |window, cx| window.draw(cx).clear(cx));
}

#[gpui_kit::test]
fn a_file_card_shows_its_path_summary_and_actions(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A]);
  assert!(card(&shell, cx, A).file().is_some());
  let shown = texts(&shell, cx, 0);
  assert!(shown.contains(&A.to_string()), "{shown:?}");
  assert!(shown.contains(&"1 🎞️ 2 🔊 2 💬".to_string()), "{shown:?}");
  // The profile selected a track: Copy and Extract are available; no
  // BetterMediaInfo, no progress.
  for id in ["card-output", "card-copy", "card-extract", "card-delete"] {
    assert_eq!(control(&shell, cx, 0, id), Some(false), "{id}");
  }
  assert_action_sizes(&shell, cx, &["card-output", "card-copy", "card-delete"]);
  for id in ["card-bmi", "card-progress"] {
    assert_eq!(control(&shell, cx, 0, id), None, "{id}");
  }
  // Without a selection Copy and Extract are off.
  let scope = crate::state::files::TableScope::File(A.to_string());
  shell
    .stores
    .files
    .update(cx, |files, cx| files.set_all_checked(&scope, false, cx));
  assert_eq!(control(&shell, cx, 0, "card-copy"), Some(true));
  assert_eq!(control(&shell, cx, 0, "card-extract"), Some(true));
}

#[test]
fn the_summary_joins_the_counts_and_the_output_directory() {
  let _locale = crate::i18n::tests::LOCALE_LOCK
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  crate::i18n::set_language(bme_core::config::Language::EnUS);
  let counts = TrackCounts {
    video: 1,
    audio: 2,
    chapters: 1,
    ..Default::default()
  };
  assert_eq!(card_summary(None, None), None);
  assert_eq!(card_summary(Some(&TrackCounts::default()), Some("")), None);
  assert_eq!(card_summary(Some(&counts), None).as_deref(), Some("1 🎞️ 2 🔊 1 📑"));
  assert_eq!(
    card_summary(Some(&counts), Some("/out")).as_deref(),
    Some("1 🎞️ 2 🔊 1 📑 To path /out")
  );
  assert_eq!(card_summary(None, Some("/out")).as_deref(), Some("To path /out"));
}

#[test]
fn the_time_label_shows_elapsed_and_eta_while_extracting() {
  let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
  let mut item = QueueItem::waiting("/m/a.mkv");
  assert_eq!(time_label(None, start), "--:--:-- / --:--:--");
  assert_eq!(time_label(Some(&item), start), "--:--:-- / --:--:--");
  item.status = crate::domain::status::QueueItemStatus::Extracting;
  item.extract_started_at = Some(start);
  item.progress = 25;
  assert_eq!(
    time_label(Some(&item), start + Duration::from_secs(90)),
    "00:01:30 / 00:04:30"
  );
}

#[gpui_kit::test]
fn extracting_from_a_card_shows_progress_and_cancel(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A]);
  click_in(&shell, cx, 0, "card-extract");
  assert_eq!(shell.enqueued(), [A]);
  // Waiting: no output path, copy or extract; Delete and Cancel stay.
  for (id, disabled) in [
    ("card-output", true),
    ("card-copy", true),
    ("card-extract", true),
    ("card-delete", false),
    ("card-cancel", false),
  ] {
    assert_eq!(control(&shell, cx, 0, id), Some(disabled), "waiting: {id}");
  }
  assert!(
    !texts(&shell, cx, 0).iter().any(|text| text.contains(" / ")),
    "no times while waiting"
  );
  shell.send(cx, started(A));
  shell.send(cx, progress(A, 40));
  assert_eq!(control(&shell, cx, 0, "card-delete"), Some(true), "extracting");
  let shown = texts(&shell, cx, 0);
  assert!(
    shown.iter().any(|text| text.len() == 19 && text.contains(" / ")),
    "elapsed / ETA in {shown:?}"
  );
  click_in(&shell, cx, 0, "card-cancel");
  assert_eq!(shell.cancelled(), [A]);
  shell.send(cx, finished(A, ExtractOutcome::Cancelled, None));
  assert_eq!(control(&shell, cx, 0, "card-progress"), None);
  assert_eq!(control(&shell, cx, 0, "card-extract"), Some(false));
}

#[gpui_kit::test]
fn delete_removes_the_card_and_cancels_a_waiting_extraction(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A, B]);
  click_in(&shell, cx, 0, "card-extract");
  click_in(&shell, cx, 0, "card-delete");
  assert_eq!(shell.cancelled(), [A]);
  assert_eq!(shell.stores.files.read_with(cx, |files, _| files.files().to_vec()), [B]);
  assert_eq!(shell.status(cx, A), None, "its queue row goes too");
  // The remaining card is B's.
  assert!(texts(&shell, cx, 0).contains(&B.to_string()));
}

#[gpui_kit::test]
fn the_file_card_path_can_be_selected_and_copied_without_editing(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A]);
  let path = card(&shell, cx, A)
    .file()
    .unwrap()
    .read_with(cx, |card, _| card.path_state().clone());
  shell.window(cx, |window, cx| {
    let summary = window.find("card-summary").bounds();
    let text = path.read(cx).text_bounds().expect("path text was laid out");
    assert_eq!(text.left(), summary.left());
  });
  shell.click(cx, "card-path");
  shell.press(cx, SELECT_ALL);
  shell.window(cx, |window, cx| {
    assert!(path.read(cx).focus_handle(cx).is_focused(window));
    assert_eq!(path.read(cx).selected_range(), 0..A.len());
  });
  let copy = if cfg!(target_os = "macos") { "cmd-c" } else { "ctrl-c" };
  shell.press(cx, copy);
  assert_eq!(clipboard(cx).as_deref(), Some(A));
  shell.window(cx, |window, cx| window.input("edited", cx));
  shell.press(cx, "backspace");
  assert_eq!(path.read_with(cx, |path, _| path.value()).as_ref(), A);
}

#[gpui_kit::test]
fn only_the_extracting_card_repaints(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A, B]);
  let (card_a, card_b) = (card(&shell, cx, A), card(&shell, cx, B));
  draw(&shell, cx);
  draw(&shell, cx);
  let counts = |cx: &mut TestAppContext| cx.read(|cx| (card_a.render_count(cx), card_b.render_count(cx)));
  let before = counts(cx);
  draw(&shell, cx);
  assert_eq!(counts(cx), before, "an idle frame reuses both cards");
  click_in(&shell, cx, 1, "card-extract");
  draw(&shell, cx);
  draw(&shell, cx);
  // The lower card extracts, so its progress row moves nothing.
  for event in [started(B), progress(B, 10), progress(B, 20)] {
    let (a_before, b_before) = counts(cx);
    shell.send(cx, event);
    draw(&shell, cx);
    draw(&shell, cx);
    let (a_after, b_after) = counts(cx);
    assert!(b_after > b_before, "the extracting card repaints");
    assert_eq!(a_after, a_before, "the other card does not");
  }
}

/// A view that counts its renders, drawn by [`MeasuredHost`].
struct Counted(Rc<Cell<usize>>);

impl Render for Counted {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    self.0.set(self.0.get() + 1);
    div().h(px(20.0))
  }
}

struct MeasuredHost {
  child: Entity<Counted>,
  height: CardHeight,
  cache: bool,
}

impl Render for MeasuredHost {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div().size_full().child(self.height.view(&self.child, self.cache))
  }
}

#[gpui_kit::test]
fn cards_are_drawn_uncached_for_assistive_technology(cx: &mut TestAppContext) {
  // GPUI does not replay a cached view's accessibility nodes, so the list
  // draws its cards uncached while a screen reader is connected.
  for cache in [true, false] {
    let renders = Rc::new(Cell::new(0));
    let window = cx.add_window(|_, cx| MeasuredHost {
      child: cx.new(|_| Counted(renders.clone())),
      height: CardHeight(Rc::new(Cell::new(Some(px(20.0))))),
      cache,
    });
    // The host renders again (notified), the child only when not cached.
    let draw = |cx: &mut TestAppContext| {
      window.update(cx, |_, _, cx| cx.notify()).unwrap();
      cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
        .unwrap()
    };
    draw(cx);
    let before = renders.get();
    draw(cx);
    draw(cx);
    let renders = renders.get() - before;
    if cache {
      assert_eq!(renders, 0, "a cached view is reused");
    } else {
      assert!(renders >= 2, "an uncached view renders every frame: {renders}");
    }
  }
}

#[gpui_kit::test]
fn copy_command_copies_the_cards_command(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A]);
  shell.stores.config.update(cx, |config, cx| {
    config.update(cx, |config| {
      config.integration.mkv_toolnix_path = "/opt/mkvtoolnix".into()
    })
  });
  let request = cx.read(|cx| crate::actions::requests(&shell.stores, &s(&[A]), cx)[0].clone());
  assert_eq!(request.mkv_toolnix_path, "/opt/mkvtoolnix", "the configured MKVToolNix");
  let expected = request.command();
  let toasts = shell.toasts(cx);
  click_in(&shell, cx, 0, "card-copy");
  assert_eq!(clipboard(cx), Some(expected.clone()));
  assert!(expected.contains("tracks") && expected.contains("3:"), "{expected}");
  assert_eq!(shell.toasts(cx), toasts + 1, "Command copied to clipboard.");
}

#[gpui_kit::test]
fn two_files_of_a_folder_form_one_group_card(cx: &mut TestAppContext) {
  let shell = group_shell(cx, &[A, B]);
  assert_action_sizes(&shell, cx, &["group-output", "group-copy", "group-clear"]);
  let group = card(&shell, cx, A);
  let group = group.group().expect("a group card");
  assert_eq!(group.read_with(cx, |card, _| card.files().to_vec()), [A, B]);
  let shown = texts(&shell, cx, 0);
  for expected in [parent_dir(A), "a.mkv", "b.mkv", "1 🎞️ 2 🔊 2 💬"] {
    assert!(shown.iter().any(|text| text == expected), "{expected} in {shown:?}");
  }
  shell.window(cx, |window, _| {
    assert!(window.try_find(("file-entry", 1usize)).is_none(), "one entry");
  });
  shell
    .stores
    .files
    .update(cx, |files, cx| files.set_group_mode(false, cx));
  cx.run_until_parked();
  assert!(card(&shell, cx, A).file().is_some());
  assert!(card(&shell, cx, B).file().is_some());
}

#[gpui_kit::test]
fn a_group_card_extracts_copies_and_clears_every_file(cx: &mut TestAppContext) {
  let shell = group_shell(cx, &[A, B]);
  shell.resize(cx, 1400.0, 760.0);
  click_in(&shell, cx, 0, "group-copy");
  let copied = clipboard(cx).expect("copied commands");
  let lines: Vec<&str> = copied.lines().collect();
  assert_eq!(lines.len(), 2, "{copied}");
  assert!(lines[0].contains(A) && lines[1].contains(B), "{copied}");
  click_in(&shell, cx, 0, "group-extract");
  assert_eq!(shell.enqueued(), [A, B]);
  for id in ["group-extract", "group-output", "group-clear"] {
    assert_eq!(control(&shell, cx, 0, id), Some(true), "{id} while active");
  }
  assert_eq!(control(&shell, cx, 0, "group-copy"), Some(false), "copying stays");
  shell.send(cx, started(A));
  shell.send(cx, progress(A, 30));
  let shown = texts(&shell, cx, 0);
  assert!(shown.contains(&"30%".to_string()), "{shown:?}");
  assert!(
    shown.iter().any(|text| text.len() == 19 && text.contains(" / ")),
    "{shown:?}"
  );
  assert_eq!(control(&shell, cx, 0, ("group-cancel", 0usize)), Some(false));
  assert_eq!(
    control(&shell, cx, 0, ("group-cancel", 1usize)),
    None,
    "only while extracting"
  );
  click_in(&shell, cx, 0, ("group-cancel", 0usize));
  assert_eq!(shell.cancelled(), [A]);
  for (file, outcome) in [(A, ExtractOutcome::Cancelled), (B, ExtractOutcome::Completed)] {
    shell.send(cx, finished(file, outcome, None));
  }
  click_in(&shell, cx, 0, "group-clear");
  assert!(shell.stores.files.read_with(cx, |files, _| !files.has_files()));
  assert_eq!(shell.status(cx, B), None, "the queue rows go too");
}

#[test]
fn the_file_list_of_a_group_keeps_its_limits() {
  let (left, width) = (px(100.0), px(1000.0));
  assert_eq!(file_list_width(px(400.0), left, width), px(300.0));
  assert_eq!(file_list_width(px(150.0), left, width), px(160.0));
  assert_eq!(file_list_width(px(1000.0), left, width), px(760.0));
  // A narrow pane keeps the minimum.
  assert_eq!(file_list_width(px(400.0), left, px(300.0)), px(160.0));
}

#[gpui_kit::test]
fn the_group_splitter_resizes_the_file_list(cx: &mut TestAppContext) {
  let c = "/media/c.mkv";
  let shell = group_shell_over(cx, &[A, B, c], &[A, B]);
  let group = card(&shell, cx, A).group().unwrap().clone();
  let splitter = |shell: &Shell, cx: &mut TestAppContext| {
    shell.window(cx, |window, cx| {
      window.render_frame(cx);
      window
        .within(("file-entry", 0usize))
        .find("group-splitter")
        .bounds()
        .center()
    })
  };
  let start = splitter(&shell, cx);
  shell.window(cx, |window, cx| {
    window.drag(start, start + point(px(100.0), px(0.0)), cx)
  });
  cx.run_until_parked();
  let width = group.read_with(cx, |card, _| card.file_list_width());
  assert!((width - px(340.0)).abs() < px(4.0), "{width:?}");
  let start = splitter(&shell, cx);
  shell.window(cx, |window, cx| window.drag(start, point(px(0.0), start.y), cx));
  cx.run_until_parked();
  assert_eq!(group.read_with(cx, |card, _| card.file_list_width()), px(160.0));
  // A file joining keeps the card and its width.
  shell
    .stores
    .files
    .update(cx, |files, cx| files.add_files(vec![c.to_string()], cx));
  cx.run_until_parked();
  let same = card(&shell, cx, c).group().unwrap().clone();
  assert_eq!(same.entity_id(), group.entity_id());
  assert_eq!(same.read_with(cx, |card, _| card.files().to_vec()), [A, B, c]);
  assert_eq!(same.read_with(cx, |card, _| card.file_list_width()), px(160.0));
}

// The track table.

/// Runs `f` on the window with queries scoped to the table of `file`'s card,
/// after a fresh frame, then lets the effects run.
fn in_table<R>(
  shell: &Shell,
  cx: &mut TestAppContext,
  shown: &str,
  f: impl FnOnce(&mut gpui_kit::test::ScopedWindow<'_>, &mut App) -> R,
) -> R {
  let scope = TrackTable::element_id(&crate::state::files::TableScope::File(shown.to_string()));
  let result = shell.window(cx, |window, cx| {
    window.render_frame(cx);
    let mut scoped = window.within(scope);
    f(&mut scoped, cx)
  });
  cx.run_until_parked();
  result
}

/// A click on row `index`'s ID cell, away from its checkbox.
fn click_row(shell: &Shell, cx: &mut TestAppContext, shown: &str, index: usize) {
  in_table(shell, cx, shown, |window, cx| {
    window.click_at(("track-row", index), point(px(60.0), px(12.0)), cx)
  });
}

fn table_texts(shell: &Shell, cx: &mut TestAppContext, shown: &str) -> Vec<String> {
  let scope = TrackTable::element_id(&crate::state::files::TableScope::File(shown.to_string()));
  texts_in(shell, cx, ElementId::Name(scope))
}

#[gpui_kit::test]
fn the_table_lists_every_track_with_its_cells(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A]);
  assert_eq!(
    texts_in(&shell, cx, "track-head"),
    ["ID", "Type", "Codec", "Language", "Track Name"]
  );
  assert_eq!(texts_in(&shell, cx, ("track-row", 2usize)), ["2", "A_AC3", "ger", ""]);
  in_table(&shell, cx, A, |window, _| {
    for index in 0..5usize {
      window.find(("track-row", index));
      window.find(("track-type", index));
    }
    assert!(window.try_find(("track-row", 5usize)).is_none());
    assert!(window.try_find_by_label("Select all").is_some());
  });
  shell.window(cx, |window, _| assert!(window.try_find_by_label("Tracks").is_some()));
}

#[gpui_kit::test]
fn a_row_click_toggles_its_checkbox_and_select_all_follows(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A]);
  assert_eq!(selected_ids(&shell, cx, A), ["subtitles:3"], "the profile's");
  let all = |shell: &Shell, cx: &mut TestAppContext| {
    in_table(shell, cx, A, |window, _| {
      let all = window.find("track-check-all");
      (all.checked(), all.indeterminate())
    })
  };
  assert_eq!(all(&shell, cx), (Some(false), Some(true)), "mixed");
  click_row(&shell, cx, A, 1);
  assert_eq!(selected_ids(&shell, cx, A), ["subtitles:3", "audio:1"]);
  click_row(&shell, cx, A, 3);
  assert_eq!(selected_ids(&shell, cx, A), ["audio:1"]);
  // The checkbox flips once, not again through its row.
  in_table(&shell, cx, A, |window, cx| window.click(("track-check", 0usize), cx));
  assert_eq!(selected_ids(&shell, cx, A), ["audio:1", "video:0"]);
  in_table(&shell, cx, A, |window, _| {
    assert_eq!(window.find(("track-check", 0usize)).checked(), Some(true));
  });
  // Mixed → every row; again → none.
  in_table(&shell, cx, A, |window, cx| window.click("track-check-all", cx));
  assert_eq!(selected_ids(&shell, cx, A).len(), 5);
  assert_eq!(all(&shell, cx), (Some(true), Some(false)));
  in_table(&shell, cx, A, |window, cx| window.click("track-check-all", cx));
  assert!(selected_ids(&shell, cx, A).is_empty());
  assert_eq!(all(&shell, cx), (Some(false), Some(false)));
  assert_eq!(control(&shell, cx, 0, "card-extract"), Some(true), "nothing to extract");
}

/// Each card's table is its own: clicks in the second card's table select
/// for that card's file only.
#[gpui_kit::test]
fn the_second_cards_table_takes_its_own_clicks(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A, B]);
  click_row(&shell, cx, B, 0);
  in_table(&shell, cx, B, |window, cx| window.click(("track-check", 1usize), cx));
  assert_eq!(selected_ids(&shell, cx, B), ["subtitles:3", "video:0", "audio:1"]);
  assert_eq!(selected_ids(&shell, cx, A), ["subtitles:3"]);
  in_table(&shell, cx, B, |window, cx| window.click("track-check-all", cx));
  assert_eq!(selected_ids(&shell, cx, B).len(), 5);
  assert_eq!(selected_ids(&shell, cx, A), ["subtitles:3"]);
}

/// What assistive clients are given for the same control of two cards: GPUI
/// names each accessibility node by its element's whole id path (the node id
/// is that path's hash), and the snapshots here record the same path and the
/// same node properties. The paths differ, through the card's view and its
/// table, and each reports its own card's state and bounds.
#[gpui_kit::test]
fn the_same_control_of_two_cards_is_two_accessibility_nodes(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A, B]);
  in_table(&shell, cx, B, |window, cx| window.click(("track-check", 0usize), cx));
  let a = in_table(&shell, cx, A, |window, _| window.find(("track-check", 0usize)));
  let b = in_table(&shell, cx, B, |window, _| window.find(("track-check", 0usize)));
  assert_ne!(a.path(), b.path());
  for snapshot in [&a, &b] {
    assert!(
      snapshot.path().iter().any(|id| matches!(id, ElementId::View(_))),
      "{:?}",
      snapshot.path()
    );
  }
  assert_eq!((a.checked(), b.checked()), (Some(false), Some(true)));
  assert_ne!(a.bounds(), b.bounds());
  assert_eq!(
    selected_ids(&shell, cx, A),
    ["subtitles:3"],
    "the first card's file is untouched"
  );
}

#[gpui_kit::test]
fn an_active_cards_table_takes_no_clicks(cx: &mut TestAppContext) {
  let shell = shell(cx, &[A]);
  click_in(&shell, cx, 0, "card-extract");
  click_row(&shell, cx, A, 1);
  in_table(&shell, cx, A, |window, cx| {
    assert_eq!(window.find(("track-check", 1usize)).disabled(), Some(true));
    assert_eq!(window.find("track-check-all").disabled(), Some(true));
    window.click("track-check-all", cx);
  });
  assert_eq!(selected_ids(&shell, cx, A), ["subtitles:3"]);
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  click_row(&shell, cx, A, 1);
  assert_eq!(selected_ids(&shell, cx, A), ["subtitles:3", "audio:1"], "finished");
}

/// A row `mkvextract` cannot extract has a disabled checkbox named by why,
/// takes no click, and select-all checks the rest.
#[gpui_kit::test]
fn a_track_mkvextract_cannot_extract_is_disabled(cx: &mut TestAppContext) {
  use crate::state::files::tests::track;
  let fixtures = std::collections::HashMap::from([(
    A.to_string(),
    vec![track(0, "video", "V_FFV1", "und"), track(1, "audio", "A_FLAC", "eng")],
  )]);
  let shell = Shell::with_files(cx, fixtures);
  shell.add_files(cx, &[A]);
  shell.window(cx, |window, _| window.activate_window());
  cx.run_until_parked();
  in_table(&shell, cx, A, |window, _| {
    assert_eq!(window.find(("track-check", 0usize)).disabled(), Some(true));
    assert_ne!(window.find(("track-check", 1usize)).disabled(), Some(true));
    assert!(
      window
        .try_find_by_label("mkvextract cannot extract V_FFV1 tracks.")
        .is_some()
    );
  });
  click_row(&shell, cx, A, 0);
  assert!(selected_ids(&shell, cx, A).is_empty());
  in_table(&shell, cx, A, |window, cx| window.click("track-check-all", cx));
  assert_eq!(selected_ids(&shell, cx, A), ["audio:1"]);
  in_table(&shell, cx, A, |window, _| {
    let all = window.find("track-check-all");
    assert_eq!((all.checked(), all.indeterminate()), (Some(true), Some(false)));
  });
}

#[gpui_kit::test]
fn the_table_shows_loading_an_error_or_no_tracks(cx: &mut TestAppContext) {
  let (empty, bad) = ("/media/empty.mkv", "/media/bad.mkv");
  let mut fixtures = movies(&[A]);
  fixtures.insert(empty.to_string(), Vec::new());
  let shell = Shell::with_files(cx, fixtures);
  shell
    .stores
    .files
    .update(cx, |files, cx| files.add_files(s(&[A, empty, bad]), cx));
  // Listed on the background executor: not yet.
  in_table(&shell, cx, A, |window, _| {
    assert!(
      window.try_find(("track-row", 0usize)).is_none(),
      "a spinner while loading"
    );
  });
  cx.run_until_parked();
  let failed = table_texts(&shell, cx, bad);
  assert!(
    failed.iter().any(|text| text.starts_with("mkvmerge failed:")),
    "{failed:?}"
  );
  // Retry lists it again (and it fails again).
  in_table(&shell, cx, bad, |window, cx| window.click("track-retry", cx));
  let failed = table_texts(&shell, cx, bad);
  assert!(
    failed.iter().any(|text| text.starts_with("mkvmerge failed:")),
    "{failed:?}"
  );
  assert_eq!(table_texts(&shell, cx, empty), ["No tracks found."]);
  in_table(&shell, cx, A, |window, _| {
    window.find(("track-row", 0usize));
  });
  assert_eq!(control(&shell, cx, 1, "card-extract"), Some(true), "nothing to extract");
}

#[gpui_kit::test]
fn a_group_cards_table_checks_for_every_file(cx: &mut TestAppContext) {
  let shell = group_shell(cx, &[A, B]);
  click_row(&shell, cx, A, 0);
  assert_eq!(selected_ids(&shell, cx, A), ["subtitles:3", "video:0"]);
  assert_eq!(selected_ids(&shell, cx, B), ["subtitles:3", "video:0"], "every file");
  in_table(&shell, cx, A, |window, cx| window.click("track-check-all", cx));
  assert_eq!(selected_ids(&shell, cx, B).len(), 5);
}

// The output path dialog.

/// The dialog's field, after a fresh frame.
fn dialog_value(shell: &Shell, cx: &mut TestAppContext) -> Option<String> {
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window
      .try_find("output-path")
      .and_then(|input| input.value().map(str::to_string))
  })
}

/// Replaces the focused dialog field's text and presses Enter.
fn type_and_confirm(shell: &Shell, cx: &mut TestAppContext, text: &str) {
  shell.window(cx, |window, cx| {
    window.press(SELECT_ALL, cx);
    window.press("backspace", cx);
    if !text.is_empty() {
      window.input(text, cx);
    }
  });
  cx.run_until_parked();
  shell.press(cx, "enter");
  cx.run_until_parked();
}

#[gpui_kit::test]
fn word_keys_stop_at_every_dot_of_the_output_path(cx: &mut TestAppContext) {
  use gpui_kit::component::WindowExt as _;

  let shell = shell(cx, &[A]);
  click_in(&shell, cx, 0, "card-output");
  assert!(shell.has_dialog(cx));
  shell.window(cx, |window, cx| {
    window.press(SELECT_ALL, cx);
    window.press("backspace", cx);
    window.input("Movie.2024.1080p.mkv", cx);
  });
  cx.run_until_parked();
  let word_left = if cfg!(target_os = "macos") {
    "alt-left"
  } else {
    "ctrl-left"
  };
  let caret = |shell: &Shell, cx: &mut TestAppContext| {
    shell.window(cx, |window, cx| match window.focused_input(cx) {
      Some(gpui_kit::component::input::AnyInputState::Input(state)) => state.read(cx).cursor(),
      _ => panic!("the dialog field has the focus"),
    })
  };
  let mut stops = Vec::new();
  for _ in 0..4 {
    shell.press(cx, word_left);
    stops.push(caret(&shell, cx));
  }
  assert_eq!(stops, [17, 11, 6, 0]);
}

#[gpui_kit::test]
fn the_output_dialog_sets_and_clears_a_cards_output_directory(cx: &mut TestAppContext) {
  let dir = TempDir::new("cards-output");
  let shell = shell(cx, &[A]);
  // Without a folder of its own: the session's (the shell sets one).
  click_in(&shell, cx, 0, "card-output");
  assert!(shell.has_dialog(cx));
  let session = std::env::temp_dir().to_string_lossy().into_owned();
  assert_eq!(dialog_value(&shell, cx), Some(session));
  let chosen = dir.path().join("out").to_string_lossy().into_owned();
  type_and_confirm(&shell, cx, &chosen);
  assert!(!shell.has_dialog(cx));
  assert_eq!(output_dir(&shell, cx, A), Some(chosen.clone()));
  assert!(
    texts(&shell, cx, 0)
      .iter()
      .any(|text| text.ends_with(&format!("To path {chosen}")))
  );
  // A relative folder is stored as the full path it names.
  click_in(&shell, cx, 0, "card-output");
  let relative = format!("bme-relative-{}", std::process::id());
  type_and_confirm(&shell, cx, &relative);
  let absolute = std::path::absolute(&relative).unwrap().to_string_lossy().into_owned();
  assert_eq!(output_dir(&shell, cx, A), Some(absolute));
  assert!(
    !std::path::Path::new(&relative).exists(),
    "created only by an extraction"
  );
  click_in(&shell, cx, 0, "card-output");
  type_and_confirm(&shell, cx, &chosen);
  // Reopened, it starts from the card's folder; empty clears it.
  click_in(&shell, cx, 0, "card-output");
  assert_eq!(dialog_value(&shell, cx).as_deref(), Some(chosen.as_str()));
  type_and_confirm(&shell, cx, "");
  assert_eq!(output_dir(&shell, cx, A), None);
  // Without a session folder either: the file's own folder.
  shell
    .stores
    .files
    .update(cx, |files, cx| files.set_global_output_dir(String::new(), cx));
  click_in(&shell, cx, 0, "card-output");
  assert_eq!(dialog_value(&shell, cx).as_deref(), Some(parent_dir(A)));
  shell.press(cx, "escape");
  assert!(!shell.has_dialog(cx), "Escape cancels");
  assert_eq!(output_dir(&shell, cx, A), None);
}

/// An edit while OK's writable check runs drops that check: the folder
/// stored is the one the field shows when OK is pressed again.
#[gpui_kit::test]
fn the_output_dialog_never_stores_a_folder_the_field_no_longer_shows(cx: &mut TestAppContext) {
  let dir = TempDir::new("cards-output-edited");
  let (first, second) = (
    dir.path().join("first").to_string_lossy().into_owned(),
    dir.path().join("second").to_string_lossy().into_owned(),
  );
  let shell = shell(cx, &[A]);
  click_in(&shell, cx, 0, "card-output");
  shell.window(cx, |window, cx| {
    window.press(SELECT_ALL, cx);
    window.press("backspace", cx);
    window.input(&first, cx);
    // OK, then an edit before the check's result arrives.
    window.press("enter", cx);
    window.press(SELECT_ALL, cx);
    window.press("backspace", cx);
    window.input(&second, cx);
  });
  cx.run_until_parked();
  assert!(shell.has_dialog(cx), "the check of the former text decided nothing");
  assert_eq!(output_dir(&shell, cx, A), None);
  shell.press(cx, "enter");
  cx.run_until_parked();
  assert!(!shell.has_dialog(cx));
  assert_eq!(output_dir(&shell, cx, A), Some(second));
}

#[gpui_kit::test]
fn the_output_dialog_warns_about_missing_and_refuses_unwritable_folders(cx: &mut TestAppContext) {
  let dir = TempDir::new("cards-output-checks");
  let blocker = dir.file("file.txt");
  let shell = shell(cx, &[A]);
  click_in(&shell, cx, 0, "card-output");
  let hint = tr!("extract.outputPathHint");
  let warning = tr!("extract.outputPathDoesNotExist");
  let error = tr!("extract.outputPathNotWritable");
  let messages = [hint.as_ref(), warning.as_ref(), error.as_ref()];
  let snapshot = |cx: &mut TestAppContext| {
    shell.window(cx, |window, cx| {
      window.render_frame(cx);
      let bounds = window.find("output-dialog").bounds();
      let shown: Vec<_> = gpui_kit::base::test_support::texts(window)
        .into_iter()
        .filter_map(|text| messages.iter().copied().find(|message| *message == text.text()))
        .collect();
      (bounds, shown)
    })
  };
  let (bounds, shown) = snapshot(cx);
  assert_eq!(shown, [hint.as_ref()]);

  let missing = dir.path().join("new").to_string_lossy().into_owned();
  shell.window(cx, |window, cx| {
    window.press(SELECT_ALL, cx);
    window.input(&missing, cx);
  });
  cx.executor().advance_clock(Duration::from_millis(400));
  cx.run_until_parked();
  assert_eq!(snapshot(cx), (bounds, vec![warning.as_ref()]));
  // Below a file nothing can be written: the dialog stays with an error.
  let blocked = format!("{blocker}/out");
  type_and_confirm(&shell, cx, &blocked);
  assert!(shell.has_dialog(cx));
  assert_eq!(snapshot(cx), (bounds, vec![error.as_ref()]));
  assert_eq!(output_dir(&shell, cx, A), None);
  // A missing folder is accepted: the extraction creates it.
  type_and_confirm(&shell, cx, &missing);
  assert!(!shell.has_dialog(cx));
  assert_eq!(output_dir(&shell, cx, A), Some(missing.clone()));
  assert!(!std::path::Path::new(&missing).exists(), "nothing created yet");
}

#[gpui_kit::test]
fn a_group_cards_output_directory_goes_to_every_file(cx: &mut TestAppContext) {
  let dir = TempDir::new("cards-group-output");
  let shell = group_shell(cx, &[A, B]);
  shell
    .stores
    .files
    .update(cx, |files, cx| files.set_global_output_dir(String::new(), cx));
  shell.resize(cx, 1400.0, 760.0);
  click_in(&shell, cx, 0, "group-output");
  assert_eq!(
    dialog_value(&shell, cx).as_deref(),
    Some(parent_dir(A)),
    "starts at the folder"
  );
  let out = dir.path().join("out").to_string_lossy().into_owned();
  type_and_confirm(&shell, cx, &out);
  for file in [A, B] {
    assert_eq!(output_dir(&shell, cx, file), Some(out.clone()));
  }
  assert!(
    texts(&shell, cx, 0)
      .iter()
      .any(|text| text.ends_with(&format!("To path {out}")))
  );
  click_in(&shell, cx, 0, "group-output");
  assert_eq!(dialog_value(&shell, cx), Some(out), "the shared folder");
  type_and_confirm(&shell, cx, "");
  assert_eq!(output_dir(&shell, cx, B), None);
  // With a session folder and no shared one, the dialog starts there.
  let session = dir.path().to_string_lossy().into_owned();
  shell
    .stores
    .files
    .update(cx, |files, cx| files.set_global_output_dir(session.clone(), cx));
  click_in(&shell, cx, 0, "group-output");
  assert_eq!(dialog_value(&shell, cx), Some(session));
}

#[gpui_kit::test]
fn the_toolbar_sets_the_global_output_directory(cx: &mut TestAppContext) {
  let dir = TempDir::new("cards-global-output");
  let shell = shell(cx, &[A]);
  shell
    .stores
    .files
    .update(cx, |files, cx| files.set_global_output_dir(String::new(), cx));
  assert!(!shell.disabled(cx, "toolbar-global-output"));
  shell.click(cx, "toolbar-global-output");
  shell.window(cx, |window, _| {
    let cancel = window.find("output-cancel").bounds();
    let ok = window.find("output-ok").bounds();
    assert_eq!(cancel.size.width, ok.size.width);
  });
  assert_eq!(dialog_value(&shell, cx).as_deref(), Some(""));
  let out = dir.path().to_string_lossy().into_owned();
  type_and_confirm(&shell, cx, &out);
  let global = |shell: &Shell, cx: &mut TestAppContext| {
    shell
      .stores
      .files
      .read_with(cx, |files, _| files.global_output_dir().map(str::to_string))
  };
  assert_eq!(global(&shell, cx), Some(out.clone()));
  // The extraction goes there; the card shows no path of its own.
  let request = cx.read(|cx| crate::actions::requests(&shell.stores, &s(&[A]), cx)[0].clone());
  assert_eq!(request.output_dir, out);
  assert!(!texts(&shell, cx, 0).iter().any(|text| text.contains("To path")));
  shell.click(cx, "toolbar-global-output");
  assert_eq!(dialog_value(&shell, cx), Some(out));
  type_and_confirm(&shell, cx, "");
  assert_eq!(global(&shell, cx), None);
}

#[gpui_kit::test]
fn better_media_info_shows_when_detected_and_a_failed_launch_is_told(cx: &mut TestAppContext) {
  let dir = TempDir::new("cards-bmi");
  let shell = group_shell(cx, &[A, B]);
  shell.stores.files.update(cx, |files, cx| {
    files.add_files(vec!["/media/other/c.mkv".to_string()], cx);
  });
  cx.run_until_parked();
  for (index, id) in [(0, "group-bmi"), (1, "card-bmi")] {
    assert_eq!(control(&shell, cx, index, id), None, "{id}");
  }
  // A launch from a folder without it fails with a message.
  let gone = dir.path().join("gone").to_string_lossy().into_owned();
  shell
    .stores
    .files
    .update(cx, |files, cx| files.set_better_media_info(Some(gone), cx));
  for (index, id) in [(0, "group-bmi"), (1, "card-bmi")] {
    assert_eq!(control(&shell, cx, index, id), Some(false), "{id}");
  }
  assert_action_sizes(&shell, cx, &["group-bmi"]);
  let toasts = shell.toasts(cx);
  click_in(&shell, cx, 0, "group-bmi");
  assert_eq!(shell.toasts(cx), toasts + 1);
}
