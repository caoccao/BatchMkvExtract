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

//! Headless tests of the Queue tab, the completion notifications, the
//! topmost window and the taskbar progress in the application shell.

use bme_core::extract::ExtractOutcome;
use core::prelude::v1::test;
use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::*;

use super::*;
use crate::domain::status::QueueItemStatus;
use crate::state::TabType;
use crate::ui::tests::{Shell, finished, movies, progress, started};

const A: &str = "/media/a.mkv";
const B: &str = "/media/b.mkv";
const C: &str = "/media/c.mkv";
const D: &str = "/media/d.mkv";
const E: &str = "/media/e.mkv";

/// The shell on the Queue tab with `files` listed and extracted (Extract
/// All).
fn queued(cx: &mut TestAppContext, files: &[&str]) -> Shell {
  let shell = Shell::with_files(cx, movies(files));
  shell.add_files(cx, files);
  shell.press(cx, "f3");
  shell
    .stores
    .ui
    .update(cx, |ui, cx| ui.set_active_tab(TabType::Queue, cx));
  cx.run_until_parked();
  shell
}

/// The texts painted inside the element `within` in a fresh frame.
fn texts(shell: &Shell, cx: &mut TestAppContext, within: impl Into<ElementId>) -> Vec<String> {
  let within = within.into();
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    gpui_kit::base::test_support::texts(window)
      .into_iter()
      .filter(|text| text.path().contains(&within))
      .map(|text| text.text().to_string())
      .collect()
  })
}

fn row_texts(shell: &Shell, cx: &mut TestAppContext, file: &str) -> Vec<String> {
  texts(shell, cx, QueueRow::element_id(file))
}

/// Whether `id` is on screen inside `within`, and its disabled state.
fn control(shell: &Shell, cx: &mut TestAppContext, within: impl Into<ElementId>, id: &str) -> Option<bool> {
  let (within, id) = (within.into(), SharedString::from(id.to_string()));
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window
      .within(within)
      .try_find(id)
      .map(|control| control.disabled() == Some(true))
  })
}

fn click(shell: &Shell, cx: &mut TestAppContext, id: &str) {
  let id = SharedString::from(id.to_string());
  shell.window(cx, |window, cx| {
    window.render_frame(cx);
    window.click(id, cx);
  });
  cx.run_until_parked();
}

/// The drive key of the test paths: `default` except on Windows, where a
/// rooted path without a drive letter has no prefix either.
fn drive() -> String {
  bme_core::extract::get_drive_key(A)
}

fn id(name: &str) -> String {
  format!("{name}:{}", drive())
}

fn statuses(shell: &Shell, cx: &mut TestAppContext) -> Vec<Option<QueueItemStatus>> {
  [A, B, C, D, E].iter().map(|file| shell.status(cx, file)).collect()
}

fn system_notifications(cx: &mut TestAppContext) -> Vec<(String, String)> {
  cx.shown_system_notifications()
    .into_iter()
    .map(|notification| (notification.title.to_string(), notification.body.to_string()))
    .collect()
}

fn topmost(shell: &Shell, cx: &mut TestAppContext) -> (usize, Option<AnyWindowHandle>) {
  shell
    .view
    .read_with(cx, |view, _| (view.topmost().opened(), view.topmost().handle()))
}

/// The texts of the topmost window, if it is open.
fn topmost_texts(cx: &mut TestAppContext, handle: AnyWindowHandle) -> Option<Vec<String>> {
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    gpui_kit::base::test_support::texts(window)
      .into_iter()
      .map(|text| text.text().to_string())
      .collect()
  })
  .ok()
}

