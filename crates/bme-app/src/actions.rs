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

//! What spans stores: extract, cancel, resume, clear and remove, which touch
//! both the files and the queue; Copy Command and BetterMediaInfo.
//!
//! An extraction is read from the stores on the foreground: the file, its
//! selected tracks, the active profile and the output directory (the file's
//! own, else the session's, else the file's folder). On the background
//! executor the output directory is created and the `mkvextract` arguments
//! are built; then each file that is not waiting or extracting is listed as
//! waiting and handed to the extract queue, in order, so a drive's queue
//! follows the file list, as in 0.3.0. A
//! directory that cannot be created is named in a toast, and that file is
//! not queued.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use bme_core::config::ConfigProfile;
use bme_core::extract_args::{build_command_string, build_extract_args, resolve_output_dir};
use bme_core::protocol::MkvTrack;
use gpui_kit::{App, AppContext, ClipboardItem, Task};

use crate::domain::status::QueueItemStatus;
use crate::state::config::active_profile;
use crate::state::ui::NotificationKind;
use crate::state::{FileStore, Stores};
use crate::tools::Tool;

/// One extraction, read from the stores.
#[derive(Clone, Debug)]
pub struct ExtractRequest {
  pub file: String,
  /// The selected tracks, in table order.
  pub tracks: Vec<MkvTrack>,
  /// The profile whose templates name the outputs.
  pub profile: ConfigProfile,
  /// Where the tracks go: the file's override, else the session's
  /// directory, else the file's folder.
  pub output_dir: String,
}

impl ExtractRequest {
  /// The extraction of `file` with its current selection; `None` without a
  /// selected track.
  pub fn of(files: &FileStore, profile: &ConfigProfile, file: &str) -> Option<Self> {
    let tracks = files.selected_tracks(file);
    (!tracks.is_empty()).then(|| Self {
      file: file.to_string(),
      tracks,
      profile: profile.clone(),
      output_dir: resolve_output_dir(file, files.output_dir(file), files.global_output_dir()),
    })
  }

  /// The `mkvextract` arguments after the source file.
  pub fn args(&self) -> Vec<String> {
    build_extract_args(&self.file, &self.output_dir, &self.tracks, &self.profile)
  }

  /// The copyable shell command with the MKVToolNix in `mkv_toolnix_path`.
  /// Looks for the `mkvextract` an extraction runs.
  pub fn command(&self, mkv_toolnix_path: &str) -> String {
    build_command_string(
      &self.file,
      &self.output_dir,
      mkv_toolnix_path,
      &self.tracks,
      &self.profile,
    )
  }
}

/// The extractions of `files`, in order, with the active profile; a file
/// without a selected track is left out, and everything without a profile.
pub fn requests(stores: &Stores, files: &[String], cx: &App) -> Vec<ExtractRequest> {
  let Some(profile) = active_profile(stores.config.read(cx).config()) else {
    return Vec::new();
  };
  let store = stores.files.read(cx);
  files
    .iter()
    .filter_map(|file| ExtractRequest::of(store, profile, file))
    .collect()
}

/// What the user is told when an output directory cannot be created.
pub fn failed_create_output_message(dir: &str) -> String {
  rust_i18n::t!("notification.failedCreateOutput", path = dir).into_owned()
}

/// Queues `requests`: creates each output directory and builds the
/// arguments on the background executor, then lists each file that is not
/// waiting or extracting as waiting and hands it to the extract queue, in
/// order.
pub fn enqueue(stores: &Stores, requests: Vec<ExtractRequest>, cx: &mut App) -> Task<()> {
  let requests: Vec<ExtractRequest> = {
    let queue = stores.queue.read(cx);
    requests
      .into_iter()
      .filter(|request| queue.can_enqueue(&request.file))
      .collect()
  };
  if requests.is_empty() {
    return Task::ready(());
  }
  let stores = stores.clone();
  cx.spawn(async move |cx| {
    let prepared = cx
      .background_spawn(async move {
        requests
          .into_iter()
          .map(
            |request| match bme_core::services::ensure_output_path(&request.output_dir) {
              Ok(()) => (request.file.clone(), Ok(request.args())),
              Err(error) => {
                log::warn!("Failed to create {}: {error}", request.output_dir);
                (request.file.clone(), Err(request.output_dir))
              }
            },
          )
          .collect::<Vec<_>>()
      })
      .await;
    cx.update(|cx| {
      for (file, prepared) in prepared {
        match prepared {
          Ok(args) => hand_over(&stores, file, args, cx),
          Err(dir) => stores.ui.update(cx, |ui, cx| {
            ui.show_toast(NotificationKind::Error, file, failed_create_output_message(&dir), cx)
          }),
        }
      }
    });
  })
}

