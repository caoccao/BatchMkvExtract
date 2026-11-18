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

//! The application log: `BatchMkvExtract.log` next to the config file, also
//! written to stderr when one is attached. Each start moves the previous run's
//! log to `BatchMkvExtract.previous.log`, so the log of a run that crashed is
//! still there after the restart. The app's own crates log from `info` up,
//! every other crate (GPUI, wgpu, …) from `warn` up. A panic is logged too.

use std::fmt::Arguments;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

/// The log of the current run.
pub const LOG_FILE: &str = "BatchMkvExtract.log";
/// The log of the run before.
pub const PREVIOUS_LOG_FILE: &str = "BatchMkvExtract.previous.log";
/// The crates whose `info` messages are kept.
const OWN_CRATES: [&str; 3] = ["batch_mkvextract", "bme_core", "bme_platform"];

struct Logger {
  file: Mutex<Option<File>>,
}

impl Log for Logger {
  fn enabled(&self, metadata: &Metadata) -> bool {
    is_logged(metadata.target(), metadata.level())
  }

  fn log(&self, record: &Record) {
    if !self.enabled(record.metadata()) {
      return;
    }
    let line = format_line(&chrono::Local::now(), record.level(), record.target(), record.args());
    // Without a console (the Windows GUI subsystem) stderr is gone; ignore it.
    let _ = std::io::stderr().write_all(line.as_bytes());
    if let Ok(mut file) = self.file.lock()
      && let Some(file) = file.as_mut()
    {
      let _ = file.write_all(line.as_bytes());
    }
  }

  fn flush(&self) {
    if let Ok(mut file) = self.file.lock()
      && let Some(file) = file.as_mut()
    {
      let _ = file.flush();
    }
  }
}

/// Installs the logger and the panic hook; returns the log file's path when
/// it could be created. Call once, before anything logs.
pub fn install(dir: &Path) -> Option<PathBuf> {
  let file = open_log_file(dir);
  let path = file.as_ref().map(|_| dir.join(LOG_FILE));
  let logger: &'static Logger = Box::leak(Box::new(Logger { file: Mutex::new(file) }));
  if log::set_logger(logger).is_ok() {
    log::set_max_level(LevelFilter::Info);
  }
  let default_hook = std::panic::take_hook();
  std::panic::set_hook(Box::new(move |info| {
    let thread = std::thread::current();
    log::error!("Thread '{}' {}", thread.name().unwrap_or("<unnamed>"), info);
    default_hook(info);
  }));
  path
}

/// Moves the last run's log aside and creates a new one in `dir`.
fn open_log_file(dir: &Path) -> Option<File> {
  let path = dir.join(LOG_FILE);
  if path.exists() {
    let _ = std::fs::rename(&path, dir.join(PREVIOUS_LOG_FILE));
  }
  File::create(&path).ok()
}

/// Whether a message of `target` at `level` is kept.
fn is_logged(target: &str, level: Level) -> bool {
  let crate_name = target.split("::").next().unwrap_or(target);
  let threshold = if OWN_CRATES.contains(&crate_name) {
    Level::Info
  } else {
    Level::Warn
  };
  level <= threshold
}

/// One line of the log: local time, level, target, message.
fn format_line(time: &chrono::DateTime<chrono::Local>, level: Level, target: &str, message: &Arguments) -> String {
  format!(
    "{} {:<5} {}: {}\n",
    time.format("%Y-%m-%d %H:%M:%S%.3f"),
    level,
    target,
    message
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use chrono::TimeZone;

  #[test]
  fn the_apps_crates_log_info_and_the_others_only_warnings() {
    assert!(is_logged("bme_core::extract", Level::Info));
    assert!(is_logged("batch_mkvextract", Level::Info));
    assert!(is_logged("bme_platform::taskbar", Level::Warn));
    assert!(!is_logged("bme_core::extract", Level::Debug));
    assert!(!is_logged("wgpu_core::device", Level::Info));
    assert!(is_logged("gpui::window", Level::Warn));
    assert!(!is_logged("bme_core_extra", Level::Info));
  }

  #[test]
  fn a_line_has_the_local_time_level_target_and_message() {
    let time = chrono::Local.with_ymd_and_hms(2026, 11, 16, 9, 5, 7).unwrap();
    assert_eq!(
      format_line(&time, Level::Warn, "bme_core::extract", &format_args!("a {}", 1)),
      "2026-11-16 09:05:07.000 WARN  bme_core::extract: a 1\n"
    );
  }

  #[test]
  fn each_run_keeps_the_previous_log() {
    let dir = std::env::temp_dir().join(format!("bme_logging_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    open_log_file(&dir).unwrap().write_all(b"first run").unwrap();
    open_log_file(&dir).unwrap().write_all(b"second run").unwrap();
    assert_eq!(std::fs::read_to_string(dir.join(LOG_FILE)).unwrap(), "second run");
    assert_eq!(
      std::fs::read_to_string(dir.join(PREVIOUS_LOG_FILE)).unwrap(),
      "first run"
    );
    let _ = std::fs::remove_dir_all(&dir);
  }
}
