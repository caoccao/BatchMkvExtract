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

//! Per-drive extract queue.
//!
//! Each drive (the Windows path prefix such as `C:`; a single `"default"`
//! bucket on Linux and macOS) runs at most one `mkvextract` at a time;
//! further files for that drive wait in FIFO order. Different drives extract
//! in parallel. Every extraction runs on its own plain thread.
//!
//! Progress and completion are pushed to an [`ExtractEventSink`]. For each
//! enqueued file the sink receives [`ExtractEvent::Started`], then any number
//! of [`ExtractEvent::Progress`], then exactly one [`ExtractEvent::Finished`].
//! A file cancelled while still waiting receives only `Finished` with
//! [`ExtractOutcome::Cancelled`]. One file's events come from one thread, in
//! order, and `Finished` of a file is sent before `Started` of the next file
//! on the same drive; events of different drives interleave.
//!
//! `mkvextract` exits with 0 on success, 1 when it extracted but printed
//! warnings, and 2 on an error. 0 and 1 complete the extraction (1 with its
//! warnings); anything else fails it with the error lines it printed. An
//! extraction ends as cancelled only when a cancel's kill ended
//! `mkvextract`. When `mkvextract` exited by itself first, even at the
//! instant of the kill, the outcome is that of its exit.
//!
//! The queue never creates, checks or removes an output. `mkvextract` gets
//! the arguments as they were built at enqueue time and handles the
//! destinations itself: it creates missing folders, replaces files that are
//! there, and leaves what it wrote when it is stopped or fails.
//!
//! The process-wide queue behind [`enqueue`], [`cancel`] and [`shutdown`]
//! launches the real `mkvextract` and forwards its events to the sink
//! installed by [`init`]. Tests build their own [`ExtractQueue`] with a fake
//! launcher.

use anyhow::Result;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::panic::{self, AssertUnwindSafe};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use crate::mkvtoolnix::{self, MkvextractLine};

const ERROR_TASK_MISSING: &str = "task missing";
const ERROR_WORKER_PANICKED: &str = "extract worker panicked";
/// At most this many warnings, and error or other lines, are kept per
/// extraction (the last ones).
const MAX_MESSAGES: usize = 10;
/// A kept line is cut after this many characters.
const MAX_MESSAGE_CHARS: usize = 1_000;
/// How often a worker checks whether its process has exited once the
/// process's output is closed.
const EXIT_POLL: Duration = Duration::from_millis(20);

/// How an extraction ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractOutcome {
  Completed,
  Cancelled,
  Failed,
}

impl ExtractOutcome {
  /// The outcome's name, the same as the queue status it ends in.
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Completed => "Completed",
      Self::Cancelled => "Cancelled",
      Self::Failed => "Failed",
    }
  }
}

/// Why an extraction failed. The app words it for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractError {
  /// The source file is missing or is not a file.
  SourceMissing { path: String },
  /// `mkvextract` could not be started from `path`; `detail` is the OS
  /// error.
  NotAvailable { path: String, detail: String },
  /// `mkvextract` ended without extracting: an exit code of 2 or more, or
  /// `None` when it was killed by something other than a cancel. `messages`
  /// are the last error lines it printed, or its last other lines (e.g. a
  /// loader error on stderr) when it printed no error line.
  Exited { code: Option<i32>, messages: Vec<String> },
  /// Any other failure, with a message shown as it is.
  Other(String),
}

impl std::fmt::Display for ExtractError {
  /// An English description, for logs.
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::SourceMissing { path } => write!(f, "the source file {path} does not exist"),
      Self::NotAvailable { path, detail } => write!(f, "mkvextract could not be started from {path}: {detail}"),
      Self::Exited { code, messages } => {
        match code {
          Some(code) => write!(f, "mkvextract exited with code {code}")?,
          None => write!(f, "mkvextract was terminated")?,
        }
        if !messages.is_empty() {
          write!(f, ": {}", messages.join(" "))?;
        }
        Ok(())
      }
      Self::Other(message) => f.write_str(message),
    }
  }
}

/// A change in one file's extraction, pushed to the [`ExtractEventSink`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtractEvent {
  /// The file left the queue and its `mkvextract` is being launched.
  Started { file: String, at: SystemTime },
  /// `mkvextract` reported a new percentage; sent only when the value
  /// changes.
  Progress { file: String, percent: u32 },
  /// The extraction ended. `error` is set when `outcome` is
  /// [`ExtractOutcome::Failed`]; `warnings` are the last warnings
  /// `mkvextract` printed (a completed extraction with warnings exited
  /// with 1).
  Finished {
    file: String,
    outcome: ExtractOutcome,
    error: Option<ExtractError>,
    warnings: Vec<String>,
    at: SystemTime,
  },
}

impl ExtractEvent {
  /// The source file the event is about.
  pub fn file(&self) -> &str {
    match self {
      Self::Started { file, .. } | Self::Progress { file, .. } | Self::Finished { file, .. } => file,
    }
  }
}

/// Receives every [`ExtractEvent`]. Called from worker threads (and from the
/// thread calling [`cancel`] for a waiting file), never with the queue lock
/// held. It must not panic and should return quickly, e.g. by sending into a
/// channel.
pub type ExtractEventSink = Box<dyn Fn(ExtractEvent) + Send + Sync>;

/// A launched extraction process.
pub struct ExtractProcess {
  /// The process's output (stdout and stderr), scanned for progress,
  /// warnings and errors ([`mkvtoolnix`]'s GUI-mode lines); `None` when it
  /// was not captured.
  pub output: Option<Box<dyn Read + Send>>,
  /// Kills the process and asks whether it has exited.
  pub control: Box<dyn ExtractProcessControl>,
}

/// Handle to a running extraction process.
pub trait ExtractProcessControl: Send {
  /// Stop the process; called on cancel while it is still running. The
  /// process may exit by itself at the same instant, so the exit that
  /// [`try_wait`](Self::try_wait) reports, not this call, says what ended
  /// it.
  fn kill(&mut self) -> std::io::Result<()>;
  /// `None` while the process runs. Once it has ended: `Some(Some(code))`
  /// when it exited by itself with `code`, `Some(None)` when a kill or a
  /// signal ended it. Must not block: the worker polls it without holding
  /// the lock that [`kill`](Self::kill) needs.
  fn try_wait(&mut self) -> std::io::Result<Option<Option<i32>>>;
}

/// Starts the extraction process for the source `file` with its
/// `mkvextract` arguments (`tracks …`, `chapters …`, `attachments …`).
pub type ExtractLauncher =
  Box<dyn Fn(&str, &[String]) -> std::result::Result<ExtractProcess, ExtractError> + Send + Sync>;

/// A launched process, and whether a cancel sent it a kill. Both are read
/// and written under one lock, so the worker that sees the process end also
/// knows whether a cancel tried to end it.
struct LaunchedProcess {
  control: Box<dyn ExtractProcessControl>,
  killed: bool,
}

type SharedProcess = Arc<Mutex<LaunchedProcess>>;

/// How a launched process ended.
struct Exit {
  /// The exit code; `None` when a kill or a signal ended it.
  code: Option<i32>,
  /// Whether a cancel sent it a kill. It may have exited by itself just
  /// before the kill arrived; `code` tells.
  killed: bool,
}

enum WorkerOutcome {
  Completed,
  Cancelled,
  Failed(ExtractError),
}

/// What an extraction printed besides its progress, capped at
/// [`MAX_MESSAGES`] per kind (the last ones).
#[derive(Default)]
struct PrintedMessages {
  warnings: VecDeque<String>,
  errors: VecDeque<String>,
  /// Lines that are neither progress, a warning nor an error.
  other: VecDeque<String>,
}

