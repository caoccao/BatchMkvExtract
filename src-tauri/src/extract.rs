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

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

use crate::mkvtoolnix;
use crate::protocol::{ExtractEntry, ExtractSnapshot, ExtractionFinishedEvent};

const STATUS_WAITING: &str = "Waiting";
const STATUS_EXTRACTING: &str = "Extracting";
const OUTCOME_COMPLETED: &str = "Completed";
const OUTCOME_CANCELLED: &str = "Cancelled";
const OUTCOME_FAILED: &str = "Failed";
const EVENT_EXTRACTION_FINISHED: &str = "extraction-finished";

enum WorkerOutcome {
  Completed,
  Cancelled,
  Failed(String),
}

static APP_HANDLE: OnceLock<AppHandle> = OnceLock::new();

pub fn init_app_handle(handle: AppHandle) {
  let _ = APP_HANDLE.set(handle);
}

enum TaskStatus {
  Queued,
  Extracting,
}

struct TaskState {
  status: TaskStatus,
  progress: u32,
  args: Vec<String>,
  cancel_requested: bool,
}

#[derive(Default)]
struct DriveState {
  extracting: Option<String>,
  queued: Vec<String>,
}

struct ExtractState {
  tasks: HashMap<String, TaskState>,
  drives: HashMap<String, DriveState>,
  children: HashMap<String, Arc<Mutex<std::process::Child>>>,
}

impl ExtractState {
  fn new() -> Self {
    Self {
      tasks: HashMap::new(),
      drives: HashMap::new(),
      children: HashMap::new(),
    }
  }
}

static STATE: OnceLock<Mutex<ExtractState>> = OnceLock::new();

fn state() -> &'static Mutex<ExtractState> {
  STATE.get_or_init(|| Mutex::new(ExtractState::new()))
}

fn get_drive_key(file: &str) -> String {
  #[cfg(target_os = "windows")]
  {
    let path = std::path::Path::new(file);
    if let Some(first) = path.components().next() {
      if let std::path::Component::Prefix(prefix) = first {
        return prefix.as_os_str().to_string_lossy().to_ascii_uppercase();
      }
    }
  }
  #[cfg(not(target_os = "windows"))]
  {
    let _ = file;
  }
  "default".to_string()
}

pub fn enqueue(file: String, args: Vec<String>) -> Result<()> {
  let drive_key = get_drive_key(&file);
  let should_start;
  {
    let mut st = state().lock().unwrap();
    if st.tasks.contains_key(&file) {
      return Ok(());
    }
    st.tasks.insert(
      file.clone(),
      TaskState {
        status: TaskStatus::Queued,
        progress: 0,
        args,
        cancel_requested: false,
      },
    );
    let drive = st.drives.entry(drive_key.clone()).or_default();
    if drive.extracting.is_none() {
      drive.extracting = Some(file.clone());
      if let Some(t) = st.tasks.get_mut(&file) {
        t.status = TaskStatus::Extracting;
      }
      should_start = true;
    } else {
      drive.queued.push(file.clone());
      should_start = false;
    }
  }
  if should_start {
    spawn_worker(file);
  }
  Ok(())
}

pub fn cancel(file: String) -> Result<()> {
  let child_to_kill: Option<Arc<Mutex<std::process::Child>>>;
  {
    let mut st = state().lock().unwrap();
    let is_extracting = match st.tasks.get(&file) {
      Some(t) => matches!(t.status, TaskStatus::Extracting),
      None => return Ok(()),
    };
    if !is_extracting {
      st.tasks.remove(&file);
      let drive_key = get_drive_key(&file);
      if let Some(drive) = st.drives.get_mut(&drive_key) {
        drive.queued.retain(|f| f != &file);
      }
      return Ok(());
    }
    if let Some(t) = st.tasks.get_mut(&file) {
      t.cancel_requested = true;
    }
    child_to_kill = st.children.get(&file).cloned();
  }
  if let Some(child) = child_to_kill {
    if let Ok(mut guard) = child.lock() {
      let _ = guard.kill();
    }
  }
  Ok(())
}

