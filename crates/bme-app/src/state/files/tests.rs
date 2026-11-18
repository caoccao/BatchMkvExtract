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

//! Tests of the file store: intake, loading, the profile's selection, group
//! cards and the table's checkboxes.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use bme_core::config::{Config, ConfigProfile};
use core::prelude::v1::test;
use gpui_kit::{AppContext, Entity, Subscription, TestAppContext};

use super::*;
use crate::state::Persistence;

/// A fresh directory under the system temp dir, removed on drop.
pub(crate) struct TempDir(pub PathBuf);

impl TempDir {
  pub(crate) fn new(name: &str) -> Self {
    let path = std::env::temp_dir().join(format!("bme-app-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    Self(path)
  }

  /// Creates an empty file.
  pub(crate) fn file(&self, name: &str) -> String {
    let path = self.0.join(name);
    std::fs::write(&path, b"").unwrap();
    path.to_string_lossy().into_owned()
  }

  pub(crate) fn path(&self) -> &Path {
    &self.0
  }
}

impl Drop for TempDir {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

/// A row of `mkvmerge -J`'s listing.
pub(crate) fn track(id: i64, track_type: &str, codec_id: &str, language: &str) -> MkvTrack {
  MkvTrack {
    id,
    number: id + 1,
    track_type: track_type.to_string(),
    codec: codec_id.to_string(),
    codec_id: codec_id.to_string(),
    track_name: String::new(),
    language: language.to_string(),
  }
}

/// A movie: video, English and German audio, English subtitles and subtitles
/// without a language.
pub(crate) fn movie() -> Vec<MkvTrack> {
  vec![
    track(0, "video", "V_MPEG4/ISO/AVC", "und"),
    track(1, "audio", "A_AAC", "eng"),
    track(2, "audio", "A_AC3", "ger"),
    track(3, "subtitles", "S_TEXT/UTF8", "eng"),
    track(4, "subtitles", "S_TEXT/ASS", "und"),
  ]
}

/// A file with one video track only.
pub(crate) fn clip() -> Vec<MkvTrack> {
  vec![track(0, "video", "V_MPEG4/ISO/AVC", "und")]
}

/// Serves `fixtures` by path and counts the listings; other paths fail as
/// `mkvmerge` reports an unknown file.
pub(crate) fn fixture_source(
  fixtures: HashMap<String, Vec<MkvTrack>>,
) -> (TrackSource, Arc<Mutex<HashMap<String, usize>>>) {
  let listings = Arc::new(Mutex::new(HashMap::new()));
  let counter = listings.clone();
  let source = TrackSource::new(move |file| {
    *counter.lock().unwrap().entry(file.to_string()).or_insert(0) += 1;
    fixtures.get(file).cloned().ok_or_else(|| TrackListError::Failed {
      messages: vec![format!("The type of file '{file}' could not be recognized.")],
    })
  });
  (source, listings)
}

/// `fixtures` for `files`, each the same tracks.
pub(crate) fn all(files: &[&str], tracks: fn() -> Vec<MkvTrack>) -> HashMap<String, Vec<MkvTrack>> {
  files.iter().map(|file| (file.to_string(), tracks())).collect()
}

/// The Default profile with Group Mode off unless asked.
pub(crate) fn profile(group_mode: bool) -> ConfigProfile {
  ConfigProfile {
    default_group_mode: group_mode,
    ..ConfigProfile::named(ConfigProfile::DEFAULT_NAME)
  }
}

pub(crate) fn s(values: &[&str]) -> Vec<String> {
  values.iter().map(|value| (*value).to_string()).collect()
}

/// A store, the events it emitted and how often it notified.
struct Harness {
  store: Entity<FileStore>,
  events: Rc<RefCell<Vec<FileEvent>>>,
  notified: Rc<RefCell<usize>>,
  _subscriptions: Vec<Subscription>,
}

impl Harness {
  fn new(cx: &mut TestAppContext, profile: Option<ConfigProfile>, source: TrackSource) -> Self {
    let store = cx.new(|_| FileStore::with_profile(profile.as_ref(), source));
    let events = Rc::new(RefCell::new(Vec::new()));
    let notified = Rc::new(RefCell::new(0));
    let (event_sink, notify_sink) = (events.clone(), notified.clone());
    let subscriptions = cx.update(|cx| {
      vec![
        cx.subscribe(&store, move |_, event: &FileEvent, _| {
          event_sink.borrow_mut().push(event.clone())
        }),
        cx.observe(&store, move |_, _| *notify_sink.borrow_mut() += 1),
      ]
    });
    Self {
      store,
      events,
      notified,
      _subscriptions: subscriptions,
    }
  }

  /// A store over `fixtures` with `order` added and loaded.
  fn loaded(
    cx: &mut TestAppContext,
    profile: ConfigProfile,
    order: &[&str],
    fixtures: HashMap<String, Vec<MkvTrack>>,
  ) -> Self {
    let (source, _) = fixture_source(fixtures);
    let harness = Self::new(cx, Some(profile), source);
    harness.update(cx, |store, cx| store.add_files(s(order), cx));
    cx.run_until_parked();
    harness.take_events();
    harness.take_notified();
    harness
  }

  fn update<R>(&self, cx: &mut TestAppContext, f: impl FnOnce(&mut FileStore, &mut Context<FileStore>) -> R) -> R {
    self.store.update(cx, f)
  }

  fn read<R>(&self, cx: &mut TestAppContext, f: impl FnOnce(&FileStore) -> R) -> R {
    self.store.read_with(cx, |store, _| f(store))
  }

  fn take_events(&self) -> Vec<FileEvent> {
    std::mem::take(&mut self.events.borrow_mut())
  }

  fn take_notified(&self) -> usize {
    std::mem::take(&mut self.notified.borrow_mut())
  }
}

fn selected(store: &FileStore, file: &str) -> Option<Vec<String>> {
  store.selected_ids(file).map(<[String]>::to_vec)
}

#[gpui_kit::test]
fn add_files_appends_new_paths_in_order(cx: &mut TestAppContext) {
  let (source, _) = fixture_source(HashMap::new());
  let harness = Harness::new(cx, Some(profile(false)), source);
  harness.update(cx, |store, cx| store.add_files(s(&["b", "a"]), cx));
  harness.update(cx, |store, cx| store.add_files(s(&["a", "c", "c"]), cx));
  assert_eq!(harness.take_notified(), 2);
  harness.update(cx, |store, cx| store.add_files(s(&["a"]), cx));
  assert_eq!(harness.take_notified(), 0, "adding nothing does not notify");
  harness.read(cx, |store| assert_eq!(store.files(), ["b", "a", "c"]));
}

#[gpui_kit::test]
fn add_paths_lists_the_mkv_files_of_files_and_folders(cx: &mut TestAppContext) {
  let dir = TempDir::new("add-paths");
  let second = dir.file("b.mkv");
  let first = dir.file("a.MKV");
  dir.file("notes.txt");
  dir.file("audio.mka");
  let other = TempDir::new("add-paths-other");
  let single = other.file("single.mkv");
  let ignored = other.file("ignored.mp4");
  // A full path on every OS: `get_mkv_files` makes a path without a drive a
  // full one, so on Windows `/does/not/exist.mkv` would gain the current drive.
  let missing = other.path().join("missing.mkv");
  let (source, _) = fixture_source(HashMap::new());
  let store = cx.new(|_| FileStore::with_profile(Some(&profile(false)), source));
  let paths = vec![
    dir.path().to_path_buf(),
    PathBuf::from(&single),
    PathBuf::from(ignored),
    missing.clone(),
  ];
  let ui = cx.new(|_| UiState::new());
  let snackbars = Rc::new(RefCell::new(Vec::new()));
  let sink = snackbars.clone();
  let _subscription = cx.update(|cx| {
    cx.subscribe(&ui, move |_, event: &crate::state::ui::UiEvent, _| {
      if let crate::state::ui::UiEvent::Snackbar(snackbar) = event {
        sink.borrow_mut().push(snackbar.message.clone());
      }
    })
  });
  // The snackbar is worded once the paths are resolved, during the run.
  let _locale = crate::i18n::tests::LOCALE_LOCK
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner());
  crate::i18n::set_language(bme_core::config::Language::EnUS);
  cx.update(|cx| add_paths(&store, Some(&ui), paths, cx));
  cx.run_until_parked();
  // A folder's files sorted, then the given file; other types are left out.
  store.read_with(cx, |store, _| assert_eq!(store.files(), [first, second, single]));
  assert_eq!(
    *snackbars.borrow(),
    [format!(
      "1 item(s) could not be read and were skipped: {}",
      missing.display()
    )]
  );
}

#[gpui_kit::test]
fn a_loaded_file_gets_tracks_counts_and_the_profile_selection(cx: &mut TestAppContext) {
  let a = "/m/a.mkv";
  let harness = Harness::loaded(cx, profile(false), &[a], all(&[a], movie));
  harness.read(cx, |store| {
    assert!(matches!(store.load_state(a), LoadState::Loaded));
    assert_eq!(store.tracks(a).unwrap().len(), 5);
    let counts = store.track_counts(a).unwrap();
    assert_eq!((counts.video, counts.audio, counts.subtitles), (1, 2, 2));
    // The Default profile: subtitles in its languages, not the one without.
    assert_eq!(selected(store, a).unwrap(), ["subtitles:3"]);
    assert_eq!(store.selected_tracks(a), [movie()[3].clone()]);
    assert!(store.has_selection());
  });
}

#[gpui_kit::test]
fn a_listing_failure_is_kept_and_not_retried(cx: &mut TestAppContext) {
  let (good, bad) = ("/m/good.mkv", "/m/bad.mkv");
  let (source, listings) = fixture_source(all(&[good], movie));
  let harness = Harness::new(cx, Some(profile(false)), source);
  harness.update(cx, |store, cx| store.add_files(s(&[good, bad]), cx));
  harness.read(cx, |store| assert!(matches!(store.load_state(bad), LoadState::Loading)));
  cx.run_until_parked();
  // Another change of the store lists nothing again.
  harness.update(cx, |store, cx| store.toggle_group_mode(cx));
  cx.run_until_parked();
  harness.read(cx, |store| {
    assert!(matches!(
      store.load_state(bad),
      LoadState::Failed(TrackListError::Failed { .. })
    ));
    assert!(selected(store, bad).is_none());
  });
  assert_eq!(listings.lock().unwrap()[bad], 1);
  // Removed and added again, it is tried again.
  harness.update(cx, |store, cx| {
    store.remove_file(bad, cx);
    store.add_files(s(&[bad]), cx);
  });
  cx.run_until_parked();
  assert_eq!(listings.lock().unwrap()[bad], 2);
}

#[gpui_kit::test]
fn a_file_removed_while_loading_drops_its_result(cx: &mut TestAppContext) {
  let a = "/m/a.mkv";
  let (source, listings) = fixture_source(all(&[a], movie));
  let harness = Harness::new(cx, Some(profile(false)), source);
  harness.update(cx, |store, cx| {
    store.add_files(s(&[a]), cx);
    store.remove_file(a, cx);
  });
  cx.run_until_parked();
  harness.read(cx, |store| assert!(store.tracks(a).is_none()));
  // Removed and added again while the listing runs: that listing is
  // cancelled and its result dropped (another file may be at the path now),
  // and the file is listed anew.
  harness.update(cx, |store, cx| {
    store.add_files(s(&[a]), cx);
    store.remove_file(a, cx);
    store.add_files(s(&[a]), cx);
    assert_eq!(store.listings_in_flight(), 1, "the cancelled listing's place is free");
  });
  cx.run_until_parked();
  harness.read(cx, |store| assert!(store.tracks(a).is_some()));
  assert_eq!(listings.lock().unwrap()[a], 3);
}

/// Four listings that never end hold no place once their files are removed:
/// `mkvmerge` is told to stop, and the next files load.
#[gpui_kit::test]
fn removing_stalled_listings_frees_their_places(cx: &mut TestAppContext) {
  let stalled: Vec<String> = (0..4).map(|index| format!("/stalled/{index}.mkv")).collect();
  let fresh = "/m/fresh.mkv";
  let stopped = Arc::new(Mutex::new(Vec::<String>::new()));
  let recorder = stopped.clone();
  let source = TrackSource::with_tools(move |file, _, cancelled| {
    if file == fresh {
      return Ok(movie());
    }
    // A stalled drive: only a cancel ends it.
    while !cancelled.load(std::sync::atomic::Ordering::Relaxed) {
      std::thread::sleep(std::time::Duration::from_millis(1));
    }
    recorder.lock().unwrap().push(file.to_string());
    Err(TrackListError::Cancelled)
  });
  let harness = Harness::new(cx, Some(profile(false)), source);
  harness.update(cx, |store, cx| {
    store.add_files(stalled.clone(), cx);
    store.add_files(s(&[fresh]), cx);
    assert_eq!(store.listings_in_flight(), MAX_CONCURRENT_LOADS);
  });
  harness.update(cx, |store, cx| {
    for file in &stalled {
      store.remove_file(file, cx);
    }
  });
  cx.run_until_parked();
  harness.read(cx, |store| {
    assert!(matches!(store.load_state(fresh), LoadState::Loaded));
    assert_eq!(store.listings_in_flight(), 0);
  });
  let mut stopped = stopped.lock().unwrap().clone();
  stopped.sort();
  assert_eq!(stopped, stalled, "every stalled mkvmerge was told to stop");
}

/// A new MKVToolNix directory lists the failed files again with it, and
/// Retry lists one failed file again.
#[gpui_kit::test]
fn failed_listings_are_tried_again_with_new_tools_or_on_retry(cx: &mut TestAppContext) {
  let a = "/m/a.mkv";
  let tools_seen = Arc::new(Mutex::new(Vec::<String>::new()));
  let recorder = tools_seen.clone();
  let source = TrackSource::with_tools(move |_, tools, _| {
    recorder.lock().unwrap().push(tools.to_string());
    if tools == "/opt/mkvtoolnix" {
      Ok(movie())
    } else {
      Err(TrackListError::NotAvailable {
        path: format!("{tools}/mkvmerge"),
        detail: "No such file or directory".into(),
      })
    }
  });
  let mut config = Config::default();
  config.integration.mkv_toolnix_path = "/missing".into();
  let config = cx.new(|cx| crate::state::ConfigStore::new(config, Persistence::new(|_| {}), cx));
  let files = cx.new(|cx| FileStore::new(&config, source, cx));
  files.update(cx, |store, cx| store.add_files(s(&[a]), cx));
  cx.run_until_parked();
  files.read_with(cx, |store, _| {
    assert!(matches!(store.load_state(a), LoadState::Failed(_)))
  });
  files.update(cx, |store, cx| store.retry_load(a, cx));
  cx.run_until_parked();
  files.read_with(cx, |store, _| {
    assert!(matches!(store.load_state(a), LoadState::Failed(_)))
  });
  config.update(cx, |config, cx| {
    config.update(cx, |config| {
      config.integration.mkv_toolnix_path = " /opt/mkvtoolnix ".into()
    })
  });
  cx.run_until_parked();
  files.read_with(cx, |store, _| assert!(matches!(store.load_state(a), LoadState::Loaded)));
  assert_eq!(*tools_seen.lock().unwrap(), ["/missing", "/missing", "/opt/mkvtoolnix"]);
}

/// Ten files are listed four at a time, each once, although loading is
/// requested again while listings run.
#[gpui_kit::test]
fn at_most_four_listings_run_at_once(cx: &mut TestAppContext) {
  let files: Vec<String> = (0..10).map(|index| format!("/m/{index:02}.mkv")).collect();
  let names: Vec<&str> = files.iter().map(String::as_str).collect();
  let (source, listings) = fixture_source(all(&names, movie));
  let harness = Harness::new(cx, Some(profile(false)), source);
  harness.update(cx, |store, cx| store.add_files(files.clone(), cx));
  harness.read(cx, |store| assert_eq!(store.listings_in_flight(), MAX_CONCURRENT_LOADS));
  // More changes while they run start no more listings.
  harness.update(cx, |store, cx| {
    store.set_output_dir(&files[..1], "/out", cx);
    store.toggle_group_mode(cx);
    store.toggle_group_mode(cx);
  });
  harness.read(cx, |store| assert_eq!(store.listings_in_flight(), MAX_CONCURRENT_LOADS));
  let mut most = 0;
  for _ in 0..1_000 {
    cx.executor().tick();
    let in_flight = harness.read(cx, FileStore::listings_in_flight);
    most = most.max(in_flight);
    if in_flight == 0 {
      break;
    }
  }
  cx.run_until_parked();
  assert!(most <= MAX_CONCURRENT_LOADS, "{most} at once");
  harness.read(cx, |store| {
    assert!(
      files
        .iter()
        .all(|file| matches!(store.load_state(file), LoadState::Loaded))
    );
    assert_eq!(store.listings_in_flight(), 0);
  });
  let listings = listings.lock().unwrap();
  assert_eq!(listings.len(), 10);
  assert!(listings.values().all(|count| *count == 1));
}

#[gpui_kit::test]
fn the_profile_selection_never_replaces_a_selection(cx: &mut TestAppContext) {
  // Folders of their own: a group card would share one selection.
  let (a, b) = ("/m/a.mkv", "/n/b.mkv");
  let harness = Harness::loaded(cx, profile(false), &[a, b], all(&[a, b], movie));
  let scope = |file: &str| TableScope::File(file.to_string());
  harness.update(cx, |store, cx| {
    store.set_all_checked(&scope(a), false, cx);
    store.set_checked(&scope(b), "audio:1", true, cx);
  });
  harness.update(cx, |store, cx| {
    // Group Mode and a new profile derive everything again.
    store.toggle_group_mode(cx);
    store.toggle_group_mode(cx);
    let mut everything = profile(false);
    everything.select_video = true;
    everything.select_audio = true;
    store.set_selector(Some(TrackSelector::new(&everything)), cx);
  });
  harness.read(cx, |store| {
    assert_eq!(selected(store, a).unwrap(), Vec::<String>::new(), "an empty one too");
    assert_eq!(selected(store, b).unwrap(), ["subtitles:3", "audio:1"]);
    assert!(store.has_selection());
  });
}

#[gpui_kit::test]
fn nothing_is_selected_without_a_profile_or_without_tracks(cx: &mut TestAppContext) {
  let (a, empty) = ("/m/a.mkv", "/m/empty.mkv");
  let fixtures = HashMap::from([(a.to_string(), movie()), (empty.to_string(), Vec::new())]);
  let (source, _) = fixture_source(fixtures);
  let harness = Harness::new(cx, None, source);
  harness.update(cx, |store, cx| store.add_files(s(&[a, empty]), cx));
  cx.run_until_parked();
  harness.read(cx, |store| {
    assert!(selected(store, a).is_none());
    assert!(!store.has_selection());
  });
  // A profile selects the tracks of files that have none selected yet.
  harness.update(cx, |store, cx| {
    store.set_selector(Some(TrackSelector::new(&profile(false))), cx)
  });
  harness.read(cx, |store| {
    assert_eq!(selected(store, a).unwrap(), ["subtitles:3"]);
    assert!(selected(store, empty).is_none(), "a file without tracks");
  });
}

/// A group card's files share one selection: the first member's that has
/// one, else the profile's over the first member's tracks; a checkbox
/// writes to every member.
#[gpui_kit::test]
fn a_group_card_shares_one_selection(cx: &mut TestAppContext) {
  let (a, b, c, other) = ("/m/a.mkv", "/m/b.mkv", "/m/c.mkv", "/n/d.mkv");
  let fixtures = all(&[a, b, c, other], movie);
  let harness = Harness::loaded(cx, profile(true), &[a, b], fixtures.clone());
  let group = TableScope::Group(s(&[a, b]));
  harness.read(cx, |store| {
    assert_eq!(
      store.entries(),
      [RenderEntry::Group {
        key: crate::domain::grouping::grouping_key(a, &movie()),
        files: s(&[a, b]),
      }]
    );
    assert_eq!(selected(store, a), selected(store, b));
    assert_eq!(store.checked_keys(&group), ["subtitles:3"]);
    assert_eq!(store.table_rows(&group), movie());
  });
  harness.update(cx, |store, cx| store.set_checked(&group, "audio:2", true, cx));
  assert_eq!(
    harness.take_events(),
    [FileEvent::Changed { files: s(&[a, b]) }],
    "every member repaints"
  );
  // A file that joins the group takes its selection.
  harness.update(cx, |store, cx| store.add_files(s(&[other, c]), cx));
  cx.run_until_parked();
  harness.read(cx, |store| {
    assert_eq!(selected(store, c).unwrap(), ["subtitles:3", "audio:2"]);
    assert_eq!(selected(store, other).unwrap(), ["subtitles:3"], "a card of its own");
    assert_eq!(store.files_on_screen(), [a, b, c, other]);
  });
  // Group Mode off: each card keeps the shared selection; a card's own edit
  // stays its own.
  harness.update(cx, |store, cx| {
    store.toggle_group_mode(cx);
    store.set_checked(&TableScope::File(b.to_string()), "audio:2", false, cx);
  });
  harness.read(cx, |store| {
    assert_eq!(selected(store, a).unwrap(), ["subtitles:3", "audio:2"]);
    assert_eq!(selected(store, b).unwrap(), ["subtitles:3"]);
  });
  // On again: the group takes its first member's selection, as 0.3.0 did.
  harness.update(cx, |store, cx| store.toggle_group_mode(cx));
  harness.read(cx, |store| {
    assert_eq!(selected(store, b).unwrap(), ["subtitles:3", "audio:2"]);
    assert_eq!(selected(store, c).unwrap(), ["subtitles:3", "audio:2"]);
  });
  harness.update(cx, |store, cx| {
    store.set_all_checked(&TableScope::Group(s(&[a, b, c])), false, cx)
  });
  harness.read(cx, |store| {
    assert!([a, b, c].iter().all(|file| selected(store, file).unwrap().is_empty()));
    assert!(store.has_selection(), "the card of its own still has one");
  });
}

#[gpui_kit::test]
fn files_differing_in_track_counts_or_folder_are_not_grouped(cx: &mut TestAppContext) {
  let (a, b, short, elsewhere) = ("/m/a.mkv", "/m/b.mkv", "/m/short.mkv", "/n/b.mkv");
  let fixtures = HashMap::from([
    (a.to_string(), movie()),
    (b.to_string(), movie()),
    (short.to_string(), clip()),
    (elsewhere.to_string(), movie()),
  ]);
  let harness = Harness::loaded(cx, profile(true), &[short, a, elsewhere, b], fixtures);
  harness.read(cx, |store| {
    let entries = store.entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0], RenderEntry::Single { file: short.into() });
    assert!(matches!(&entries[1], RenderEntry::Group { files, .. } if files == &s(&[a, b])));
    assert_eq!(entries[2], RenderEntry::Single { file: elsewhere.into() });
    assert_eq!(store.files_on_screen(), [short, a, b, elsewhere]);
  });
}

