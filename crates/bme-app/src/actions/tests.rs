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

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Cursor;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use bme_core::config::{Config, ConfigProfile};
use bme_core::extract::{ExtractEvent, ExtractOutcome, ExtractProcess, ExtractProcessControl, ExtractQueue};
use core::prelude::v1::test;
use gpui_kit::{Entity, TestAppContext};

use super::*;
use crate::extract_events::{self, ExtractBackend};
use crate::platform::taskbar::Taskbar;
use crate::state::files::TableScope;
use crate::state::files::tests::{TempDir, all, clip, fixture_source, movie, s};
use crate::state::ui::{UiEvent, UiState};
use crate::state::{Persistence, Services};
use crate::tools::{ToolDetector, ToolStatus};

/// The default config with Group Mode off unless asked.
fn config(group_mode: bool) -> Config {
  let mut config = Config::default();
  config.profiles[0].default_group_mode = group_mode;
  config
}

/// What a recording extract backend was asked to do.
#[derive(Default)]
struct Calls {
  enqueued: Vec<(String, Vec<String>)>,
  cancelled: Vec<String>,
}

impl Calls {
  fn files(&self) -> Vec<String> {
    self.enqueued.iter().map(|(file, _)| file.clone()).collect()
  }
}

/// A backend that records every call; nothing reports extract events but
/// the test.
fn recording_backend() -> (ExtractBackend, Arc<Mutex<Calls>>) {
  let calls = Arc::new(Mutex::new(Calls::default()));
  let (enqueued, cancelled) = (calls.clone(), calls.clone());
  let backend = ExtractBackend::new(
    move |file, args| {
      enqueued.lock().unwrap().enqueued.push((file, args));
      Ok(())
    },
    move |file| {
      cancelled.lock().unwrap().cancelled.push(file.to_string());
      Ok(())
    },
  );
  (backend, calls)
}

fn stores_with(
  cx: &mut TestAppContext,
  config: Config,
  fixtures: HashMap<String, Vec<bme_core::protocol::MkvTrack>>,
  extracts: ExtractBackend,
  tools: ToolDetector,
) -> Stores {
  let (tracks, _) = fixture_source(fixtures);
  cx.update(|cx| {
    let services = Services {
      persistence: Persistence::new(|_| {}),
      tracks,
      extracts,
      taskbar: Taskbar::none(),
      tools,
    };
    Stores::new(config, services, cx)
  })
}

/// Stores over `fixtures` whose backend records every call.
fn recording_stores(
  cx: &mut TestAppContext,
  config: Config,
  fixtures: HashMap<String, Vec<bme_core::protocol::MkvTrack>>,
) -> (Stores, Arc<Mutex<Calls>>) {
  let (backend, calls) = recording_backend();
  (
    stores_with(cx, config, fixtures, backend, ToolDetector::system()),
    calls,
  )
}

/// Adds `files` and lets them load. The fixtures' folders do not exist, so
/// the outputs go to the system's temporary directory, which does.
fn add(stores: &Stores, cx: &mut TestAppContext, files: &[&str]) {
  stores.files.update(cx, |store, cx| {
    store.add_files(s(files), cx);
    if store.global_output_dir().is_none() {
      store.set_global_output_dir(std::env::temp_dir().to_string_lossy().into_owned(), cx);
    }
  });
  cx.run_until_parked();
}

fn run(cx: &mut TestAppContext, f: impl FnOnce(&mut App) -> Task<()>) {
  cx.update(|cx| f(cx).detach());
  cx.run_until_parked();
}

fn status(stores: &Stores, cx: &mut TestAppContext, file: &str) -> Option<QueueItemStatus> {
  stores
    .queue
    .read_with(cx, |queue, _| queue.item(file).map(|item| item.status))
}

/// What the extract queue reports.
fn report(stores: &Stores, cx: &mut TestAppContext, event: ExtractEvent) {
  let now = SystemTime::now();
  stores.queue.update(cx, |queue, cx| queue.apply_event(&event, now, cx));
}

fn started(file: &str) -> ExtractEvent {
  ExtractEvent::Started {
    file: file.to_string(),
    at: SystemTime::now(),
  }
}

fn finished(file: &str, outcome: ExtractOutcome) -> ExtractEvent {
  ExtractEvent::Finished {
    file: file.to_string(),
    outcome,
    error: None,
    warnings: Vec::new(),
    at: SystemTime::now(),
  }
}