/// Lists `file` as waiting and hands it to the extract queue, unless it is
/// waiting or extracting already.
fn hand_over(stores: &Stores, file: String, args: Vec<String>, cx: &mut App) {
  let backend = stores.extracts.clone();
  stores.queue.update(cx, |queue, cx| {
    if !queue.add_to_queue(&file, cx) {
      return;
    }
    if let Err(error) = backend.enqueue(file.clone(), args) {
      log::error!("Failed to enqueue {file}: {error}");
      queue.abandon(&file, cx);
    }
  });
}

/// A card's Extract, a group card's Extract All: each of `files` with a
/// selection that is not waiting or extracting.
pub fn extract(stores: &Stores, files: &[String], cx: &mut App) -> Task<()> {
  let requests = requests(stores, files, cx);
  enqueue(stores, requests, cx)
}

/// Whether Extract All (toolbar, F3) is available: a listed file has a
/// selected track.
pub fn can_extract_all(stores: &Stores, cx: &App) -> bool {
  stores.files.read(cx).has_selection()
}

/// Extract All: every file with a selection, in the order the file list
/// shows them.
pub fn extract_all(stores: &Stores, cx: &mut App) -> Task<()> {
  let files = stores.files.read(cx).files_on_screen();
  extract(stores, &files, cx)
}

/// Asks the queue to cancel `file`; the outcome arrives as an extract event.
pub fn cancel_extract(stores: &Stores, file: &str, cx: &mut App) {
  stores
    .queue
    .update(cx, |queue, cx| queue.mark_cancel_requested(file, cx));
  if let Err(error) = stores.extracts.cancel(file) {
    log::error!("Failed to cancel {file}: {error}");
  }
}

/// [`cancel_extract`] for each of `files`.
pub fn cancel_extracts(stores: &Stores, files: &[String], cx: &mut App) {
  for file in files {
    cancel_extract(stores, file, cx);
  }
}

/// Cancel All (toolbar, F4, the close guard's Yes): every waiting or
/// extracting file.
pub fn cancel_all(stores: &Stores, cx: &mut App) {
  let files = stores.queue.read(cx).active_files();
  cancel_extracts(stores, &files, cx);
}

/// The cancelled and failed files of the queue that are still listed, in
/// queue order.
fn resumable_files(stores: &Stores, cx: &App) -> Vec<String> {
  let listed: HashSet<&String> = stores.files.read(cx).files().iter().collect();
  stores
    .queue
    .read(cx)
    .resumable_items()
    .filter(|item| listed.contains(&item.file))
    .map(|item| item.file.clone())
    .collect()
}

/// Whether Resume All is available: a cancelled or failed file is still
/// listed.
pub fn can_resume_all(stores: &Stores, cx: &App) -> bool {
  !resumable_files(stores, cx).is_empty()
}

/// Resumes one cancelled or failed file from its queue row, with its current
/// selection and the active profile.
pub fn resume_extract(stores: &Stores, file: &str, cx: &mut App) -> Task<()> {
  let resumable = stores
    .queue
    .read(cx)
    .item(file)
    .is_some_and(|item| item.status.is_resumable());
  if !resumable {
    return Task::ready(());
  }
  extract(stores, &[file.to_string()], cx)
}

/// Resume All requeues the cancelled and failed files in queue order.
pub fn resume_all(stores: &Stores, cx: &mut App) -> Task<()> {
  let files = resumable_files(stores, cx);
  extract(stores, &files, cx)
}

/// Whether Clear Completed is available: the queue has a finished row.
pub fn can_clear_completed(stores: &Stores, cx: &App) -> bool {
  stores.queue.read(cx).items().any(|item| item.status.is_terminal())
}

/// Removes the completed, cancelled and failed rows of every drive.
pub fn clear_completed(stores: &Stores, cx: &mut App) {
  stores.queue.update(cx, |queue, cx| queue.clear_completed(cx));
}

/// Whether Clear (toolbar, Ctrl+Q) is available: files listed and no
/// extraction waiting or running.
pub fn can_clear(stores: &Stores, cx: &App) -> bool {
  stores.files.read(cx).has_files() && !stores.has_active_extracts(cx)
}

/// Clears the file list and the queue if [`can_clear`].
pub fn clear_files(stores: &Stores, cx: &mut App) {
  if can_clear(stores, cx) {
    stores.files.update(cx, |files, cx| files.clear_files(cx));
    stores.queue.update(cx, |queue, cx| queue.clear(cx));
  }
}