/// Two files whose English and Japanese audio tracks swap ids get cards of
/// their own, so checking English on one never extracts Japanese from the
/// other.
#[gpui_kit::test]
fn files_whose_languages_swap_ids_are_not_grouped(cx: &mut TestAppContext) {
  let (a, b) = ("/m/a.mkv", "/m/b.mkv");
  let mut swapped = movie();
  swapped[1].language = "ger".into();
  swapped[2].language = "eng".into();
  let fixtures = HashMap::from([(a.to_string(), movie()), (b.to_string(), swapped)]);
  let harness = Harness::loaded(cx, profile(true), &[a, b], fixtures);
  harness.update(cx, |store, cx| {
    assert_eq!(
      store.entries(),
      [
        RenderEntry::Single { file: a.into() },
        RenderEntry::Single { file: b.into() }
      ]
    );
    store.set_checked(&TableScope::File(a.to_string()), "audio:1", true, cx);
  });
  harness.read(cx, |store| {
    let languages = |file: &str| -> Vec<String> {
      store
        .selected_tracks(file)
        .iter()
        .map(|track| format!("{}:{}", track.key(), track.language))
        .collect()
    };
    assert_eq!(languages(a), ["audio:1:eng", "subtitles:3:eng"]);
    assert_eq!(languages(b), ["subtitles:3:eng"]);
  });
}