/// The snackbars and toasts `ui` shows from now on.
fn shown(cx: &mut TestAppContext, ui: &Entity<UiState>) -> (Rc<RefCell<Vec<String>>>, gpui_kit::Subscription) {
  let messages = Rc::new(RefCell::new(Vec::new()));
  let sink = messages.clone();
  let subscription = cx.update(|cx| {
    cx.subscribe(ui, move |_, event: &UiEvent, _| match event {
      UiEvent::Snackbar(snackbar) => sink.borrow_mut().push(snackbar.message.clone()),
      UiEvent::Toast(toast) => sink.borrow_mut().push(format!("{}: {}", toast.file, toast.detail)),
      _ => {}
    })
  });
  (messages, subscription)
}

#[test]
fn a_request_reads_the_selection_and_the_output_directory() {
  let mut files = crate::state::FileStore::with_profile(None, fixture_source(HashMap::new()).0);
  let profile = ConfigProfile::named(ConfigProfile::DEFAULT_NAME);
  assert!(
    ExtractRequest::of(&files, &profile, "/m/a.mkv").is_none(),
    "nothing selected"
  );
  let a = "/m/a.mkv";
  files.load_for_tests(a, movie(), &["audio:2", "video:0"]);
  let request = ExtractRequest::of(&files, &profile, a).expect("a selection");
  assert_eq!(
    request.tracks,
    [movie()[0].clone(), movie()[2].clone()],
    "in table order"
  );
  assert_eq!(request.output_dir, "/m", "the file's folder");
  let out = |name: &str| bme_core::extract_args::join_output_path("/m", name);
  assert_eq!(
    request.args(),
    [
      "tracks".to_string(),
      format!("0:{}", out("a.0.und.h264")),
      format!("2:{}", out("a.2.ger.ac3")),
    ]
  );
  assert!(request.command("/opt/mkvtoolnix").contains("a.2.ger.ac3"));
}

/// Ends a fake `mkvextract` at once with exit code 0.
struct FinishedProcess;

impl ExtractProcessControl for FinishedProcess {
  fn kill(&mut self) -> std::io::Result<()> {
    Ok(())
  }

  fn try_wait(&mut self) -> std::io::Result<Option<Option<i32>>> {
    Ok(Some(Some(0)))
  }
}

/// Extract All through `bme_core`'s queue on its own worker threads, with a
/// fake `mkvextract` that prints two progress lines: every file ends
/// completed, one at a time in list order, into the output directory the
/// action created. GPUI's test scheduler rejects a foreground task woken
/// from another thread, so the test thread relays the workers' events into
/// the app's channel.
#[gpui_kit::test]
fn extract_all_runs_through_the_core_queue(cx: &mut TestAppContext) {
  let files = ["/m/a.mkv", "/m/b.mkv", "/m/c.mkv"];
  let out = TempDir::new("extract-all-out");
  let target = out.path().join("nested").to_string_lossy().into_owned();
  let launched = Arc::new(Mutex::new(Vec::<(String, Vec<String>)>::new()));
  let recorder = launched.clone();
  let (sink, events) = extract_events::channel();
  let (relay, relayed) = std::sync::mpsc::channel();
  let relay = Mutex::new(relay);
  let queue = ExtractQueue::new(
    Box::new(move |file, args| {
      recorder.lock().unwrap().push((file.to_string(), args.to_vec()));
      Ok(ExtractProcess {
        output: Some(Box::new(Cursor::new(
          b"#GUI#progress 30%\n#GUI#progress 100%\n".to_vec(),
        ))),
        control: Box::new(FinishedProcess),
      })
    }),
    Box::new(move |event| {
      let _ = relay.lock().unwrap().send(event);
    }),
  );
  let stores = stores_with(
    cx,
    config(false),
    all(&files, movie),
    ExtractBackend::queue(queue),
    ToolDetector::system(),
  );
  cx.update(|cx| extract_events::start(&stores, events, cx));
  add(&stores, cx, &files);
  stores
    .files
    .update(cx, |store, cx| store.set_global_output_dir(target.clone(), cx));
  assert!(cx.update(|cx| can_extract_all(&stores, cx)));
  run(cx, |cx| extract_all(&stores, cx));
  assert!(
    std::path::Path::new(&target).is_dir(),
    "the output directory was created"
  );
  let deadline = Instant::now() + Duration::from_secs(10);
  loop {
    while let Ok(event) = relayed.try_recv() {
      sink(event);
    }
    cx.run_until_parked();
    let done = stores.queue.read_with(cx, |queue, _| {
      queue.items().count() == 3 && queue.items().all(|item| item.status == QueueItemStatus::Completed)
    });
    if done {
      break;
    }
    assert!(Instant::now() < deadline, "the extractions did not finish");
    std::thread::sleep(Duration::from_millis(5));
  }
  let launched = launched.lock().unwrap();
  let order: Vec<&str> = launched.iter().map(|(file, _)| file.as_str()).collect();
  assert_eq!(order, files, "one drive extracts one file at a time, in order");
  let expected = bme_core::extract_args::join_output_path(&target, "a.3.eng.srt");
  assert_eq!(launched[0].1, ["tracks".to_string(), format!("3:{expected}")]);
  stores.queue.read_with(cx, |queue, _| {
    assert!(
      queue
        .items()
        .all(|item| item.progress == 100 && item.extract_ended_at.is_some())
    );
    assert!(!queue.has_active());
  });
}