impl PrintedMessages {
  fn record(&mut self, line: MkvextractLine<'_>) {
    let (list, text) = match line {
      MkvextractLine::Progress(_) => return,
      MkvextractLine::Warning(text) => (&mut self.warnings, text),
      MkvextractLine::Error(text) => (&mut self.errors, text),
      MkvextractLine::Other(text) => (&mut self.other, text),
    };
    if list.len() == MAX_MESSAGES {
      list.pop_front();
    }
    list.push_back(text.chars().take(MAX_MESSAGE_CHARS).collect());
  }

  /// The error lines, or the other lines when there is no error line.
  fn failure_messages(&mut self) -> Vec<String> {
    let lines = if self.errors.is_empty() {
      &mut self.other
    } else {
      &mut self.errors
    };
    lines.drain(..).collect()
  }
}

enum TaskStatus {
  Queued,
  Extracting,
}

struct TaskState {
  status: TaskStatus,
  progress: u32,
  /// Taken by the worker when it starts.
  args: Option<Vec<String>>,
  cancel_requested: bool,
}

#[derive(Default)]
struct DriveState {
  extracting: Option<String>,
  queued: Vec<String>,
}

#[derive(Default)]
struct ExtractState {
  tasks: HashMap<String, TaskState>,
  drives: HashMap<String, DriveState>,
  children: HashMap<String, SharedProcess>,
}

/// A per-drive extract queue with its own launcher and event sink.
pub struct ExtractQueue {
  state: Mutex<ExtractState>,
  /// Notified whenever a task goes, for [`Self::shutdown`].
  task_removed: Condvar,
  launcher: ExtractLauncher,
  sink: ExtractEventSink,
}

impl ExtractQueue {
  pub fn new(launcher: ExtractLauncher, sink: ExtractEventSink) -> Arc<Self> {
    Arc::new(Self {
      state: Mutex::new(ExtractState::default()),
      task_removed: Condvar::new(),
      launcher,
      sink,
    })
  }

  /// Queue `file` for extraction with its `mkvextract` arguments. It starts
  /// at once when its drive is idle. A file that is already queued or
  /// extracting is ignored.
  pub fn enqueue(self: &Arc<Self>, file: String, args: Vec<String>) -> Result<()> {
    let drive_key = get_drive_key(&file);
    let should_start;
    {
      let mut st = self.lock_state();
      if st.tasks.contains_key(&file) {
        return Ok(());
      }
      st.tasks.insert(
        file.clone(),
        TaskState {
          status: TaskStatus::Queued,
          progress: 0,
          args: Some(args),
          cancel_requested: false,
        },
      );
      let drive = st.drives.entry(drive_key).or_default();
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
      self.spawn_worker(file);
    }
    Ok(())
  }

  /// Cancel `file`. A waiting file is dropped from its drive queue and
  /// finishes as `Cancelled` right away; a running extraction is killed and
  /// its worker reports `Cancelled`, unless `mkvextract` had exited by
  /// itself, whose outcome then stands. Unknown files are ignored.
  pub fn cancel(&self, file: &str) -> Result<()> {
    let process = {
      let mut st = self.lock_state();
      let is_extracting = match st.tasks.get(file) {
        Some(t) => matches!(t.status, TaskStatus::Extracting),
        None => return Ok(()),
      };
      if !is_extracting {
        st.tasks.remove(file);
        if let Some(drive) = st.drives.get_mut(&get_drive_key(file)) {
          drive.queued.retain(|f| f != file);
        }
        drop(st);
        self.task_removed.notify_all();
        self.emit_finished(file, WorkerOutcome::Cancelled, Vec::new());
        return Ok(());
      }
      if let Some(t) = st.tasks.get_mut(file) {
        t.cancel_requested = true;
      }
      st.children.get(file).cloned()
    };
    if let Some(process) = process {
      kill(&process);
    }
    Ok(())
  }

  /// Cancels every waiting and running extraction and waits up to `timeout`
  /// for the running ones to finish, so their processes are gone before the
  /// app exits. Returns whether every extraction finished in time.
  pub fn shutdown(&self, timeout: Duration) -> bool {
    let files: Vec<String> = self.lock_state().tasks.keys().cloned().collect();
    for file in &files {
      let _ = self.cancel(file);
    }
    let deadline = Instant::now() + timeout;
    let mut st = self.lock_state();
    while !st.tasks.is_empty() {
      let left = deadline.saturating_duration_since(Instant::now());
      if left.is_zero() {
        log::warn!("{} extraction(s) were still running at shutdown", st.tasks.len());
        return false;
      }
      st = self
        .task_removed
        .wait_timeout(st, left)
        .unwrap_or_else(PoisonError::into_inner)
        .0;
    }
    true
  }

  /// `(file, status, progress)` of the files that are waiting or
  /// extracting.
  #[cfg(test)]
  fn entries(&self) -> Vec<(String, String, u32)> {
    let st = self.lock_state();
    st.tasks
      .iter()
      .map(|(file, task)| {
        let status = match task.status {
          TaskStatus::Queued => "Waiting",
          TaskStatus::Extracting => "Extracting",
        };
        (file.clone(), status.to_owned(), task.progress)
      })
      .collect()
  }

  fn emit(&self, event: ExtractEvent) {
    (self.sink)(event);
  }

  fn emit_finished(&self, file: &str, outcome: WorkerOutcome, warnings: Vec<String>) {
    let (outcome, error) = match outcome {
      WorkerOutcome::Completed => (ExtractOutcome::Completed, None),
      WorkerOutcome::Cancelled => (ExtractOutcome::Cancelled, None),
      WorkerOutcome::Failed(error) => (ExtractOutcome::Failed, Some(error)),
    };
    self.emit(ExtractEvent::Finished {
      file: file.to_owned(),
      outcome,
      error,
      warnings,
      at: SystemTime::now(),
    });
  }

  /// Runs the extraction and decides its outcome; also returns the warnings
  /// it printed. Everything a worker does between `Started` and `Finished`
  /// happens here, so [`Self::run_worker`] can turn a panic into `Failed`
  /// without losing the `Finished` event.
  fn execute(&self, file: &str) -> (WorkerOutcome, Vec<String>) {
    let args = self.lock_state().tasks.get_mut(file).and_then(|t| t.args.take());
    let Some(args) = args else {
      return (
        WorkerOutcome::Failed(ExtractError::Other(ERROR_TASK_MISSING.to_owned())),
        Vec::new(),
      );
    };
    let ExtractProcess { output, control } = match (self.launcher)(file, &args) {
      Ok(process) => process,
      Err(err) => {
        log::error!("Couldn't start the extraction of {}: {}", file, err);
        return (WorkerOutcome::Failed(err), Vec::new());
      }
    };
    let process: SharedProcess = Arc::new(Mutex::new(LaunchedProcess { control, killed: false }));
    let cancelled_while_launching = {
      let mut st = self.lock_state();
      st.children.insert(file.to_owned(), Arc::clone(&process));
      st.tasks.get(file).is_some_and(|t| t.cancel_requested)
    };
    if cancelled_while_launching {
      // `cancel` ran before the process was registered, so nothing killed it.
      kill(&process);
    }
    let mut printed = PrintedMessages::default();
    if let Some(output) = output {
      mkvtoolnix::read_mkvextract_output(output, |line| match mkvtoolnix::parse_mkvextract_line(line) {
        Some(MkvextractLine::Progress(percent)) => self.record_progress(file, percent),
        Some(line) => printed.record(line),
        None => {}
      });
    }
    // `mkvextract` exited by itself whenever it has an exit code, even when
    // a cancel's kill came at that instant: its exit decides.
    let outcome = match wait_for_exit(&process) {
      // The tracks were extracted; with 1, the warnings say what to check.
      Ok(Exit { code: Some(0 | 1), .. }) => WorkerOutcome::Completed,
      Ok(Exit {
        code: None,
        killed: true,
      }) => WorkerOutcome::Cancelled,
      Ok(Exit { code, .. }) => WorkerOutcome::Failed(ExtractError::Exited {
        code,
        messages: printed.failure_messages(),
      }),
      Err(e) => WorkerOutcome::Failed(ExtractError::Other(e.to_string())),
    };
    (outcome, printed.warnings.into())
  }