#[gpui_kit::test]
fn the_table_waits_for_the_tracks(cx: &mut TestAppContext) {
  let a = "/m/a.mkv";
  let (source, _) = fixture_source(all(&[a], movie));
  let harness = Harness::new(cx, Some(profile(false)), source);
  harness.update(cx, |store, cx| {
    store.add_files(s(&[a]), cx);
    let scope = TableScope::File(a.to_string());
    store.set_all_checked(&scope, true, cx);
    store.set_checked(&scope, "video:0", true, cx);
    assert!(store.table_rows(&scope).is_empty());
    assert!(store.checked_keys(&scope).is_empty());
    assert!(store.selected_ids(a).is_none(), "nothing before the tracks");
  });
  cx.run_until_parked();
  harness.update(cx, |store, cx| {
    let scope = TableScope::File(a.to_string());
    store.set_checked(&scope, "subtitles:3", true, cx);
    store.set_all_checked(&scope, true, cx);
    assert_eq!(store.checked_keys(&scope).len(), 5);
    assert_eq!(store.selected_tracks(a), movie(), "in table order");
  });
}

/// A track `mkvextract` refuses (here FFV1 video) is never selected: not by
/// the profile, not by select-all, not by its checkbox, and not in a
/// request, so the FLAC audio beside it is still extracted.
#[gpui_kit::test]
fn a_track_mkvextract_cannot_extract_is_never_selected(cx: &mut TestAppContext) {
  let a = "/m/a.mkv";
  let tracks = vec![track(0, "video", "V_FFV1", "und"), track(1, "audio", "A_FLAC", "eng")];
  let everything = ConfigProfile {
    select_video: true,
    select_audio: true,
    ..profile(false)
  };
  let harness = Harness::loaded(cx, everything, &[a], HashMap::from([(a.to_string(), tracks)]));
  let scope = TableScope::File(a.to_string());
  harness.update(cx, |store, cx| {
    assert_eq!(selected(store, a).unwrap(), ["audio:1"], "the profile's selection");
    store.set_all_checked(&scope, false, cx);
    store.set_checked(&scope, "video:0", true, cx);
    assert!(store.checked_keys(&scope).is_empty(), "its checkbox");
    store.set_all_checked(&scope, true, cx);
    assert_eq!(store.checked_keys(&scope), ["audio:1"], "select-all");
  });
  // Even a selection that names it extracts only the rest.
  harness.update(cx, |store, _| {
    store
      .file_selected_ids
      .insert(a.to_string(), s(&["video:0", "audio:1"]));
  });
  harness.read(cx, |store| {
    let keys: Vec<String> = store.selected_tracks(a).iter().map(MkvTrack::key).collect();
    assert_eq!(keys, ["audio:1"]);
  });
}