#[gpui_kit::test]
fn an_empty_queue_shows_just_the_table_headers_and_the_toolbar_has_the_queue_actions(cx: &mut TestAppContext) {
  let shell = Shell::with_files(cx, movies(&[]));
  shell
    .stores
    .ui
    .update(cx, |ui, cx| ui.set_active_tab(TabType::Queue, cx));
  cx.run_until_parked();
  assert_eq!(
    texts(&shell, cx, "queue"),
    ["File Path", "Status", "Start", "End", "Elapsed", "ETA"]
  );
  shell.window(cx, |window, _| {
    let table = window.find("queue-drive:").bounds();
    assert_eq!(table, window.find("queue").bounds());
    let bounds = table.scale(window.scale_factor());
    assert!(
      !window
        .painted_quads()
        .iter()
        .any(|quad| { quad.bounds == bounds && (quad.border_widths.left.0 > 0.0 || quad.border_widths.right.0 > 0.0) })
    );
  });
  // The toggles are always available; the queue actions are not.
  for toggle in [
    "toolbar-quit-when-complete",
    "toolbar-system-notification",
    "toolbar-topmost-notification",
  ] {
    assert_eq!(control(&shell, cx, "toolbar", toggle), Some(false), "{toggle}");
  }
  for action in ["toolbar-resume-all", "toolbar-cancel-all", "toolbar-clear-completed"] {
    assert_eq!(control(&shell, cx, "toolbar", action), Some(true), "{action}");
  }
  shell.resize(cx, 600.0, 450.0);
  shell.window(cx, |window, _| {
    let toolbar = window.within("toolbar");
    let bounds: Vec<_> = [
      "toolbar-global-output",
      "toolbar-extract-all",
      "toolbar-resume-all",
      "toolbar-cancel-all",
      "toolbar-clear-completed",
      "toolbar-clear",
      "toolbar-quit-when-complete",
      "toolbar-system-notification",
      "toolbar-topmost-notification",
      "toolbar-group-mode",
      "toolbar-profile",
      "toolbar-settings",
      "toolbar-about",
      "toolbar-overflow",
    ]
    .into_iter()
    .filter_map(|id| toolbar.try_find(id).map(|button| button.bounds()))
    .collect();
    assert_eq!(bounds.last(), Some(&toolbar.find("toolbar-overflow").bounds()));
    assert!(toolbar.try_find("toolbar-about").is_none());
    assert!(
      bounds
        .windows(2)
        .all(|pair| pair[0].top() == pair[1].top() && pair[1].left() - pair[0].right() >= px(2.0))
    );
    let last_button = bounds[bounds.len() - 2];
    assert_eq!(
      toolbar.find("toolbar-overflow").bounds().left() - last_button.right(),
      px(2.0)
    );
    let root = window.find("app-root").bounds();
    assert!(
      bounds
        .iter()
        .all(|button| button.left() >= root.left() && button.right() <= root.right())
    );
  });
}