  /// Drop the task, emit `Finished` and start the next file of its drive.
  ///
  /// The task goes before `Finished` is sent, so an enqueue of the same file
  /// that answers it (a retry) is a new task rather than a duplicate of this
  /// one. The drive stays taken until `Finished` is out: the retry waits on
  /// it, and no file of the drive starts before the report.
  fn finish(self: &Arc<Self>, file: &str, outcome: WorkerOutcome, warnings: Vec<String>) {
    // A launched process is registered until here, also when the worker
    // panicked after the launch.
    {
      let mut st = self.lock_state();
      st.tasks.remove(file);
      st.children.remove(file);
    }
    self.task_removed.notify_all();
    self.emit_finished(file, outcome, warnings);
    let next_file = {
      let mut st = self.lock_state();
      let drive_key = get_drive_key(file);
      let drive = st.drives.entry(drive_key).or_default();
      if drive.extracting.as_deref() == Some(file) {
        drive.extracting = None;
      }
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
      self.spawn_worker(next);
    }
  }

  fn kill_child(&self, file: &str) {
    let process = self.lock_state().children.get(file).cloned();
    if let Some(process) = process {
      kill(&process);
    }
  }

  /// The queue state stays consistent across a worker panic (every update is
  /// a few field writes), so a poisoned lock is recovered rather than
  /// propagated.
  fn lock_state(&self) -> MutexGuard<'_, ExtractState> {
    self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
  }

  fn record_progress(&self, file: &str, percent: u32) {
    let changed = {
      let mut st = self.lock_state();
      match st.tasks.get_mut(file) {
        Some(task) if task.progress != percent => {
          task.progress = percent;
          true
        }
        _ => false,
      }
    };
    if changed {
      self.emit(ExtractEvent::Progress {
        file: file.to_owned(),
        percent,
      });
    }
  }

  fn run_worker(self: &Arc<Self>, file: String) {
    self.emit(ExtractEvent::Started {
      file: file.clone(),
      at: SystemTime::now(),
    });
    let (outcome, warnings) = panic::catch_unwind(AssertUnwindSafe(|| self.execute(&file))).unwrap_or_else(|_| {
      log::error!("Extract worker for {} panicked", file);
      self.kill_child(&file);
      (
        WorkerOutcome::Failed(ExtractError::Other(ERROR_WORKER_PANICKED.to_owned())),
        Vec::new(),
      )
    });
    self.finish(&file, outcome, warnings);
  }

  fn spawn_worker(self: &Arc<Self>, file: String) {
    let queue = Arc::clone(self);
    std::thread::spawn(move || {
      queue.run_worker(file);
    });
  }
}

/// Kills the process unless it has exited by itself, and records that a kill
/// was sent.
fn kill(process: &SharedProcess) {
  let mut process = process.lock().unwrap_or_else(PoisonError::into_inner);
  if process.killed || matches!(process.control.try_wait(), Ok(Some(_))) {
    return;
  }
  match process.control.kill() {
    Ok(()) => process.killed = true,
    Err(err) => log::warn!("Couldn't stop mkvextract: {}", err),
  }
}

/// Waits for the process to exit, polling so that the lock is held only for
/// an instant and a cancel can always take it to kill the process.
fn wait_for_exit(process: &SharedProcess) -> std::io::Result<Exit> {
  loop {
    {
      let mut process = process.lock().unwrap_or_else(PoisonError::into_inner);
      if let Some(code) = process.control.try_wait()? {
        return Ok(Exit {
          code,
          killed: process.killed,
        });
      }
    }
    std::thread::sleep(EXIT_POLL);
  }
}

/// The per-drive queue a file belongs to: the Windows drive or share of its
/// path (`C:`, `\\SERVER\SHARE`, uppercased), else `default`. One drive
/// extracts one file at a time. The app groups its queue rows by the same
/// key.
pub fn get_drive_key(file: &str) -> String {
  #[cfg(target_os = "windows")]
  {
    let path = std::path::Path::new(file);
    if let Some(std::path::Component::Prefix(prefix)) = path.components().next() {
      return drive_key_of(prefix.kind());
    }
  }
  #[cfg(not(target_os = "windows"))]
  {
    let _ = file;
  }
  "default".to_string()
}

/// The drive key of a Windows path prefix. A verbatim spelling
/// (`\\?\C:\…`, `\\?\UNC\server\share\…`) is the same drive as the plain
/// one, so both share one queue instead of extracting side by side on one
/// disk.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn drive_key_of(prefix: std::path::Prefix<'_>) -> String {
  use std::path::Prefix;
  match prefix {
    Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => format!("{}:", char::from(letter).to_ascii_uppercase()),
    Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => format!(
      r"\\{}\{}",
      server.to_string_lossy().to_ascii_uppercase(),
      share.to_string_lossy().to_ascii_uppercase()
    ),
    Prefix::Verbatim(name) => format!(r"\\?\{}", name.to_string_lossy().to_ascii_uppercase()),
    Prefix::DeviceNS(name) => format!(r"\\.\{}", name.to_string_lossy().to_ascii_uppercase()),
  }
}

/// The exit code that a cancel on Windows terminates `mkvextract` with. A
/// terminated process exits with the code it was given, and `Child::kill`
/// gives 1, which is also `mkvextract`'s own exit with warnings. This code
/// is neither `mkvextract`'s (0, 1, 2) nor a crash's NTSTATUS (`0xC…`): it
/// has the customer bit of an application's own status set.
#[cfg(windows)]
const KILLED_EXIT_CODE: u32 = 0xE042_4D4D;

struct ChildControl(std::process::Child);

impl ExtractProcessControl for ChildControl {
  #[cfg(not(windows))]
  fn kill(&mut self) -> std::io::Result<()> {
    self.0.kill()
  }

  #[cfg(windows)]
  fn kill(&mut self) -> std::io::Result<()> {
    terminate(&mut self.0)
  }

  /// On Unix a kill leaves no exit code (a signal ended the process); on
  /// Windows it leaves [`KILLED_EXIT_CODE`]. Any other exit, also one at the
  /// instant of a kill, is the process's own.
  fn try_wait(&mut self) -> std::io::Result<Option<Option<i32>>> {
    let status = self.0.try_wait()?;
    #[cfg(windows)]
    let status = status.map(|status| status.code().filter(|&code| code as u32 != KILLED_EXIT_CODE));
    #[cfg(not(windows))]
    let status = status.map(|status| status.code());
    Ok(status)
  }
}