#[gpui_kit::test]
fn group_mode_starts_from_the_profile_and_follows_its_operations(cx: &mut TestAppContext) {
  let mut config = Config::default();
  config.profiles.push(profile(false));
  config.profiles[1].name = "Flat".to_string();
  let store = cx.new(|cx| crate::state::ConfigStore::new(config, Persistence::new(|_| {}), cx));
  let (source, _) = fixture_source(HashMap::new());
  let files = cx.new(|cx| FileStore::new(&store, source, cx));
  let group_mode = |cx: &mut TestAppContext| files.read_with(cx, |files, _| files.group_mode());
  assert!(group_mode(cx), "Default's defaultGroupMode");
  store.update(cx, |store, cx| store.set_active_profile("Flat", cx));
  assert!(!group_mode(cx));
  files.update(cx, |files, cx| files.toggle_group_mode(cx));
  assert!(group_mode(cx), "a session toggle");
  // Activating the active profile again sets its value, as 0.3.0 did.
  store.update(cx, |store, cx| store.set_active_profile("Flat", cx));
  assert!(!group_mode(cx));
  store.update(cx, |store, cx| {
    store.update_active_profile(cx, |profile| profile.default_group_mode = true)
  });
  assert!(group_mode(cx));
  // The profile's selection follows the active profile.
  store.update(cx, |store, cx| {
    store.update_active_profile(cx, |profile| profile.select_video = true)
  });
  let selects_video = files.read_with(cx, |files, _| {
    files
      .selector
      .as_ref()
      .is_some_and(|selector| selector.selects(&track(0, "video", "V_MPEG4/ISO/AVC", "und")))
  });
  assert!(selects_video);
}