/// Whether `file` is being extracted now.
pub fn is_extracting(stores: &Stores, file: &str, cx: &App) -> bool {
  stores
    .queue
    .read(cx)
    .item(file)
    .is_some_and(|item| item.status == QueueItemStatus::Extracting)
}

/// A card's Delete (`MkvFileCard.handleDelete`): nothing while the file is
/// extracted; otherwise it goes with its queue row, a waiting one cancelled
/// first (its row stays, hidden, until the extract queue reports the end).
/// `false` when nothing was removed.
pub fn remove_file(stores: &Stores, file: &str, cx: &mut App) -> bool {
  if is_extracting(stores, file, cx) {
    return false;
  }
  let uncancelled = stores
    .queue
    .read(cx)
    .item(file)
    .is_some_and(|item| item.status.is_active() && !item.cancel_requested);
  if uncancelled {
    cancel_extract(stores, file, cx);
  }
  stores.files.update(cx, |files, cx| files.remove_file(file, cx));
  stores.queue.update(cx, |queue, cx| queue.remove_from_queue(file, cx));
  true
}

/// A group card's Clear All (`GroupCard.handleClearAll`): every file but the
/// ones being extracted goes, as with [`remove_file`].
pub fn clear_group(stores: &Stores, files: &[String], cx: &mut App) {
  for file in files {
    remove_file(stores, file, cx);
  }
}

/// Copies the commands of `requests`, one per line, to the clipboard and says
/// so in a snackbar. The commands are built on the background executor: they
/// name the `mkvextract` an extraction runs, which may be looked for.
pub fn copy_commands(stores: &Stores, requests: Vec<ExtractRequest>, cx: &mut App) -> Task<()> {
  if requests.is_empty() {
    return Task::ready(());
  }
  let toolnix = stores.config.read(cx).config().integration.mkv_toolnix_path.clone();
  let ui = stores.ui.clone();
  cx.spawn(async move |cx| {
    let commands = cx
      .background_spawn(async move {
        requests
          .iter()
          .map(|request| request.command(&toolnix))
          .collect::<Vec<_>>()
      })
      .await;
    cx.update(|cx| {
      cx.write_to_clipboard(ClipboardItem::new_string(commands.join("\n")));
      ui.update(cx, |ui, cx| {
        ui.show_snackbar(NotificationKind::Success, tr!("extract.commandCopied").to_string(), cx)
      });
    });
  })
}

/// Opens `paths` in the detected BetterMediaInfo; a failure is shown in a
/// snackbar.
pub fn open_in_better_media_info(stores: &Stores, paths: Vec<String>, cx: &mut App) -> Task<()> {
  let Some(dir) = stores.files.read(cx).better_media_info_dir().map(str::to_string) else {
    return Task::ready(());
  };
  let ui = stores.ui.clone();
  cx.spawn(async move |cx| {
    let launched = cx
      .background_spawn(async move { bme_core::services::launch_better_media_info_from(&dir, paths) })
      .await;
    if let Err(error) = launched {
      cx.update(|cx| {
        ui.update(cx, |ui, cx| {
          ui.show_snackbar(NotificationKind::Error, error.to_string(), cx)
        });
      });
    }
  })
}

/// Detects BetterMediaInfo now and whenever its configured path changes: the
/// cards show Open in BetterMediaInfo while it is found. A detection that a
/// newer one overtook is dropped.
pub fn watch_better_media_info(stores: &Stores, cx: &mut App) {
  let detected_path: Rc<RefCell<Option<String>>> = Rc::default();
  let generation = Rc::new(Cell::new(0_u64));
  let detect = {
    let (files, tools) = (stores.files.clone(), stores.tools.clone());
    move |path: String, cx: &mut App| {
      if detected_path.borrow().as_ref() == Some(&path) {
        return;
      }
      *detected_path.borrow_mut() = Some(path.clone());
      generation.set(generation.get() + 1);
      let (files, tools, generation, current) = (files.clone(), tools.clone(), generation.clone(), generation.get());
      cx.spawn(async move |cx| {
        let status = cx
          .background_spawn(async move { tools.detect(Tool::BetterMediaInfo, path.trim(), false) })
          .await;
        if generation.get() != current {
          return;
        }
        let dir = status.found.then_some(status.path);
        files.update(cx, |files, cx| files.set_better_media_info(dir, cx));
      })
      .detach();
    }
  };
  let path = |stores: &Stores, cx: &App| Tool::BetterMediaInfo.path(stores.config.read(cx).config()).to_string();
  detect(path(stores, cx), cx);
  let watched = stores.clone();
  cx.observe(&stores.config, move |_, cx| detect(path(&watched, cx), cx))
    .detach();
}

#[cfg(test)]
mod tests;