#[gpui_kit::test]
fn rows_show_each_status_with_its_label_and_times(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B, C, D, E]);
  shell.send(cx, started(A));
  shell.send(cx, progress(A, 17));
  for (file, outcome, error) in [
    (B, ExtractOutcome::Completed, None),
    (C, ExtractOutcome::Failed, Some("mkvextract exited with code 2")),
    (D, ExtractOutcome::Cancelled, None),
  ] {
    shell.send(cx, started(file));
    shell.send(cx, finished(file, outcome, error));
  }
  let shown = texts(&shell, cx, id("queue-drive"));
  assert_eq!(shown.first().map(String::as_str), Some("File Path"));
  let expected = [
    (A, "Extracting 17%"),
    (B, "Completed"),
    (C, "Failed"),
    (D, "Cancelled"),
    (E, "Waiting"),
  ];
  for (file, status) in expected {
    let row = row_texts(&shell, cx, file);
    assert_eq!(row[0], file, "{row:?}");
    assert_eq!(row[1], status, "{row:?}");
  }
  // A waiting row knows no time; an extracting one has a start but no end
  // or ETA before its progress means anything; a finished one has both ends.
  assert_eq!(row_texts(&shell, cx, E)[2..], ["--:--:--"; 4]);
  let extracting = row_texts(&shell, cx, A);
  assert_ne!(extracting[2], "--:--:--", "a start");
  assert_eq!(extracting[3], "--:--:--", "no end");
  let completed = row_texts(&shell, cx, B);
  assert!(completed[2..5].iter().all(|time| time != "--:--:--"), "{completed:?}");
  assert_eq!(completed[5], "--:--:--", "no ETA once finished");
  for file in [A, E] {
    let row = QueueRow::element_id(file);
    assert_eq!(control(&shell, cx, row.clone(), "queue-cancel"), Some(false));
    assert_eq!(control(&shell, cx, row.clone(), "queue-resume"), None);
    shell.window(cx, |window, cx| window.within(row.clone()).click("queue-cancel", cx));
    assert_eq!(control(&shell, cx, row, "queue-cancel"), Some(true));
  }
  assert_eq!(shell.cancelled(), [A, E]);

  for file in [B, C, D] {
    let row = QueueRow::element_id(file);
    assert_eq!(control(&shell, cx, row.clone(), "queue-cancel"), None);
    assert_eq!(control(&shell, cx, row, "queue-resume"), (file != B).then_some(false));
  }
  shell.window(cx, |window, cx| {
    window.within(QueueRow::element_id(C)).click("queue-resume", cx)
  });
  // The arguments are built in the background.
  cx.run_until_parked();
  assert_eq!(shell.enqueued(), [A, B, C, D, E, C]);
  assert_eq!(shell.status(cx, C), Some(QueueItemStatus::Waiting));
  assert_eq!(shell.status(cx, D), Some(QueueItemStatus::Cancelled));
}

#[gpui_kit::test]
fn a_rows_path_can_be_selected_and_copied_without_editing(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A]);
  let header = shell.window(cx, |window, cx| {
    window.render_frame(cx);
    gpui_kit::base::test_support::texts(window)
      .into_iter()
      .find(|text| text.text() == "File Path")
      .expect("the File Path header")
      .bounds()
  });
  let path = shell
    .view
    .read_with(cx, |view, cx| {
      let row = view.main().read(cx).queue().read(cx).row(A).cloned();
      row.and_then(|row| row.read(cx).path_state().cloned())
    })
    .expect("the row drew its path");
  // The text starts where the header's label does, inside the cell padding.
  shell.window(cx, |_, cx| {
    let text = path.read(cx).text_bounds().expect("path text was laid out");
    assert_eq!(text.left(), header.left() + px(8.0));
  });
  shell.click(cx, "queue-path");
  let select_all = if cfg!(target_os = "macos") { "cmd-a" } else { "ctrl-a" };
  shell.press(cx, select_all);
  shell.window(cx, |window, cx| {
    assert!(path.read(cx).focus_handle(cx).is_focused(window));
    assert_eq!(path.read(cx).selected_range(), 0..A.len());
  });
  let copy = if cfg!(target_os = "macos") { "cmd-c" } else { "ctrl-c" };
  shell.press(cx, copy);
  assert_eq!(
    cx.read(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
      .as_deref(),
    Some(A)
  );
  shell.window(cx, |window, cx| window.input("edited", cx));
  shell.press(cx, "backspace");
  assert_eq!(path.read_with(cx, |path, _| path.value()).as_ref(), A);
}

#[gpui_kit::test]
fn one_table_per_drive(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B, C]);
  shell.stores.queue.update(cx, |queue, cx| {
    queue.move_to_drive(B, "D:", cx);
    queue.move_to_drive(C, "\\\\SERVER\\SHARE", cx);
  });
  cx.run_until_parked();
  let shown = texts(&shell, cx, "queue");
  assert!(!shown.iter().any(|text| text == "Queue"));
  for title in ["D:", "\\\\SERVER\\SHARE"] {
    assert_eq!(
      shown.iter().filter(|text| **text == title).count(),
      1,
      "{title} in {shown:?}"
    );
  }
  for (drive, file) in [
    (drive(), A),
    ("D:".to_string(), B),
    ("\\\\SERVER\\SHARE".to_string(), C),
  ] {
    let table = texts(&shell, cx, format!("queue-drive:{drive}"));
    assert!(table.iter().any(|text| text == file), "{file} in {table:?}");
    assert_eq!(table.iter().filter(|text| text.starts_with("/media/")).count(), 1);
  }
  click(&shell, cx, "toolbar-cancel-all");
  assert_eq!(shell.cancelled(), [A, B, C]);
}

