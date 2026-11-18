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
//! own, else the session's, else the file's folder). Each file that is not
//! waiting or extracting is listed as waiting at once (a reservation, which
//! Cancel and Delete end). On the background executor the output directory
//! is created and the `mkvextract` arguments are built; then each file whose
//! reservation still stands is handed to the extract queue, in order, so a
//! drive's queue follows the file list, as in 0.3.0.
//!
//! A file is not queued, and a toast says why, when its output directory
//! cannot be created, when two of its rows would write one file, when an
//! output is a listed source file, or when another waiting or extracting
//! file writes one of its outputs: the first file asked for keeps it.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::SystemTime;

use bme_core::config::ConfigProfile;
use bme_core::extract_args::{
  ExtractPlan, build_command_string, build_extract_plan, output_key, repeated_output, resolve_output_dir,
};
use bme_core::protocol::MkvTrack;
use gpui_kit::{App, AppContext, ClipboardItem, Task};

use crate::domain::status::QueueItemStatus;
use crate::state::config::active_profile;
use crate::state::ui::NotificationKind;
use crate::state::{FileStore, Stores};
use crate::tools::{Lookup, Tool};

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
  /// The MKVToolNix directory of the settings when the extraction was asked
  /// for: the copyable command names its `mkvextract`, and the extraction
  /// runs it, also when the setting changes while it waits.
  pub mkv_toolnix_path: String,
}

impl ExtractRequest {
  /// The extraction of `file` with its current selection, run with the
  /// MKVToolNix in `mkv_toolnix_path`; `None` without a selected track.
  pub fn of(files: &FileStore, profile: &ConfigProfile, mkv_toolnix_path: &str, file: &str) -> Option<Self> {
    let tracks = files.selected_tracks(file);
    (!tracks.is_empty()).then(|| Self {
      file: file.to_string(),
      tracks,
      profile: profile.clone(),
      output_dir: resolve_output_dir(file, files.output_dir(file), files.global_output_dir()),
      mkv_toolnix_path: mkv_toolnix_path.to_string(),
    })
  }

  /// The `mkvextract` arguments after the source file.
  pub fn args(&self) -> Vec<String> {
    self.plan().args
  }

  /// The arguments and the files the extraction writes.
  pub fn plan(&self) -> ExtractPlan {
    build_extract_plan(&self.file, &self.output_dir, &self.tracks, &self.profile)
  }

  /// The copyable shell command, with the `mkvextract` the extraction runs
  /// (which may be looked for).
  pub fn command(&self) -> String {
    build_command_string(
      &self.file,
      &self.output_dir,
      &self.mkv_toolnix_path,
      &self.tracks,
      &self.profile,
    )
  }
}

/// The extractions of `files`, in order, with the active profile and the
/// configured MKVToolNix; a file without a selected track is left out, and
/// everything without a profile.
pub fn requests(stores: &Stores, files: &[String], cx: &App) -> Vec<ExtractRequest> {
  let config = stores.config.read(cx).config();
  let Some(profile) = active_profile(config) else {
    return Vec::new();
  };
  let mkv_toolnix_path = Tool::MkvToolNix.path(config).trim();
  let store = stores.files.read(cx);
  files
    .iter()
    .filter_map(|file| ExtractRequest::of(store, profile, mkv_toolnix_path, file))
    .collect()
}

/// What the user is told when an output directory cannot be created.
pub fn failed_create_output_message(dir: &str) -> String {
  rust_i18n::t!("notification.failedCreateOutput", path = dir).into_owned()
}

/// An extraction ready for the extract queue: its output directory exists.
struct Prepared {
  args: Vec<String>,
  /// The files it writes, as the arguments name them.
  outputs: Vec<String>,
  /// The [`output_key`]s of `outputs` inside the output directory as the
  /// file system resolves it (links, `.`, `..`), so two spellings of one
  /// directory meet.
  keys: Vec<String>,
}

/// Creates the output directory of `request` and builds its arguments.
/// Blocking; `Err` names a directory that could not be created.
fn prepare(request: &ExtractRequest) -> Result<Prepared, String> {
  if let Err(error) = bme_core::services::ensure_output_path(&request.output_dir) {
    log::warn!("Failed to create {}: {error}", request.output_dir);
    return Err(request.output_dir.clone());
  }
  let ExtractPlan { args, outputs } = request.plan();
  let keys = match std::fs::canonicalize(&request.output_dir) {
    Ok(resolved) => {
      let resolved = resolved.to_string_lossy();
      build_extract_plan(&request.file, &resolved, &request.tracks, &request.profile).outputs
    }
    Err(_) => outputs.clone(),
  };
  let keys = keys.iter().map(|output| output_key(output)).collect();
  Ok(Prepared { args, outputs, keys })
}