pub fn snapshot() -> ExtractSnapshot {
  let st = state().lock().unwrap();
  let entries: Vec<ExtractEntry> = st
    .tasks
    .iter()
    .map(|(file, task)| ExtractEntry {
      file: file.clone(),
      status: match task.status {
        TaskStatus::Queued => STATUS_WAITING.to_owned(),
        TaskStatus::Extracting => STATUS_EXTRACTING.to_owned(),
      },
      progress: task.progress,
    })
    .collect();
  ExtractSnapshot { entries }
}

fn emit_finished(file: &str, outcome: &WorkerOutcome) {
  let Some(handle) = APP_HANDLE.get() else {
    return;
  };
  let (outcome_str, error) = match outcome {
    WorkerOutcome::Completed => (OUTCOME_COMPLETED, None),
    WorkerOutcome::Cancelled => (OUTCOME_CANCELLED, None),
    WorkerOutcome::Failed(msg) => (OUTCOME_FAILED, Some(msg.clone())),
  };
  let payload = ExtractionFinishedEvent {
    file: file.to_string(),
    outcome: outcome_str.to_string(),
    error,
  };
  if let Err(err) = handle.emit(EVENT_EXTRACTION_FINISHED, payload) {
    log::error!("Failed to emit extraction-finished event: {}", err);
  }
}

fn spawn_worker(file: String) {
  std::thread::spawn(move || {
    run_worker(file);
  });
}

fn run_worker(file: String) {
  let args: Option<Vec<String>> = {
    let st = state().lock().unwrap();
    st.tasks.get(&file).map(|t| t.args.clone())
  };
  let args = match args {
    Some(a) => a,
    None => {
      on_worker_finished(&file, WorkerOutcome::Failed("task missing".to_owned()));
      return;
    }
  };

  let outcome = match mkvtoolnix::spawn_mkvextract(&file, &args) {
    Err(err) => {
      log::error!("spawn_mkvextract failed for {}: {}", file, err);
      WorkerOutcome::Failed(err.to_string())
    }
    Ok(mut child) => {
      let stdout = child.stdout.take();
      let child_arc = Arc::new(Mutex::new(child));
      {
        let mut st = state().lock().unwrap();
        st.children.insert(file.clone(), child_arc.clone());
      }
      if let Some(stdout) = stdout {
        let file_for_cb = file.clone();
        mkvtoolnix::read_mkvextract_output(stdout, |line| {
          if let Some(percent) = mkvtoolnix::parse_mkvextract_progress(line) {
            let mut st = state().lock().unwrap();
            if let Some(t) = st.tasks.get_mut(&file_for_cb) {
              t.progress = percent;
            }
          }
        });
      }
      let exit = match child_arc.lock() {
        Ok(mut guard) => guard.wait(),
        Err(_) => Err(std::io::Error::new(std::io::ErrorKind::Other, "child lock poisoned")),
      };
      let cancel_requested = {
        let st = state().lock().unwrap();
        st.tasks.get(&file).map(|t| t.cancel_requested).unwrap_or(false)
      };
      if cancel_requested {
        WorkerOutcome::Cancelled
      } else {
        match exit {
          Ok(status) if status.success() => WorkerOutcome::Completed,
          Ok(status) => WorkerOutcome::Failed(format!("mkvextract exited with code {:?}", status.code())),
          Err(e) => WorkerOutcome::Failed(e.to_string()),
        }
      }
    }
  };

  on_worker_finished(&file, outcome);
}

fn on_worker_finished(file: &str, outcome: WorkerOutcome) {
  emit_finished(file, &outcome);
  let next_file = {
    let mut st = state().lock().unwrap();
    st.children.remove(file);
    st.tasks.remove(file);
    let drive_key = get_drive_key(file);
    let drive = st.drives.entry(drive_key).or_default();
    if drive.extracting.as_deref() == Some(file) {
      drive.extracting = None;
    }
    drive.queued.retain(|f| f != file);
    if drive.extracting.is_none() && !drive.queued.is_empty() {
      let next = drive.queued.remove(0);
      drive.extracting = Some(next.clone());
      if let Some(t) = st.tasks.get_mut(&next) {
        t.status = TaskStatus::Extracting;
        t.progress = 0;
      }
      Some(next)
    } else {
      None
    }
  };
  if let Some(next) = next_file {
    spawn_worker(next);
  }
}