/// Extract All goes in the order the file list shows: a group card's files
/// where the group is, not in the order they were added.
#[gpui_kit::test]
fn extract_all_follows_the_file_list_order(cx: &mut TestAppContext) {
  let (a, b, c) = ("/m/a.mkv", "/n/b.mkv", "/m/c.mkv");
  let (stores, calls) = recording_stores(cx, config(true), all(&[a, b, c], movie));
  add(&stores, cx, &[a, b, c]);
  run(cx, |cx| extract_all(&stores, cx));
  assert_eq!(calls.lock().unwrap().files(), [a, c, b]);
  assert!(cx.update(|cx| stores.has_active_extracts(cx)));
  assert_eq!(status(&stores, cx, b), Some(QueueItemStatus::Waiting));
}

/// Only files with a selection are queued, and a waiting or extracting file
/// is not queued again; a finished one is.
#[gpui_kit::test]
fn extract_skips_files_without_a_selection_or_in_the_queue(cx: &mut TestAppContext) {
  let (a, b, video) = ("/m/a.mkv", "/m/b.mkv", "/m/video.mkv");
  let fixtures = HashMap::from([(a.into(), movie()), (b.into(), movie()), (video.into(), clip())]);
  let (stores, calls) = recording_stores(cx, config(false), fixtures);
  add(&stores, cx, &[a, b, video]);
  // The Default profile selects no video track.
  assert!(
    stores
      .files
      .read_with(cx, |files, _| files.selected_ids(video).unwrap().is_empty())
  );
  run(cx, |cx| extract(&stores, &s(&[a, video]), cx));
  assert_eq!(calls.lock().unwrap().files(), [a]);
  // Twice at once: the second finds it waiting.
  cx.update(|cx| {
    extract(&stores, &s(&[b]), cx).detach();
    extract(&stores, &s(&[b]), cx).detach();
  });
  cx.run_until_parked();
  run(cx, |cx| extract_all(&stores, cx));
  assert_eq!(calls.lock().unwrap().files(), [a, b]);
  report(&stores, cx, started(a));
  report(&stores, cx, finished(a, ExtractOutcome::Completed));
  run(cx, |cx| extract_all(&stores, cx));
  assert_eq!(calls.lock().unwrap().files(), [a, b, a], "a finished file can go again");
}

/// An output directory that cannot be created is named in a toast, and that
/// file is not queued; the others are.
#[gpui_kit::test]
fn an_output_directory_that_cannot_be_created_queues_nothing(cx: &mut TestAppContext) {
  let _locale = crate::i18n::tests::LOCALE_LOCK
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  crate::i18n::set_language(bme_core::config::Language::EnUS);
  let dir = TempDir::new("blocked-output");
  let blocked = dir.file("not-a-directory");
  let (a, b) = ("/m/a.mkv", "/m/b.mkv");
  let (stores, calls) = recording_stores(cx, config(false), all(&[a, b], movie));
  let (messages, _subscription) = shown(cx, &stores.ui);
  add(&stores, cx, &[a, b]);
  stores
    .files
    .update(cx, |files, cx| files.set_output_dir(&s(&[a]), &blocked, cx));
  run(cx, |cx| extract_all(&stores, cx));
  assert_eq!(calls.lock().unwrap().files(), [b]);
  assert_eq!(status(&stores, cx, a), None);
  assert_eq!(
    *messages.borrow(),
    [format!(
      "{a}: Failed to create {blocked}. Please check it out and try again."
    )]
  );
}

#[gpui_kit::test]
fn a_failing_enqueue_takes_the_item_off_the_queue(cx: &mut TestAppContext) {
  let a = "/m/a.mkv";
  let backend = ExtractBackend::new(|_, _| Err(anyhow::anyhow!("no queue")), |_| Ok(()));
  let stores = stores_with(cx, config(false), all(&[a], movie), backend, ToolDetector::system());
  add(&stores, cx, &[a]);
  run(cx, |cx| extract_all(&stores, cx));
  assert_eq!(status(&stores, cx, a), None);
  assert!(!cx.update(|cx| stores.has_active_extracts(cx)));
}