#[gpui_kit::test]
fn resume_all_extracts_again_only_cancelled_and_failed_files(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B, C, D]);
  assert_eq!(control(&shell, cx, "toolbar", "toolbar-resume-all"), Some(true));
  shell
    .stores
    .queue
    .update(cx, |queue, cx| queue.move_to_drive(C, "D:", cx));
  shell.send(cx, started(A));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  shell.send(cx, finished(B, ExtractOutcome::Cancelled, None));
  shell.send(cx, started(C));
  shell.send(cx, finished(C, ExtractOutcome::Failed, Some("boom")));
  shell.send(cx, started(D));
  shell.press(cx, "ctrl-1");
  assert_eq!(shell.active_tab(cx), TabType::FileList);
  assert_eq!(control(&shell, cx, "toolbar", "toolbar-resume-all"), Some(false));
  click(&shell, cx, "toolbar-resume-all");
  assert_eq!(
    shell.enqueued(),
    [A, B, C, D, B, C],
    "only B and C again, in queue order"
  );
  assert_eq!(
    statuses(&shell, cx)[..4],
    [
      Some(QueueItemStatus::Completed),
      Some(QueueItemStatus::Waiting),
      Some(QueueItemStatus::Waiting),
      Some(QueueItemStatus::Extracting),
    ]
  );
  // A file no longer listed is not resumed.
  shell.send(cx, finished(B, ExtractOutcome::Cancelled, None));
  cx.update(|cx| crate::actions::remove_file(&shell.stores, B, cx));
  shell.send(cx, finished(B, ExtractOutcome::Cancelled, None));
  click(&shell, cx, "toolbar-resume-all");
  assert_eq!(shell.enqueued().len(), 6, "B is gone, C still waits");
}

#[gpui_kit::test]
fn clear_completed_removes_only_finished_rows(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B, C, D]);
  assert_eq!(control(&shell, cx, "toolbar", "toolbar-clear-completed"), Some(true));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  shell.send(cx, finished(B, ExtractOutcome::Failed, Some("boom")));
  shell.send(cx, finished(C, ExtractOutcome::Cancelled, None));
  click(&shell, cx, "toolbar-clear-completed");
  assert_eq!(
    statuses(&shell, cx)[..4],
    [None, None, None, Some(QueueItemStatus::Waiting)]
  );
  let shown = texts(&shell, cx, "queue");
  assert!(!shown.iter().any(|text| text == A), "{shown:?}");
  assert!(shown.iter().any(|text| text == D), "{shown:?}");
  assert_eq!(control(&shell, cx, "toolbar", "toolbar-clear-completed"), Some(true));
}