#[gpui_kit::test]
fn remove_and_clear_drop_everything_about_the_files(cx: &mut TestAppContext) {
  let (a, b) = ("/m/a.mkv", "/m/b.mkv");
  let harness = Harness::loaded(cx, profile(false), &[a, b], all(&[a, b], movie));
  harness.update(cx, |store, cx| {
    store.set_output_dir(&s(&[a, b]), "/out", cx);
    store.set_global_output_dir("/global".to_string(), cx);
    store.remove_file(a, cx);
    assert_eq!(store.files(), [b]);
    assert!(store.tracks(a).is_none() && store.selected_ids(a).is_none() && store.output_dir(a).is_none());
    assert_eq!(store.output_dir(b), Some("/out"));
    store.toggle_group_mode(cx);
    store.clear_files(cx);
    assert!(!store.has_files() && !store.has_selection());
    assert!(store.tracks(b).is_none() && store.output_dir(b).is_none());
    assert_eq!(
      store.global_output_dir(),
      Some("/global"),
      "the session's directory stays"
    );
    assert!(store.group_mode(), "Group Mode stays");
  });
}

#[gpui_kit::test]
fn output_directories(cx: &mut TestAppContext) {
  let (a, b) = ("/m/a.mkv", "/m/b.mkv");
  let harness = Harness::loaded(cx, profile(false), &[a, b], all(&[a, b], movie));
  harness.update(cx, |store, cx| {
    store.set_output_dir(&s(&[a, b]), "/out", cx);
    assert_eq!((store.output_dir(a), store.output_dir(b)), (Some("/out"), Some("/out")));
    store.set_output_dir(&s(&[b]), "", cx);
    assert_eq!((store.output_dir(a), store.output_dir(b)), (Some("/out"), None));
  });
  assert_eq!(
    harness.take_events(),
    [
      FileEvent::Changed { files: s(&[a, b]) },
      FileEvent::Changed { files: s(&[b]) },
    ]
  );
  assert_eq!(harness.take_notified(), 0, "not structural");
  // Observers hear of each update once; each change is an update here.
  for dir in ["/global", "/global", ""] {
    harness.update(cx, |store, cx| store.set_global_output_dir(dir.to_string(), cx));
  }
  harness.read(cx, |store| assert_eq!(store.global_output_dir(), None));
  assert_eq!(harness.take_notified(), 2, "the repeated value is no change");
}

