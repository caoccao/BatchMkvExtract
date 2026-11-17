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

//! Launcher progress through the `com.canonical.Unity.LauncherEntry.Update`
//! session-bus signal, which KDE Plasma, GNOME's Dash to Dock / Ubuntu Dock and
//! Unity honour. The signal names the application by its desktop file, so a
//! run whose desktop id does not match an installed `.desktop` file (an
//! AppImage, `cargo run`) shows nothing. That is best effort by design.
//!
//! D-Bus calls run on a dedicated thread so a slow or missing session bus
//! never blocks the UI thread.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use raw_window_handle::HasWindowHandle;
use zbus::zvariant::Value;

const INTERFACE: &str = "com.canonical.Unity.LauncherEntry";
const OBJECT_PATH: &str = "/com/canonical/unity/launcherentry/batchmkvextract";

/// How long dropping the progress waits for the D-Bus thread to send what
/// is queued (the hide on quit) before the process may exit.
const DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

pub(super) struct Progress {
  sender: Option<Sender<Option<u8>>>,
  /// Disconnects once the D-Bus thread has ended.
  finished: Option<Receiver<()>>,
}

impl Progress {
  pub(super) fn new(_window: &impl HasWindowHandle, desktop_id: &str) -> Self {
    let app_uri = format!("application://{desktop_id}.desktop");
    let (sender, receiver) = mpsc::channel::<Option<u8>>();
    let (finished_sender, finished) = mpsc::channel::<()>();
    let spawned = thread::Builder::new().name("launcher-progress".into()).spawn(move || {
      // Dropped when the thread ends, which disconnects `finished`.
      let _finished = finished_sender;
      let connection = match zbus::blocking::Connection::session() {
        Ok(connection) => connection,
        Err(error) => {
          log::warn!("Launcher progress disabled, no session bus: {error}");
          return;
        }
      };
      while let Ok(percent) = receiver.recv() {
        if let Err(error) = emit_update(&connection, &app_uri, percent) {
          log::warn!("Failed to emit launcher progress: {error}");
        }
      }
    });
    match spawned {
      Ok(_) => Self {
        sender: Some(sender),
        finished: Some(finished),
      },
      Err(error) => {
        log::warn!("Failed to start the launcher progress thread: {error}");
        Self {
          sender: None,
          finished: None,
        }
      }
    }
  }

  pub(super) fn set(&mut self, percent: Option<u8>) {
    if let Some(sender) = &self.sender {
      let _ = sender.send(percent);
    }
  }
}

impl Drop for Progress {
  /// Closes the channel, so the thread ends once it has emitted what is
  /// queued, including the hide `TaskbarProgress` sends when it is dropped,
  /// and waits for that briefly: the process may exit right after (quitting
  /// mid-extraction), which would leave the launcher icon with its bar.
  fn drop(&mut self) {
    self.sender.take();
    if let Some(finished) = self.finished.take() {
      let _ = finished.recv_timeout(DRAIN_TIMEOUT);
    }
  }
}

fn emit_update(connection: &zbus::blocking::Connection, app_uri: &str, percent: Option<u8>) -> zbus::Result<()> {
  let mut properties: HashMap<&str, Value<'_>> = HashMap::new();
  properties.insert("progress", Value::from(f64::from(percent.unwrap_or(0)) / 100.0));
  properties.insert("progress-visible", Value::from(percent.is_some()));
  connection.emit_signal(None::<&str>, OBJECT_PATH, INTERFACE, "Update", &(app_uri, properties))
}

#[cfg(test)]
mod tests {
  use super::*;
  use raw_window_handle::{HandleError, WindowHandle};

  /// The launcher progress does not use the window.
  struct NoWindow;

  impl HasWindowHandle for NoWindow {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
      Err(HandleError::Unavailable)
    }
  }

  /// With or without a session bus, dropping sends what is queued and
  /// returns, at the latest after the drain timeout.
  #[test]
  fn dropping_the_progress_waits_only_briefly_for_its_thread() {
    let mut progress = Progress::new(&NoWindow, "BatchMkvExtractTest");
    progress.set(Some(40));
    progress.set(None);
    let started = std::time::Instant::now();
    drop(progress);
    assert!(started.elapsed() < DRAIN_TIMEOUT + Duration::from_secs(2));
  }
}