/// With both notifications on, three queued extractions send one OS notification
/// and open the topmost window once, after the third finishes; the in-app
/// toast stays per file.
#[gpui_kit::test]
fn three_extractions_notify_once_after_the_last(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B, C]);
  click(&shell, cx, "toolbar-system-notification");
  click(&shell, cx, "toolbar-topmost-notification");
  shell.stores.ui.read_with(cx, |ui, _| {
    assert!(ui.system_notification() && ui.topmost_notification());
  });
  shell.send(cx, started(A));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  shell.send(cx, started(B));
  shell.send(cx, finished(B, ExtractOutcome::Failed, Some("boom")));
  assert_eq!(system_notifications(cx), [], "two of three finished");
  assert_eq!(topmost(&shell, cx), (0, None));
  shell.send(cx, started(C));
  shell.send(cx, finished(C, ExtractOutcome::Completed, None));
  assert_eq!(
    system_notifications(cx),
    [(
      "Extraction complete.".to_string(),
      "2 completed, 1 failed, 0 cancelled\nFinished in 00:00:00".to_string()
    )]
  );
  let (opened, handle) = topmost(&shell, cx);
  assert_eq!(opened, 1);
  let handle = handle.expect("the topmost window");
  assert_eq!(
    topmost_texts(cx, handle).expect("open")[..3],
    [
      "Extraction complete.",
      "2 completed, 1 failed, 0 cancelled",
      "Finished in 00:00:00"
    ]
  );
  assert_eq!(shell.toasts(cx), 3, "a toast per finished file");

  // The next batch of one file reuses the open window with the single-file
  // text.
  click(&shell, cx, "toolbar-resume-all");
  shell.send(cx, started(B));
  shell.send(cx, finished(B, ExtractOutcome::Completed, None));
  assert_eq!(system_notifications(cx).len(), 2);
  assert_eq!(topmost(&shell, cx), (1, Some(handle)), "reused");
  let shown = topmost_texts(cx, handle).expect("open");
  assert_eq!(shown[..3], ["Extraction complete.", B, "Completed in 00:00:00"]);

  // Escape closes it; the next completion opens a new one.
  cx.update_window(handle, |_, window, cx| {
    window.render_frame(cx);
    assert!(window.try_find("topmost-keep-open").is_none());
    window.press("escape", cx);
  })
  .unwrap();
  cx.run_until_parked();
  assert!(topmost_texts(cx, handle).is_none(), "closed");
  shell.press(cx, "f3");
  for file in [A, B, C] {
    shell.send(cx, started(file));
    shell.send(cx, finished(file, ExtractOutcome::Completed, None));
  }
  let (opened, reopened) = topmost(&shell, cx);
  assert_eq!(opened, 2);
  assert_ne!(reopened, Some(handle));
}

#[gpui_kit::test]
fn nothing_notifies_with_the_toggles_off_or_a_cancelled_batch(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B]);
  shell.send(cx, started(A));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  shell.send(cx, finished(B, ExtractOutcome::Completed, None));
  assert_eq!(system_notifications(cx), [], "both toggles are off");
  assert_eq!(topmost(&shell, cx).0, 0);
  click(&shell, cx, "toolbar-system-notification");
  click(&shell, cx, "toolbar-topmost-notification");
  click(&shell, cx, "toolbar-resume-all");
  assert_eq!(shell.enqueued().len(), 2, "nothing to resume");
  click(&shell, cx, "toolbar-quit-when-complete");
  shell.press(cx, "f3");
  shell.send(cx, finished(A, ExtractOutcome::Cancelled, None));
  shell.send(cx, finished(B, ExtractOutcome::Cancelled, None));
  assert_eq!(system_notifications(cx), [], "the user cancelled everything");
  assert_eq!(topmost(&shell, cx).0, 0);
  assert!(cx.update_window(shell.handle, |_, _, _| ()).is_ok());
}

#[gpui_kit::test]
fn quit_when_complete_closes_the_main_window_after_the_last_job(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B]);
  click(&shell, cx, "toolbar-quit-when-complete");
  click(&shell, cx, "toolbar-system-notification");
  shell.send(cx, started(A));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  assert!(cx.update_window(shell.handle, |_, _, _| ()).is_ok());
  assert_eq!(system_notifications(cx), []);
  shell.send(cx, started(B));
  shell.send(cx, finished(B, ExtractOutcome::Completed, None));
  assert!(cx.update_window(shell.handle, |_, _, _| ()).is_err());
  assert_eq!(system_notifications(cx).len(), 1);
}