#[gpui_kit::test]
fn the_selection_flag_notifies_only_when_it_flips(cx: &mut TestAppContext) {
  let a = "/m/a.mkv";
  let harness = Harness::loaded(cx, profile(false), &[a], all(&[a], movie));
  let scope = TableScope::File(a.to_string());
  harness.update(cx, |store, cx| store.set_checked(&scope, "audio:1", true, cx));
  assert_eq!(harness.take_notified(), 0, "still selected");
  assert_eq!(harness.take_events(), [FileEvent::Changed { files: s(&[a]) }]);
  harness.update(cx, |store, cx| store.set_all_checked(&scope, false, cx));
  assert_eq!(harness.take_notified(), 1, "nothing selected any more");
  harness.take_events();
  harness.update(cx, |store, cx| store.set_checked(&scope, "audio:1", false, cx));
  assert_eq!(harness.take_events(), [], "no change");
}

#[gpui_kit::test]
fn better_media_info_and_group_mode_notify_on_change(cx: &mut TestAppContext) {
  let (source, _) = fixture_source(HashMap::new());
  let harness = Harness::new(cx, Some(profile(false)), source);
  for _ in 0..2 {
    harness.update(cx, |store, cx| store.set_better_media_info(Some("/opt/bmi".into()), cx));
    harness.update(cx, |store, cx| store.set_group_mode(true, cx));
  }
  assert_eq!(harness.take_notified(), 2, "only the changes notify");
  harness.read(cx, |store| {
    assert_eq!(store.better_media_info_dir(), Some("/opt/bmi"));
    assert!(store.better_media_info_available() && store.group_mode());
  });
}