#[gpui_kit::test]
fn cancel_requests_reach_the_queue_and_clear_waits_for_the_extractions(cx: &mut TestAppContext) {
  let (a, b) = ("/m/a.mkv", "/m/b.mkv");
  let (stores, calls) = recording_stores(cx, config(false), all(&[a, b], movie));
  add(&stores, cx, &[a, b]);
  run(cx, |cx| extract_all(&stores, cx));
  report(&stores, cx, started(a));
  cx.update(|cx| {
    assert!(!can_clear(&stores, cx));
    clear_files(&stores, cx);
    assert!(
      stores.files.read(cx).has_files(),
      "nothing is cleared while extractions run"
    );
    cancel_all(&stores, cx);
    assert!(stores.queue.read(cx).item(b).unwrap().cancel_requested);
  });
  assert_eq!(calls.lock().unwrap().cancelled, [a, b]);
  report(&stores, cx, finished(a, ExtractOutcome::Cancelled));
  report(&stores, cx, finished(b, ExtractOutcome::Cancelled));
  cx.update(|cx| {
    assert!(can_clear(&stores, cx));
    clear_files(&stores, cx);
    assert!(!stores.files.read(cx).has_files());
    assert_eq!(stores.queue.read(cx).items().count(), 0, "Clear empties the queue too");
  });
}

/// Delete on a waiting file cancels it; the extract queue's late `Cancelled`
/// lists nothing again, and the extraction counts as active until it. An
/// extracting file is not removed, by Delete or by a group's Clear All.
#[gpui_kit::test]
fn delete_cancels_a_waiting_file_and_keeps_an_extracting_one(cx: &mut TestAppContext) {
  let (a, b, c) = ("/m/a.mkv", "/m/b.mkv", "/m/c.mkv");
  let (stores, calls) = recording_stores(cx, config(false), all(&[a, b, c], movie));
  add(&stores, cx, &[a, b, c]);
  run(cx, |cx| extract(&stores, &s(&[a, b]), cx));
  report(&stores, cx, started(a));
  cx.update(|cx| {
    assert!(!remove_file(&stores, a, cx), "extracting");
    assert!(remove_file(&stores, b, cx));
    assert!(stores.has_active_extracts(cx));
  });
  assert_eq!(calls.lock().unwrap().cancelled, [b]);
  assert_eq!(status(&stores, cx, b), None, "hidden at once");
  report(&stores, cx, finished(b, ExtractOutcome::Cancelled));
  assert_eq!(status(&stores, cx, b), None, "its end lists nothing");
  cx.update(|cx| clear_group(&stores, &s(&[a, c]), cx));
  stores.files.read_with(cx, |files, _| {
    assert_eq!(files.files(), [a], "the extracting file stays")
  });
  report(&stores, cx, finished(a, ExtractOutcome::Completed));
  cx.update(|cx| assert!(remove_file(&stores, a, cx)));
  assert_eq!(status(&stores, cx, a), None, "a finished file's row goes with it");
  assert!(!cx.update(|cx| stores.has_active_extracts(cx)));
}

/// Resume All queues the cancelled and failed files that are still listed,
/// in queue order, with their selection now; Resume does one.
#[gpui_kit::test]
fn resume_requeues_cancelled_and_failed_files(cx: &mut TestAppContext) {
  let (a, b, c, gone) = ("/m/a.mkv", "/m/b.mkv", "/m/c.mkv", "/m/gone.mkv");
  let (stores, calls) = recording_stores(cx, config(false), all(&[a, b, c, gone], movie));
  add(&stores, cx, &[a, b, c, gone]);
  run(cx, |cx| extract_all(&stores, cx));
  for (file, outcome) in [
    (a, ExtractOutcome::Completed),
    (b, ExtractOutcome::Failed),
    (c, ExtractOutcome::Cancelled),
    (gone, ExtractOutcome::Failed),
  ] {
    report(&stores, cx, finished(file, outcome));
  }
  cx.update(|cx| assert!(remove_file(&stores, gone, cx)));
  assert!(cx.update(|cx| can_resume_all(&stores, cx)));
  // The selection now: b with its audio too.
  stores.files.update(cx, |files, cx| {
    files.set_checked(&TableScope::File(b.to_string()), "audio:1", true, cx)
  });
  calls.lock().unwrap().enqueued.clear();
  run(cx, |cx| resume_extract(&stores, a, cx));
  assert!(
    calls.lock().unwrap().enqueued.is_empty(),
    "a completed file is not resumed"
  );
  run(cx, |cx| resume_all(&stores, cx));
  let calls = calls.lock().unwrap();
  assert_eq!(calls.files(), [b, c]);
  assert_eq!(calls.enqueued[0].1.len(), 3, "tracks, the subtitles and the audio");
  assert!(!cx.update(|cx| can_resume_all(&stores, cx)));
}