/// Queues `requests`: lists each file that is not waiting or extracting as
/// waiting, creates each output directory and builds the arguments on the
/// background executor, then hands each file whose reservation stands to
/// the extract queue, in order.
pub fn enqueue(stores: &Stores, requests: Vec<ExtractRequest>, cx: &mut App) -> Task<()> {
  if stores.ui.read(cx).is_closing() {
    // The exit dialog's Yes is stopping every extraction.
    return Task::ready(());
  }
  let reserved: Vec<(ExtractRequest, u64)> = stores.queue.update(cx, |queue, cx| {
    requests
      .into_iter()
      .filter_map(|request| queue.reserve(&request.file, cx).map(|token| (request, token)))
      .collect()
  });
  if reserved.is_empty() {
    return Task::ready(());
  }
  let stores = stores.clone();
  cx.spawn(async move |cx| {
    let prepared = cx
      .background_spawn(async move {
        reserved
          .into_iter()
          .map(|(request, token)| {
            let prepared = prepare(&request);
            (
              request.file,
              token,
              prepared.map(|prepared| (prepared, request.mkv_toolnix_path)),
            )
          })
          .collect::<Vec<_>>()
      })
      .await;
    cx.update(|cx| {
      let sources = stores.files.read(cx).source_keys();
      for (file, token, prepared) in prepared {
        let refusal = match prepared {
          Ok((prepared, tools)) => hand_over(&stores, &file, token, prepared, tools, &sources, cx),
          Err(dir) => {
            let reserved = stores.queue.update(cx, |queue, cx| {
              let reserved = queue.is_reserved(&file, token);
              if reserved {
                queue.abandon(&file, cx);
              }
              reserved
            });
            reserved.then(|| failed_create_output_message(&dir))
          }
        };
        if let Some(message) = refusal {
          stores
            .ui
            .update(cx, |ui, cx| ui.show_toast(NotificationKind::Error, file, message, cx));
        }
      }
    });
  })
}

/// Hands the reserved `file` to the extract queue, to run with the
/// MKVToolNix in `tools`, unless its reservation `token` was cancelled
/// meanwhile or an output of its is taken; returns what the user is told
/// when it is refused. `sources` are the output keys of the listed source
/// files.
fn hand_over(
  stores: &Stores,
  file: &str,
  token: u64,
  prepared: Prepared,
  tools: String,
  sources: &HashSet<String>,
  cx: &mut App,
) -> Option<String> {
  let backend = stores.extracts.clone();
  stores.queue.update(cx, |queue, cx| {
    if !queue.is_reserved(file, token) {
      return None;
    }
    let Prepared { args, outputs, keys } = prepared;
    let refusal = if let Some(index) = repeated_output(&keys) {
      Some(rust_i18n::t!("notification.outputRepeated", path = outputs[index]).into_owned())
    } else if let Some(index) = keys.iter().position(|key| sources.contains(key)) {
      Some(rust_i18n::t!("notification.outputIsSource", path = outputs[index]).into_owned())
    } else {
      queue.output_owner(file, &keys).map(|(index, owner)| {
        rust_i18n::t!("notification.outputTaken", path = outputs[index], other = owner).into_owned()
      })
    };
    if refusal.is_some() {
      queue.abandon(file, cx);
      return refusal;
    }
    queue.hand_over(file, keys);
    if let Err(error) = backend.enqueue(file.to_string(), tools, args) {
      log::error!("Failed to enqueue {file}: {error}");
      queue.abandon(file, cx);
    }
    None
  })
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
/// A file still being prepared is cancelled at once: the extract queue does
/// not have it yet.
pub fn cancel_extract(stores: &Stores, file: &str, cx: &mut App) {
  let reservation = stores
    .queue
    .update(cx, |queue, cx| queue.cancel_reservation(file, SystemTime::now(), cx));
  if reservation {
    return;
  }
  stores
    .queue
    .update(cx, |queue, cx| queue.mark_cancel_requested(file, cx));
  if let Err(error) = stores.extracts.cancel(file) {
    cancel_failed(stores, &error, cx);
  }
}

/// A cancel that could not stop an `mkvextract`: its request is withdrawn,
/// so Cancel can be tried again, and a toast says so.
fn cancel_failed(stores: &Stores, error: &anyhow::Error, cx: &mut App) {
  log::error!("Failed to cancel: {error}");
  let Some(failed) = error.downcast_ref::<bme_core::extract::KillFailed>() else {
    return;
  };
  for (file, detail) in &failed.files {
    stores.queue.update(cx, |queue, cx| queue.cancel_failed(file, cx));
    let message = rust_i18n::t!("notification.cancelFailed", detail = detail).into_owned();
    stores.ui.update(cx, |ui, cx| {
      ui.show_toast(NotificationKind::Error, file.clone(), message, cx)
    });
  }
}

/// [`cancel_extract`] for each of `files`, the ones in the extract queue in
/// one request ([`ExtractBackend::cancel_many`](crate::extract_events::ExtractBackend::cancel_many)),
/// so a running extraction that ends meanwhile starts none of the waiting
/// ones.
pub fn cancel_extracts(stores: &Stores, files: &[String], cx: &mut App) {
  let now = SystemTime::now();
  let queued: Vec<String> = stores.queue.update(cx, |queue, cx| {
    let mut queued = Vec::new();
    for file in files {
      if !queue.cancel_reservation(file, now, cx) {
        queue.mark_cancel_requested(file, cx);
        queued.push(file.clone());
      }
    }
    queued
  });
  if queued.is_empty() {
    return;
  }
  if let Err(error) = stores.extracts.cancel_many(&queued) {
    cancel_failed(stores, &error, cx);
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
  let ui = stores.ui.clone();
  cx.spawn(async move |cx| {
    let commands = cx
      .background_spawn(async move { requests.iter().map(ExtractRequest::command).collect::<Vec<_>>() })
      .await;
    cx.update(|cx| {
      cx.write_to_clipboard(ClipboardItem::new_string(commands.join("\n")));
      ui.update(cx, |ui, cx| {
        // The copied command is quoted for one shell; on Windows, say which.
        let message = if cfg!(windows) {
          tr!("extract.commandCopiedForCommandPrompt")
        } else {
          tr!("extract.commandCopied")
        };
        ui.show_snackbar(NotificationKind::Success, message.to_string(), cx)
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
          .background_spawn(async move { tools.detect(Tool::BetterMediaInfo, path.trim(), Lookup::Installed) })
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