/// Dropped again under another spelling (`./`, a link, another letter case
/// where the file system ignores case), a listed file is not listed twice.
#[gpui_kit::test]
fn the_same_file_under_another_spelling_is_listed_once(cx: &mut TestAppContext) {
  let dir = TempDir::new("same-file");
  let movie = dir.file("Movie.mkv");
  let other = dir.file("Other.mkv");
  let dotted = dir.path().join(".").join("Movie.mkv").to_string_lossy().into_owned();
  let mut spellings = vec![dotted];
  #[cfg(unix)]
  {
    let link = dir.path().join("Link.mkv");
    std::os::unix::fs::symlink(&movie, &link).unwrap();
    spellings.push(link.to_string_lossy().into_owned());
  }
  if cfg!(any(target_os = "macos", target_os = "windows")) {
    spellings.push(dir.path().join("MOVIE.MKV").to_string_lossy().into_owned());
  }
  let (source, _) = fixture_source(HashMap::new());
  let store = cx.new(|_| FileStore::with_profile(Some(&profile(false)), source));
  let paths = |paths: Vec<String>| paths.into_iter().map(PathBuf::from).collect::<Vec<_>>();
  cx.update(|cx| add_paths(&store, None, paths(vec![movie.clone()]), cx));
  cx.run_until_parked();
  cx.update(|cx| add_paths(&store, None, paths([spellings, vec![other.clone()]].concat()), cx));
  cx.run_until_parked();
  store.read_with(cx, |store, _| assert_eq!(store.files(), [movie.clone(), other.clone()]));
  // Removed, it can come back under any spelling (made a full path).
  store.update(cx, |store, cx| store.remove_file(&movie, cx));
  let link = dir.path().join("Again.mkv");
  #[cfg(unix)]
  std::os::unix::fs::symlink(&movie, &link).unwrap();
  #[cfg(not(unix))]
  std::fs::copy(&movie, &link).unwrap();
  let again = link.to_string_lossy().into_owned();
  cx.update(|cx| add_paths(&store, None, paths(vec![again.clone()]), cx));
  cx.run_until_parked();
  store.read_with(cx, |store, _| assert_eq!(store.files(), [other, again]));
}