#[gpui_kit::test]
fn clear_completed_removes_only_finished_rows(cx: &mut TestAppContext) {
  let (a, b) = ("/m/a.mkv", "/m/b.mkv");
  let (stores, _) = recording_stores(cx, config(false), all(&[a, b], movie));
  add(&stores, cx, &[a, b]);
  assert!(!cx.update(|cx| can_clear_completed(&stores, cx)));
  run(cx, |cx| extract_all(&stores, cx));
  report(&stores, cx, finished(a, ExtractOutcome::Completed));
  assert!(cx.update(|cx| can_clear_completed(&stores, cx)));
  cx.update(|cx| clear_completed(&stores, cx));
  assert_eq!(status(&stores, cx, a), None);
  assert_eq!(status(&stores, cx, b), Some(QueueItemStatus::Waiting));
  assert!(
    stores.files.read_with(cx, |files, _| files.files().len() == 2),
    "the files stay"
  );
}

#[gpui_kit::test]
fn copy_commands_copies_one_line_per_file(cx: &mut TestAppContext) {
  let _locale = crate::i18n::tests::LOCALE_LOCK
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  crate::i18n::set_language(bme_core::config::Language::EnUS);
  let (a, b) = ("/m/a.mkv", "/m/b.mkv");
  let (stores, _) = recording_stores(cx, config(false), all(&[a, b], movie));
  let (messages, _subscription) = shown(cx, &stores.ui);
  add(&stores, cx, &[a, b]);
  run(cx, |cx| {
    let requests = requests(&stores, &s(&[a, b]), cx);
    copy_commands(&stores, requests, cx)
  });
  let copied = cx
    .read(|cx| cx.read_from_clipboard().and_then(|item| item.text()))
    .expect("copied");
  let lines: Vec<&str> = copied.lines().collect();
  assert_eq!(lines.len(), 2);
  assert!(lines[0].contains("a.mkv") && lines[0].contains("a.3.eng.srt"));
  assert!(lines[1].contains("b.3.eng.srt"));
  assert!(!copied.contains("--gui-mode"));
  assert_eq!(*messages.borrow(), ["Command copied to clipboard."]);
  // Nothing to copy, nothing said.
  run(cx, |cx| copy_commands(&stores, Vec::new(), cx));
  assert_eq!(messages.borrow().len(), 1);
}

/// BetterMediaInfo is looked for at start and whenever its configured path
/// changes; the cards' button follows what was found.
#[gpui_kit::test]
fn better_media_info_is_detected_when_its_path_changes(cx: &mut TestAppContext) {
  let asked = Arc::new(Mutex::new(Vec::<String>::new()));
  let recorder = asked.clone();
  let tools = ToolDetector::new(move |tool, path, _| {
    assert_eq!(tool, Tool::BetterMediaInfo);
    recorder.lock().unwrap().push(path.to_string());
    ToolStatus {
      found: path == "/opt/bmi",
      path: format!("{path}/bin"),
    }
  });
  let (backend, _) = recording_backend();
  let stores = stores_with(cx, config(false), HashMap::new(), backend, tools);
  cx.update(|cx| watch_better_media_info(&stores, cx));
  cx.run_until_parked();
  assert!(
    !stores
      .files
      .read_with(cx, |files, _| files.better_media_info_available())
  );
  stores.config.update(cx, |config, cx| {
    config.update(cx, |config| {
      config.integration.better_media_info_path = " /opt/bmi ".into()
    })
  });
  cx.run_until_parked();
  let dir = stores
    .files
    .read_with(cx, |files, _| files.better_media_info_dir().map(str::to_string));
  assert_eq!(dir.as_deref(), Some("/opt/bmi/bin"));
  // Another change of the config asks again only for a new path.
  stores.config.update(cx, |config, cx| {
    config.update(cx, |config| config.language = bme_core::config::Language::De)
  });
  cx.run_until_parked();
  assert_eq!(*asked.lock().unwrap(), ["", "/opt/bmi"]);
  // Opening needs the executable there; the failure is said.
  let (messages, _subscription) = shown(cx, &stores.ui);
  run(cx, |cx| open_in_better_media_info(&stores, s(&["/m/a.mkv"]), cx));
  assert_eq!(messages.borrow().len(), 1);
}