#[gpui_kit::test]
fn topmost_completion_lets_the_user_keep_the_app_open_or_quit(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A]);
  click(&shell, cx, "toolbar-quit-when-complete");
  click(&shell, cx, "toolbar-topmost-notification");
  shell.send(cx, started(A));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  let notification = topmost(&shell, cx).1.expect("topmost notification");
  assert!(cx.update_window(shell.handle, |_, _, _| ()).is_ok());
  cx.update_window(notification, |_, window, cx| {
    window.render_frame(cx);
    window.find_by_label("OK, but do not quit");
    window.click("topmost-keep-open", cx);
  })
  .unwrap();
  cx.run_until_parked();
  assert!(topmost_texts(cx, notification).is_none());
  assert!(cx.update_window(shell.handle, |_, _, _| ()).is_ok());
  assert!(shell.stores.ui.read_with(cx, |ui, _| ui.quit_when_complete()));

  shell.press(cx, "f3");
  shell.send(cx, started(A));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  let notification = topmost(&shell, cx).1.expect("topmost notification");
  cx.update_window(notification, |_, window, cx| {
    window.render_frame(cx);
    window.find_by_label("Quit");
    window.click("topmost-close", cx);
  })
  .unwrap();
  cx.run_until_parked();
  assert!(topmost_texts(cx, notification).is_none());
  assert!(cx.update_window(shell.handle, |_, _, _| ()).is_err());
}

#[gpui_kit::test]
fn closing_mid_extraction_sends_no_completion(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B]);
  click(&shell, cx, "toolbar-system-notification");
  click(&shell, cx, "toolbar-topmost-notification");
  shell.send(cx, started(A));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  shell.send(cx, started(B));
  let stores = shell.stores.clone();
  shell.window(cx, |window, cx| {
    crate::platform::window::close_requested(&stores, window, cx)
  });
  cx.run_until_parked();
  click(&shell, cx, "exit-yes");
  assert_eq!(shell.cancelled(), [B]);
  shell.send(cx, finished(B, ExtractOutcome::Cancelled, None));
  assert_eq!(system_notifications(cx), []);
  assert_eq!(topmost(&shell, cx).0, 0);
}

#[gpui_kit::test]
fn the_taskbar_follows_the_whole_queue(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B]);
  shell.send(cx, started(A));
  shell.send(cx, progress(A, 50));
  shell.send(cx, finished(A, ExtractOutcome::Completed, None));
  shell.send(cx, started(B));
  shell.send(cx, progress(B, 40));
  shell.send(cx, progress(B, 41));
  shell.send(cx, finished(B, ExtractOutcome::Failed, Some("boom")));
  // Each value once: 0 while both wait, the mean of both progresses, then
  // hidden.
  assert_eq!(
    shell.taskbar.borrow().as_slice(),
    [Some(0), Some(25), Some(50), Some(70), None]
  );
}

#[gpui_kit::test]
fn only_the_extracting_row_repaints(cx: &mut TestAppContext) {
  let shell = queued(cx, &[A, B]);
  let draw = |cx: &mut TestAppContext| shell.window(cx, |window, cx| window.draw(cx).clear(cx));
  let rows = shell.view.read_with(cx, |view, cx| {
    let queue = view.main().read(cx).queue().read(cx);
    (queue.row(A).cloned().unwrap(), queue.row(B).cloned().unwrap())
  });
  let counts = |cx: &mut TestAppContext| cx.read(|cx| (rows.0.read(cx).render_count(), rows.1.read(cx).render_count()));
  draw(cx);
  draw(cx);
  let before = counts(cx);
  draw(cx);
  assert_eq!(counts(cx), before, "an idle frame reuses both rows");
  shell.send(cx, started(B));
  draw(cx);
  draw(cx);
  for event in [progress(B, 10), progress(B, 20)] {
    let (a_before, b_before) = counts(cx);
    shell.send(cx, event);
    draw(cx);
    draw(cx);
    let (a_after, b_after) = counts(cx);
    assert!(b_after > b_before, "the extracting row repaints");
    assert_eq!(a_after, a_before, "the waiting row does not");
  }
  // The clock tick repaints only the extracting row too.
  let (a_before, b_before) = counts(cx);
  cx.executor().advance_clock(crate::state::queue::TICK);
  cx.run_until_parked();
  draw(cx);
  let (a_after, b_after) = counts(cx);
  assert!(b_after > b_before);
  assert_eq!(a_after, a_before);
}