/// Terminates `child` with [`KILLED_EXIT_CODE`]. A process that has already
/// exited keeps its own exit code: `TerminateProcess` then fails with
/// "access denied", which is not an error here.
#[cfg(windows)]
fn terminate(child: &mut std::process::Child) -> std::io::Result<()> {
  use std::os::windows::io::AsRawHandle;

  #[link(name = "kernel32")]
  unsafe extern "system" {
    fn TerminateProcess(process: *mut std::ffi::c_void, exit_code: u32) -> i32;
  }

  // SAFETY: the handle is the child's own process handle, open while `child`
  // lives; `TerminateProcess` only reads it.
  if unsafe { TerminateProcess(child.as_raw_handle(), KILLED_EXIT_CODE) } != 0 {
    return Ok(());
  }
  let error = std::io::Error::last_os_error();
  match child.try_wait() {
    Ok(Some(_)) => Ok(()),
    _ => Err(error),
  }
}

fn launch_mkvextract(file: &str, args: &[String]) -> std::result::Result<ExtractProcess, ExtractError> {
  let (child, output) = mkvtoolnix::spawn_mkvextract(file, args)?;
  Ok(ExtractProcess {
    output: Some(Box::new(output)),
    control: Box::new(ChildControl(child)),
  })
}

static SINK: OnceLock<ExtractEventSink> = OnceLock::new();
static QUEUE: OnceLock<Arc<ExtractQueue>> = OnceLock::new();

fn global() -> &'static Arc<ExtractQueue> {
  QUEUE.get_or_init(|| {
    ExtractQueue::new(
      Box::new(launch_mkvextract),
      Box::new(|event| {
        if let Some(sink) = SINK.get() {
          sink(event);
        }
      }),
    )
  })
}

/// Install the event sink of the process-wide queue. Only the first call
/// counts; events sent before it are dropped.
pub fn init(sink: ExtractEventSink) {
  if SINK.set(sink).is_err() {
    log::warn!("extract::init called more than once; keeping the first sink");
  }
}

/// [`ExtractQueue::enqueue`] on the process-wide queue, which runs
/// `mkvextract`.
pub fn enqueue(file: String, args: Vec<String>) -> Result<()> {
  global().enqueue(file, args)
}

/// [`ExtractQueue::cancel`] on the process-wide queue.
pub fn cancel(file: String) -> Result<()> {
  global().cancel(&file)
}

/// [`ExtractQueue::shutdown`] on the process-wide queue.
pub fn shutdown(timeout: Duration) -> bool {
  global().shutdown(timeout)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::{PipeWriter, Write};
  use std::path::{Path, PathBuf};
  use std::sync::atomic::{AtomicBool, Ordering};
  use std::sync::mpsc::{self, Receiver, Sender};
  use std::time::{Duration, Instant};

  const TIMEOUT: Duration = Duration::from_secs(10);
  const A: &str = "/media/a.mkv";
  const B: &str = "/media/b.mkv";
  const C: &str = "/media/c.mkv";

  /// An event without its timestamp, for comparisons.
  #[derive(Debug, PartialEq, Eq)]
  enum Seen {
    Started(String),
    Progress(String, u32),
    Finished(String, ExtractOutcome, Option<ExtractError>, Vec<String>),
  }

  fn seen(event: ExtractEvent) -> Seen {
    match event {
      ExtractEvent::Started { file, .. } => Seen::Started(file),
      ExtractEvent::Progress { file, percent } => Seen::Progress(file, percent),
      ExtractEvent::Finished {
        file,
        outcome,
        error,
        warnings,
        ..
      } => Seen::Finished(file, outcome, error, warnings),
    }
  }

  fn started(file: &str) -> Seen {
    Seen::Started(file.to_owned())
  }

  fn progress(file: &str, percent: u32) -> Seen {
    Seen::Progress(file.to_owned(), percent)
  }

  fn finished(file: &str, outcome: ExtractOutcome, error: Option<ExtractError>) -> Seen {
    Seen::Finished(file.to_owned(), outcome, error, Vec::new())
  }

  fn other(message: &str) -> Option<ExtractError> {
    Some(ExtractError::Other(message.to_owned()))
  }

  fn exited(code: Option<i32>, messages: &[&str]) -> Option<ExtractError> {
    Some(ExtractError::Exited {
      code,
      messages: messages.iter().map(|message| message.to_string()).collect(),
    })
  }

  /// How the fake launcher treats a file.
  enum Launch {
    Run,
    /// Wait for [`Harness::release_launch`] before starting the process.
    Gated,
    /// A process that a kill does not end; it exits only when told to.
    IgnoresKill,
    Fail(ExtractError),
    Panic,
  }

  /// Shared between the fake process (worker side) and the test.
  struct FakeShared {
    writer: Mutex<Option<PipeWriter>>,
    /// `None` while running; `Some(code)` once ended (`Some(None)` = killed).
    exit: Mutex<Option<Option<i32>>>,
    killed: AtomicBool,
    ignores_kill: bool,
  }

  impl FakeShared {
    fn end(&self, code: Option<i32>) {
      {
        let mut exit = self.exit.lock().unwrap();
        if exit.is_none() {
          *exit = Some(code);
        }
      }
      // Closing the pipe is what lets the worker reach `wait`.
      self.writer.lock().unwrap().take();
    }
  }

  /// The first output file (`<id>:<path>` after `tracks`) of an
  /// `mkvextract` argument list.
  fn output_file(args: &[String]) -> Option<&str> {
    args
      .windows(2)
      .find(|pair| pair[0] == "tracks")
      .and_then(|pair| pair[1].split_once(':'))
      .map(|(_, path)| path)
  }

  /// Test side of one fake `mkvextract` run.
  struct FakeProcess {
    file: String,
    args: Vec<String>,
    shared: Arc<FakeShared>,
  }

  impl FakeProcess {
    fn report(&self, percent: u32) {
      self.print(&format!("#GUI#progress {percent}%\n"));
    }

    /// Writes `text` to the process's output as it is.
    fn print(&self, text: &str) {
      let mut writer = self.shared.writer.lock().unwrap();
      let writer = writer.as_mut().expect("process still running");
      writer.write_all(text.as_bytes()).unwrap();
      writer.flush().unwrap();
    }

    fn exit(&self, code: i32) {
      self.shared.end(Some(code));
    }

    fn was_killed(&self) -> bool {
      self.shared.killed.load(Ordering::SeqCst)
    }

    /// Waits for a cancel's kill. A cancel that comes before the worker has
    /// registered the process leaves the kill to the worker, a moment later.
    fn wait_for_kill(&self) {
      let deadline = Instant::now() + TIMEOUT;
      while !self.was_killed() {
        assert!(Instant::now() < deadline, "the kill was never sent");
        std::thread::sleep(Duration::from_millis(1));
      }
    }

    /// Writes `contents` to the file this run was told to extract to, as
    /// `mkvextract` would.
    fn write_output(&self, contents: &str) {
      std::fs::write(output_file(&self.args).expect("a tracks argument"), contents).unwrap();
    }
  }

  struct FakeControl(Arc<FakeShared>);

  impl ExtractProcessControl for FakeControl {
    fn kill(&mut self) -> std::io::Result<()> {
      self.0.killed.store(true, Ordering::SeqCst);
      if !self.0.ignores_kill {
        self.0.end(None);
      }
      Ok(())
    }

    fn try_wait(&mut self) -> std::io::Result<Option<Option<i32>>> {
      Ok(*self.0.exit.lock().unwrap())
    }
  }

  struct Harness {
    queue: Arc<ExtractQueue>,
    events: Receiver<ExtractEvent>,
    launched: Receiver<FakeProcess>,
    gate: Sender<()>,
  }

  impl Harness {
    fn new(plan: impl Fn(&str) -> Launch + Send + Sync + 'static) -> Self {
      let (event_tx, events) = mpsc::channel();
      let (launch_tx, launched) = mpsc::channel();
      let (gate, gate_rx) = mpsc::channel::<()>();
      let gate_rx = Mutex::new(gate_rx);
      let launcher: ExtractLauncher = Box::new(move |file, args| {
        let mut ignores_kill = false;
        match plan(file) {
          Launch::Run => {}
          Launch::Gated => {
            gate_rx.lock().unwrap().recv_timeout(TIMEOUT).expect("launch released");
          }
          Launch::IgnoresKill => ignores_kill = true,
          Launch::Fail(error) => return Err(error),
          Launch::Panic => panic!("fake launcher panicked"),
        }
        let (reader, writer) = std::io::pipe().expect("a pipe");
        let shared = Arc::new(FakeShared {
          writer: Mutex::new(Some(writer)),
          exit: Mutex::new(None),
          killed: AtomicBool::new(false),
          ignores_kill,
        });
        launch_tx
          .send(FakeProcess {
            file: file.to_owned(),
            args: args.to_vec(),
            shared: Arc::clone(&shared),
          })
          .unwrap();
        Ok(ExtractProcess {
          output: Some(Box::new(reader)),
          control: Box::new(FakeControl(shared)),
        })
      });
      let sink: ExtractEventSink = Box::new(move |event| {
        let _ = event_tx.send(event);
      });
      Self {
        queue: ExtractQueue::new(launcher, sink),
        events,
        launched,
        gate,
      }
    }

    fn enqueue(&self, file: &str) {
      self.queue.enqueue(file.to_owned(), args_for(file)).unwrap();
    }

    fn next(&self) -> Seen {
      seen(self.events.recv_timeout(TIMEOUT).expect("an extract event"))
    }

    fn launch(&self) -> FakeProcess {
      self.launched.recv_timeout(TIMEOUT).expect("a launched process")
    }

    fn release_launch(&self) {
      self.gate.send(()).unwrap();
    }

    fn assert_quiet(&self) {
      if let Ok(event) = self.events.recv_timeout(Duration::from_millis(100)) {
        panic!("unexpected event {event:?}");
      }
    }

    /// `(file, status, progress)` sorted by file.
    fn entries(&self) -> Vec<(String, String, u32)> {
      let mut entries = self.queue.entries();
      entries.sort();
      entries
    }

    /// The drive is freed after `Finished` is sent, so wait for that.
    fn wait_until_idle(&self) {
      let deadline = Instant::now() + TIMEOUT;
      while !self.queue.entries().is_empty() {
        assert!(Instant::now() < deadline, "queue never became idle");
        std::thread::sleep(Duration::from_millis(5));
      }
    }
  }

  /// Arguments that name no output of their own.
  fn args_for(file: &str) -> Vec<String> {
    vec!["chapters".to_owned(), format!("{file}.xml")]
  }

  fn entry(file: &str, status: &str, progress: u32) -> (String, String, u32) {
    (file.to_owned(), status.to_owned(), progress)
  }

  #[test]
  fn events_arrive_as_started_progress_finished() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    assert_eq!(a.file, A);
    assert_eq!(a.args, args_for(A));
    a.report(10);
    a.report(10);
    a.report(55);
    assert_eq!(h.next(), progress(A, 10));
    assert_eq!(h.next(), progress(A, 55));
    a.exit(0);
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    h.wait_until_idle();
    h.assert_quiet();
  }

  #[test]
  fn the_queue_lists_extracting_and_waiting_files_with_progress() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    h.enqueue(B);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    a.report(40);
    assert_eq!(h.next(), progress(A, 40));
    assert_eq!(h.entries(), vec![entry(A, "Extracting", 40), entry(B, "Waiting", 0)]);
    a.exit(0);
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    assert_eq!(h.next(), started(B));
    h.launch().exit(0);
    assert_eq!(h.next(), finished(B, ExtractOutcome::Completed, None));
    h.wait_until_idle();
  }

  #[test]
  fn one_drive_extracts_one_file_at_a_time_in_fifo_order() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    h.enqueue(B);
    h.enqueue(C);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    h.assert_quiet();
    a.exit(0);
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    assert_eq!(h.next(), started(B));
    let b = h.launch();
    assert_eq!(b.file, B);
    b.exit(3);
    assert_eq!(h.next(), finished(B, ExtractOutcome::Failed, exited(Some(3), &[])));
    assert_eq!(h.next(), started(C));
    h.launch().exit(0);
    assert_eq!(h.next(), finished(C, ExtractOutcome::Completed, None));
    h.wait_until_idle();
    h.assert_quiet();
  }

  #[test]
  fn enqueueing_a_queued_file_again_is_ignored() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    h.queue.enqueue(A.to_owned(), vec!["other".to_owned()]).unwrap();
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    assert_eq!(a.args, args_for(A));
    a.exit(0);
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    h.wait_until_idle();
    h.assert_quiet();
  }

  /// The app may enqueue a file again as soon as its `Finished` arrives; the
  /// sink here does it while the event is being sent. The retry extracts.
  #[test]
  fn a_file_enqueued_again_on_its_finished_event_extracts_again() {
    struct Done;
    impl ExtractProcessControl for Done {
      fn kill(&mut self) -> std::io::Result<()> {
        Ok(())
      }
      fn try_wait(&mut self) -> std::io::Result<Option<Option<i32>>> {
        Ok(Some(Some(0)))
      }
    }
    let launches = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
    let recorder = Arc::clone(&launches);
    let launcher: ExtractLauncher = Box::new(move |_, args| {
      recorder.lock().unwrap().push(args.to_vec());
      Ok(ExtractProcess {
        output: None,
        control: Box::new(Done),
      })
    });
    let queue_cell: Arc<OnceLock<std::sync::Weak<ExtractQueue>>> = Arc::new(OnceLock::new());
    let retry_queue = Arc::clone(&queue_cell);
    let retried = AtomicBool::new(false);
    let (event_tx, events) = mpsc::channel();
    let sink: ExtractEventSink = Box::new(move |event| {
      if matches!(event, ExtractEvent::Finished { .. }) && !retried.swap(true, Ordering::SeqCst) {
        let queue = retry_queue.get().and_then(std::sync::Weak::upgrade).expect("the queue");
        queue.enqueue(A.to_owned(), vec!["retry".to_owned()]).unwrap();
      }
      let _ = event_tx.send(event);
    });
    let queue = ExtractQueue::new(launcher, sink);
    queue_cell.set(Arc::downgrade(&queue)).unwrap();
    queue.enqueue(A.to_owned(), args_for(A)).unwrap();
    let mut events_seen = Vec::new();
    for _ in 0..4 {
      let event = events.recv_timeout(TIMEOUT).expect("an extract event");
      events_seen.push(seen(event));
    }
    let done = finished(A, ExtractOutcome::Completed, None);
    assert_eq!(
      events_seen,
      [
        started(A),
        finished(A, ExtractOutcome::Completed, None),
        started(A),
        done
      ]
    );
    assert_eq!(*launches.lock().unwrap(), [args_for(A), vec!["retry".to_owned()]]);
  }

  #[test]
  fn cancelling_a_running_extraction_kills_it() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    a.report(20);
    assert_eq!(h.next(), progress(A, 20));
    h.queue.cancel(A).unwrap();
    assert_eq!(h.next(), finished(A, ExtractOutcome::Cancelled, None));
    assert!(a.was_killed());
    h.wait_until_idle();
  }

  #[test]
  fn cancelling_a_waiting_file_finishes_it_without_starting_it() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    h.enqueue(B);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    h.queue.cancel(B).unwrap();
    assert_eq!(h.next(), finished(B, ExtractOutcome::Cancelled, None));
    assert_eq!(h.entries(), vec![entry(A, "Extracting", 0)]);
    a.exit(0);
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    h.wait_until_idle();
    h.assert_quiet();
  }

  #[test]
  fn cancelling_while_the_process_is_launching_still_kills_it() {
    let h = Harness::new(|_| Launch::Gated);
    h.enqueue(A);
    assert_eq!(h.next(), started(A));
    h.queue.cancel(A).unwrap();
    h.release_launch();
    let a = h.launch();
    assert_eq!(h.next(), finished(A, ExtractOutcome::Cancelled, None));
    assert!(a.was_killed());
    h.wait_until_idle();
  }

  #[test]
  fn cancelling_an_unknown_file_does_nothing() {
    let h = Harness::new(|_| Launch::Run);
    h.queue.cancel(A).unwrap();
    h.assert_quiet();
  }

  #[test]
  fn launch_failure_finishes_as_failed_and_starts_the_next_file() {
    let not_available = ExtractError::NotAvailable {
      path: "/opt/mkvextract".to_owned(),
      detail: "not found".to_owned(),
    };
    let failure = not_available.clone();
    let h = Harness::new(move |file| {
      if file == A {
        Launch::Fail(failure.clone())
      } else {
        Launch::Run
      }
    });
    h.enqueue(A);
    h.enqueue(B);
    assert_eq!(h.next(), started(A));
    assert_eq!(h.next(), finished(A, ExtractOutcome::Failed, Some(not_available)));
    assert_eq!(h.next(), started(B));
    h.launch().exit(0);
    assert_eq!(h.next(), finished(B, ExtractOutcome::Completed, None));
    h.wait_until_idle();
  }

  #[test]
  fn panicking_worker_finishes_as_failed_and_starts_the_next_file() {
    let h = Harness::new(|file| if file == A { Launch::Panic } else { Launch::Run });
    h.enqueue(A);
    h.enqueue(B);
    assert_eq!(h.next(), started(A));
    assert_eq!(
      h.next(),
      finished(A, ExtractOutcome::Failed, other(ERROR_WORKER_PANICKED))
    );
    assert_eq!(h.next(), started(B));
    h.launch().exit(0);
    assert_eq!(h.next(), finished(B, ExtractOutcome::Completed, None));
    h.wait_until_idle();
  }

  /// A worker that panics after the launch kills its process before it
  /// reports the failure, so no `mkvextract` is left running.
  #[test]
  fn a_worker_that_panics_after_the_launch_kills_its_process() {
    struct Exploding;
    impl Read for Exploding {
      fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
        panic!("the output exploded")
      }
    }
    let shared = Arc::new(FakeShared {
      writer: Mutex::new(None),
      exit: Mutex::new(None),
      killed: AtomicBool::new(false),
      ignores_kill: false,
    });
    let process = Arc::clone(&shared);
    let launcher: ExtractLauncher = Box::new(move |_, _| {
      Ok(ExtractProcess {
        output: Some(Box::new(Exploding)),
        control: Box::new(FakeControl(Arc::clone(&process))),
      })
    });
    let (event_tx, events) = mpsc::channel();
    let queue = ExtractQueue::new(
      launcher,
      Box::new(move |event| {
        let _ = event_tx.send(event);
      }),
    );
    queue.enqueue(A.to_owned(), args_for(A)).unwrap();
    assert_eq!(seen(events.recv_timeout(TIMEOUT).unwrap()), started(A));
    assert_eq!(
      seen(events.recv_timeout(TIMEOUT).unwrap()),
      finished(A, ExtractOutcome::Failed, other(ERROR_WORKER_PANICKED))
    );
    assert!(shared.killed.load(Ordering::SeqCst));
  }

  /// A process whose exit cannot be read fails the extraction with the
  /// error.
  #[test]
  fn an_unreadable_exit_fails_the_extraction() {
    struct Unknown;
    impl ExtractProcessControl for Unknown {
      fn kill(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("cannot kill"))
      }
      fn try_wait(&mut self) -> std::io::Result<Option<Option<i32>>> {
        Err(std::io::Error::other("no exit status"))
      }
    }
    let launcher: ExtractLauncher = Box::new(|_, _| {
      Ok(ExtractProcess {
        output: None,
        control: Box::new(Unknown),
      })
    });
    let (event_tx, events) = mpsc::channel();
    let queue = ExtractQueue::new(
      launcher,
      Box::new(move |event| {
        let _ = event_tx.send(event);
      }),
    );
    queue.enqueue(A.to_owned(), args_for(A)).unwrap();
    assert_eq!(seen(events.recv_timeout(TIMEOUT).unwrap()), started(A));
    assert_eq!(
      seen(events.recv_timeout(TIMEOUT).unwrap()),
      finished(A, ExtractOutcome::Failed, other("no exit status"))
    );
  }

  /// A cancel whose kill fails leaves the process to end by itself; its exit
  /// decides.
  #[test]
  fn a_kill_that_fails_leaves_the_outcome_to_the_exit() {
    struct Stubborn(Arc<Mutex<Option<Option<i32>>>>);
    impl ExtractProcessControl for Stubborn {
      fn kill(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::other("access denied"))
      }
      fn try_wait(&mut self) -> std::io::Result<Option<Option<i32>>> {
        Ok(*self.0.lock().unwrap())
      }
    }
    let exit = Arc::new(Mutex::new(None));
    let control_exit = Arc::clone(&exit);
    let launcher: ExtractLauncher = Box::new(move |_, _| {
      Ok(ExtractProcess {
        output: None,
        control: Box::new(Stubborn(Arc::clone(&control_exit))),
      })
    });
    let (event_tx, events) = mpsc::channel();
    let queue = ExtractQueue::new(
      launcher,
      Box::new(move |event| {
        let _ = event_tx.send(event);
      }),
    );
    queue.enqueue(A.to_owned(), args_for(A)).unwrap();
    assert_eq!(seen(events.recv_timeout(TIMEOUT).unwrap()), started(A));
    // Wait for the worker to register the process, then cancel.
    let deadline = Instant::now() + TIMEOUT;
    while !queue.lock_state().children.contains_key(A) {
      assert!(Instant::now() < deadline, "the process was never registered");
      std::thread::sleep(Duration::from_millis(1));
    }
    queue.cancel(A).unwrap();
    *exit.lock().unwrap() = Some(Some(0));
    assert_eq!(
      seen(events.recv_timeout(TIMEOUT).unwrap()),
      finished(A, ExtractOutcome::Completed, None)
    );
  }

  #[cfg(target_os = "windows")]
  #[test]
  fn different_drives_extract_in_parallel() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(r"C:\media\a.mkv");
    h.enqueue(r"D:\media\b.mkv");
    let mut first = vec![h.next(), h.next()];
    first.sort_by_key(|seen| format!("{seen:?}"));
    assert_eq!(first, vec![started(r"C:\media\a.mkv"), started(r"D:\media\b.mkv")]);
    let one = h.launch();
    let two = h.launch();
    one.exit(0);
    two.exit(0);
    h.next();
    h.next();
    h.wait_until_idle();
  }

  #[test]
  fn drive_key_groups_files_per_drive() {
    if cfg!(target_os = "windows") {
      assert_eq!(get_drive_key(r"c:\media\a.mkv"), "C:");
      assert_eq!(get_drive_key(r"\\nas\share\a.mkv"), r"\\NAS\SHARE");
    } else {
      assert_eq!(get_drive_key("/Volumes/one/a.mkv"), "default");
    }
    assert_eq!(get_drive_key("relative/a.mkv"), "default");
  }

  #[test]
  fn verbatim_and_plain_spellings_of_a_drive_share_its_key() {
    use std::ffi::OsStr;
    use std::path::Prefix;
    assert_eq!(drive_key_of(Prefix::Disk(b'c')), "C:");
    assert_eq!(drive_key_of(Prefix::VerbatimDisk(b'C')), "C:");
    let (server, share) = (OsStr::new("nas"), OsStr::new("Media"));
    assert_eq!(drive_key_of(Prefix::UNC(server, share)), r"\\NAS\MEDIA");
    assert_eq!(drive_key_of(Prefix::VerbatimUNC(server, share)), r"\\NAS\MEDIA");
    assert_eq!(drive_key_of(Prefix::DeviceNS(OsStr::new("pipe"))), r"\\.\PIPE");
    assert_eq!(
      drive_key_of(Prefix::Verbatim(OsStr::new("Volume{1}"))),
      r"\\?\VOLUME{1}"
    );
  }

  #[test]
  fn outcome_wire_names_match_queue_statuses() {
    assert_eq!(ExtractOutcome::Completed.as_str(), "Completed");
    assert_eq!(ExtractOutcome::Cancelled.as_str(), "Cancelled");
    assert_eq!(ExtractOutcome::Failed.as_str(), "Failed");
  }

  #[test]
  fn event_file_names_the_source_file() {
    let at = SystemTime::now();
    assert_eq!(ExtractEvent::Started { file: A.into(), at }.file(), A);
    assert_eq!(
      ExtractEvent::Progress {
        file: B.into(),
        percent: 1
      }
      .file(),
      B
    );
    let finished = ExtractEvent::Finished {
      file: C.into(),
      outcome: ExtractOutcome::Completed,
      error: None,
      warnings: Vec::new(),
      at,
    };
    assert_eq!(finished.file(), C);
  }

  /// A scratch directory for real output files, removed when dropped.
  struct ScratchDir(PathBuf);

  impl ScratchDir {
    fn new(name: &str) -> Self {
      let path = std::env::temp_dir().join(format!("bme_extract_{name}_{}", std::process::id()));
      let _ = std::fs::remove_dir_all(&path);
      std::fs::create_dir_all(&path).unwrap();
      Self(path)
    }

    fn output(&self, name: &str) -> String {
      self.0.join(name).to_string_lossy().into_owned()
    }
  }

  impl Drop for ScratchDir {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.0);
    }
  }

  fn args_writing(output: &str) -> Vec<String> {
    vec!["tracks".to_owned(), format!("0:{output}")]
  }

  #[test]
  fn gui_mode_progress_is_read_in_any_interface_language() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    a.print("Die Spur 0 mit der CodecID »S_TEXT/UTF8« wird in die Datei »a.srt« extrahiert.\n#GUI#progress 42%\n#GUI#begin_scanning_playlists\n");
    assert_eq!(h.next(), progress(A, 42));
    a.exit(0);
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    h.wait_until_idle();
  }

  #[test]
  fn exit_code_one_completes_with_the_warnings() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    a.print("#GUI#warning The track 2 has no codec private data.\n");
    a.print("Warning: an English warning without GUI mode\n");
    a.exit(1);
    assert_eq!(
      h.next(),
      Seen::Finished(
        A.to_owned(),
        ExtractOutcome::Completed,
        None,
        vec![
          "The track 2 has no codec private data.".to_owned(),
          "an English warning without GUI mode".to_owned(),
        ]
      )
    );
    h.wait_until_idle();
  }

  #[test]
  fn a_failure_carries_the_error_lines_or_else_the_last_output() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    h.enqueue(B);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    a.print("Extracting track 0 with the CodecID 'S_TEXT/UTF8' to the file 'a.srt'.\n");
    a.print("#GUI#error No track with the ID 5 was found in the source file.\n");
    a.exit(2);
    assert_eq!(
      h.next(),
      finished(
        A,
        ExtractOutcome::Failed,
        exited(Some(2), &["No track with the ID 5 was found in the source file."])
      )
    );
    assert_eq!(h.next(), started(B));
    let b = h.launch();
    b.print("mkvextract v101.0\ndyld: Library not loaded: libQt6Core\n");
    b.shared.end(None);
    assert_eq!(
      h.next(),
      finished(
        B,
        ExtractOutcome::Failed,
        exited(None, &["mkvextract v101.0", "dyld: Library not loaded: libQt6Core"])
      )
    );
    h.wait_until_idle();
  }

  #[test]
  fn at_most_the_last_ten_messages_of_a_kind_are_kept() {
    let mut printed = PrintedMessages::default();
    for index in 0..12 {
      printed.record(MkvextractLine::Warning(&format!("warning {index}")));
    }
    printed.record(MkvextractLine::Progress(5));
    printed.record(MkvextractLine::Other(&"x".repeat(MAX_MESSAGE_CHARS + 5)));
    let warnings: Vec<String> = printed.warnings.iter().cloned().collect();
    assert_eq!(warnings.len(), MAX_MESSAGES);
    assert_eq!(
      (warnings[0].as_str(), warnings[9].as_str()),
      ("warning 2", "warning 11")
    );
    assert_eq!(printed.failure_messages()[0].chars().count(), MAX_MESSAGE_CHARS);
  }

  /// A cancel's kill reaches `mkvextract` as it exits by itself. On Windows
  /// `TerminateProcess` then fails and `Child::kill` still returns `Ok`;
  /// here the fake ignores the kill and exits afterwards. The exit decides.
  #[test]
  fn a_kill_that_comes_as_mkvextract_exits_leaves_the_outcome_of_its_exit() {
    let h = Harness::new(|_| Launch::IgnoresKill);
    let cases = [
      (A, 0, ExtractOutcome::Completed, None),
      (B, 1, ExtractOutcome::Completed, None),
      (C, 2, ExtractOutcome::Failed, exited(Some(2), &[])),
    ];
    for (file, code, outcome, error) in cases {
      h.enqueue(file);
      assert_eq!(h.next(), started(file));
      let process = h.launch();
      h.queue.cancel(file).unwrap();
      process.wait_for_kill();
      process.exit(code);
      assert_eq!(h.next(), finished(file, outcome, error));
      h.wait_until_idle();
    }
  }

  fn read(path: impl AsRef<Path>) -> String {
    std::fs::read_to_string(path).unwrap()
  }

  /// `mkvextract` gets the output paths as they were given and handles the
  /// destination: nothing is created before the launch, not even a missing
  /// folder.
  #[test]
  fn mkvextract_gets_the_output_as_given_and_creates_it_itself() {
    let dir = ScratchDir::new("as_given");
    let folder = dir.0.join("new folder");
    let output = folder.join("Movie.2.eng.srt").to_string_lossy().into_owned();
    let h = Harness::new(|_| Launch::Run);
    h.queue.enqueue(A.to_owned(), args_writing(&output)).unwrap();
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    assert_eq!(a.args, args_writing(&output));
    assert!(!folder.exists(), "nothing is created before the launch");
    std::fs::create_dir_all(&folder).unwrap();
    a.write_output("extracted");
    a.exit(0);
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    assert_eq!(read(&output), "extracted");
    h.wait_until_idle();
  }

  /// An extraction removes no file: what `mkvextract` wrote before a cancel
  /// or a failure stays where it wrote it.
  #[test]
  fn a_cancelled_or_failed_extraction_leaves_what_mkvextract_wrote() {
    let dir = ScratchDir::new("leaves");
    let (cancelled, failed) = (dir.output("a.srt"), dir.output("b.srt"));
    let h = Harness::new(|_| Launch::Run);
    h.queue.enqueue(A.to_owned(), args_writing(&cancelled)).unwrap();
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    a.write_output("partial");
    h.queue.cancel(A).unwrap();
    assert_eq!(h.next(), finished(A, ExtractOutcome::Cancelled, None));
    assert!(a.was_killed());
    assert_eq!(read(&cancelled), "partial");
    h.queue.enqueue(B.to_owned(), args_writing(&failed)).unwrap();
    assert_eq!(h.next(), started(B));
    let b = h.launch();
    b.write_output("partial");
    b.exit(2);
    assert_eq!(h.next(), finished(B, ExtractOutcome::Failed, exited(Some(2), &[])));
    assert_eq!(read(&failed), "partial");
    h.wait_until_idle();
  }

  /// `mkvextract` exited with warnings, and the cancel came before the worker
  /// saw it: there is nothing to kill, and the extraction completes.
  #[test]
  fn a_cancel_after_mkvextract_exited_by_itself_keeps_its_output() {
    let dir = ScratchDir::new("late_cancel");
    let output = dir.output("a.srt");
    let h = Harness::new(|_| Launch::Run);
    h.queue.enqueue(A.to_owned(), args_writing(&output)).unwrap();
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    a.write_output("extracted with warnings");
    // Exited, with its output still open: the worker is still reading it.
    *a.shared.exit.lock().unwrap() = Some(Some(1));
    h.queue.cancel(A).unwrap();
    assert!(!a.was_killed(), "nothing to kill");
    a.shared.writer.lock().unwrap().take();
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    assert_eq!(read(&output), "extracted with warnings");
    h.wait_until_idle();
  }

  #[test]
  fn shutdown_cancels_everything_and_waits_for_the_running_extractions() {
    let h = Harness::new(|_| Launch::Run);
    h.enqueue(A);
    h.enqueue(B);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    assert!(h.queue.shutdown(TIMEOUT));
    assert!(a.was_killed());
    assert!(h.entries().is_empty());
    let mut ends = vec![h.next(), h.next()];
    ends.sort_by_key(|seen| format!("{seen:?}"));
    assert_eq!(
      ends,
      [
        finished(A, ExtractOutcome::Cancelled, None),
        finished(B, ExtractOutcome::Cancelled, None)
      ]
    );
    h.assert_quiet();
  }

  #[test]
  fn shutdown_gives_up_after_the_timeout() {
    let h = Harness::new(|_| Launch::IgnoresKill);
    h.enqueue(A);
    assert_eq!(h.next(), started(A));
    let a = h.launch();
    assert!(!h.queue.shutdown(Duration::from_millis(50)));
    a.exit(0);
    assert_eq!(h.next(), finished(A, ExtractOutcome::Completed, None));
    h.wait_until_idle();
  }

  /// A process without captured output: the worker waits for it from the
  /// start, and a cancel must still get the lock to kill it.
  #[test]
  fn cancel_does_not_wait_for_a_process_without_output() {
    let shared = Arc::new(FakeShared {
      writer: Mutex::new(None),
      exit: Mutex::new(None),
      killed: AtomicBool::new(false),
      ignores_kill: false,
    });
    let process = Arc::clone(&shared);
    let launcher: ExtractLauncher = Box::new(move |_, _| {
      Ok(ExtractProcess {
        output: None,
        control: Box::new(FakeControl(Arc::clone(&process))),
      })
    });
    let (event_tx, events) = mpsc::channel();
    let queue = ExtractQueue::new(
      launcher,
      Box::new(move |event| {
        let _ = event_tx.send(event);
      }),
    );
    queue.enqueue(A.to_owned(), args_for(A)).unwrap();
    assert!(matches!(events.recv_timeout(TIMEOUT), Ok(ExtractEvent::Started { .. })));
    std::thread::sleep(EXIT_POLL * 3);
    let (done_tx, done) = mpsc::channel();
    let canceller = Arc::clone(&queue);
    std::thread::spawn(move || {
      canceller.cancel(A).unwrap();
      done_tx.send(()).unwrap();
    });
    done.recv_timeout(TIMEOUT).expect("cancel returned");
    assert!(shared.killed.load(Ordering::SeqCst));
    match events.recv_timeout(TIMEOUT) {
      Ok(ExtractEvent::Finished { outcome, .. }) => assert_eq!(outcome, ExtractOutcome::Cancelled),
      other => panic!("expected Finished, got {other:?}"),
    }
  }

  #[test]
  fn the_output_file_is_the_path_after_the_track_id() {
    let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
    assert_eq!(
      output_file(&args(&["tracks", "0:/out/a.srt", "1:/out/b.ac3"])),
      Some("/out/a.srt")
    );
    assert_eq!(output_file(&args(&["chapters", "/out/a.xml"])), None);
    assert_eq!(output_file(&args(&["tracks"])), None);
  }

  #[test]
  fn extract_errors_describe_themselves_for_logs() {
    assert_eq!(
      ExtractError::SourceMissing { path: "/a.mkv".into() }.to_string(),
      "the source file /a.mkv does not exist"
    );
    assert_eq!(
      exited(Some(2), &["one.", "two."]).unwrap().to_string(),
      "mkvextract exited with code 2: one. two."
    );
    assert_eq!(exited(None, &[]).unwrap().to_string(), "mkvextract was terminated");
    assert_eq!(
      ExtractError::NotAvailable {
        path: "/opt/mkvextract".into(),
        detail: "not found".into()
      }
      .to_string(),
      "mkvextract could not be started from /opt/mkvextract: not found"
    );
    assert_eq!(other("as it is").unwrap().to_string(), "as it is");
  }

  /// The control of a real process: its own exit is its code; a kill
  /// leaves none on Unix.
  #[cfg(unix)]
  #[test]
  fn a_real_process_reports_its_exit_or_its_kill() {
    let wait = |control: &mut ChildControl| {
      let deadline = Instant::now() + TIMEOUT;
      loop {
        if let Some(exit) = control.try_wait().unwrap() {
          return exit;
        }
        assert!(Instant::now() < deadline, "the process never ended");
        std::thread::sleep(Duration::from_millis(5));
      }
    };
    let spawn = |script: &str| {
      ChildControl(
        std::process::Command::new("/bin/sh")
          .arg("-c")
          .arg(script)
          .spawn()
          .unwrap(),
      )
    };
    assert_eq!(wait(&mut spawn("exit 3")), Some(3));
    let mut sleeping = spawn("sleep 30");
    assert_eq!(sleeping.try_wait().unwrap(), None);
    sleeping.kill().unwrap();
    assert_eq!(wait(&mut sleeping), None);
  }

  /// The process-wide queue: its sink gets the events, and a source that
  /// does not exist fails before any `mkvextract` is looked for.
  #[test]
  fn the_process_wide_queue_reports_through_the_installed_sink() {
    let (event_tx, events) = mpsc::channel();
    init(Box::new(move |event| {
      let _ = event_tx.send(event);
    }));
    init(Box::new(|_| {}));
    let missing = "/definitely/not/here/bme.mkv";
    cancel(missing.to_owned()).unwrap();
    assert!(shutdown(Duration::from_millis(10)), "nothing to wait for");
    enqueue(missing.to_owned(), args_for(missing)).unwrap();
    assert_eq!(seen(events.recv_timeout(TIMEOUT).unwrap()), started(missing));
    assert_eq!(
      seen(events.recv_timeout(TIMEOUT).unwrap()),
      finished(
        missing,
        ExtractOutcome::Failed,
        Some(ExtractError::SourceMissing {
          path: missing.to_owned()
        })
      )
    );
  }
}
