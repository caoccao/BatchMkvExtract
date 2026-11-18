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

//! The two external tools of the Integration settings: where MKVToolNix and
//! BetterMediaInfo are. Detection never writes the config; the caller stores a
//! found path through the config store. The same holds for the MKVToolNix
//! directory a track listing or an extraction finds by itself
//! ([`auto_detected_channel`]).

use std::sync::Arc;

use async_channel::Receiver;
use bme_core::config::{Config, ConfigIntegration};
use bme_core::mkvtoolnix::{AutoDetected, AutoDetectedSink};
use gpui_kit::{App, Entity};

use crate::state::ConfigStore;

/// An external tool with a path in `config.integration`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tool {
  MkvToolNix,
  BetterMediaInfo,
}

impl Tool {
  /// A stable name for element ids.
  pub fn name(self) -> &'static str {
    match self {
      Self::MkvToolNix => "mkvtoolnix",
      Self::BetterMediaInfo => "better-media-info",
    }
  }

  /// The configured path.
  pub fn path(self, config: &Config) -> &str {
    match self {
      Self::MkvToolNix => &config.integration.mkv_toolnix_path,
      Self::BetterMediaInfo => &config.integration.better_media_info_path,
    }
  }

  pub fn set_path(self, config: &mut Config, path: String) {
    match self {
      Self::MkvToolNix => config.integration.mkv_toolnix_path = path,
      Self::BetterMediaInfo => config.integration.better_media_info_path = path,
    }
  }

  /// The path of a fresh config (the platform's MKVToolNix directory; no
  /// BetterMediaInfo path).
  pub fn default_path(self) -> String {
    let defaults = ConfigIntegration::default();
    match self {
      Self::MkvToolNix => defaults.mkv_toolnix_path,
      Self::BetterMediaInfo => defaults.better_media_info_path,
    }
  }
}

/// The outcome of a detection: whether the tool is there, and the directory
/// to keep (it can differ from the given path: a running application's
/// directory, a versioned macOS bundle, the folder of a given executable).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolStatus {
  pub found: bool,
  pub path: String,
}

/// Where a detection looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lookup {
  /// The path only: its directory, the folder of a given executable, and on
  /// macOS the newest versioned MKVToolNix bundle for MKVToolNix's default
  /// path. What a path being typed is checked with: whatever it finds comes
  /// from the text itself.
  Path,
  /// The path, then the usual install places (BetterMediaInfo; MKVToolNix
  /// has none). What the cards' Open in BetterMediaInfo follows.
  Installed,
  /// A running application first, then as [`Self::Installed`]: Detect.
  Running,
}

type Detect = dyn Fn(Tool, &str, Lookup) -> ToolStatus + Send + Sync;

/// Finds the tools: the core's detection in the app, a stand-in in tests.
/// Runs on the background executor.
#[derive(Clone)]
pub struct ToolDetector(Arc<Detect>);

impl ToolDetector {
  pub fn new(detect: impl Fn(Tool, &str, Lookup) -> ToolStatus + Send + Sync + 'static) -> Self {
    Self(Arc::new(detect))
  }

  /// The core's detection, which looks at the file system and, for
  /// [`Lookup::Running`], at the running processes first.
  pub fn system() -> Self {
    Self::new(|tool, path, lookup| match tool {
      Tool::MkvToolNix => {
        let status = bme_core::mkvtoolnix::detect_mkvtoolnix(path, lookup == Lookup::Running);
        ToolStatus {
          found: status.found,
          path: status.mkv_toolnix_path,
        }
      }
      Tool::BetterMediaInfo => {
        let detected = match lookup {
          Lookup::Path => Ok(bme_core::services::find_better_media_info_at(path)),
          Lookup::Installed | Lookup::Running => {
            bme_core::services::detect_better_media_info(path.to_string(), lookup == Lookup::Running)
          }
        };
        match detected {
          Ok(status) => ToolStatus {
            found: status.found,
            path: status.path,
          },
          Err(error) => {
            log::warn!("BetterMediaInfo detection failed: {error}");
            ToolStatus {
              found: false,
              path: String::new(),
            }
          }
        }
      }
    })
  }

  /// Looks for `tool` at `path` (trimmed by the caller) as `lookup` says.
  pub fn detect(&self, tool: Tool, path: &str, lookup: Lookup) -> ToolStatus {
    (self.0)(tool, path, lookup)
  }
}

/// A sink for the MKVToolNix directories track listings and extractions find
/// by themselves (`bme_core::mkvtoolnix::init_auto_detected` in the app) and
/// the receiver [`store_auto_detected`] reads them from.
pub fn auto_detected_channel() -> (AutoDetectedSink, Receiver<AutoDetected>) {
  let (sender, receiver) = async_channel::unbounded();
  let sink: AutoDetectedSink = Box::new(move |path| {
    // Fails only once the receiver is gone, when the app is quitting.
    let _ = sender.try_send(path);
  });
  (sink, receiver)
}

/// Stores every directory from `paths` as the MKVToolNix path through the
/// config store, on the foreground, so the Integration settings show it and no
/// other write drops it; the core never writes the config file itself. A
/// directory found from a path the settings no longer hold (the user chose
/// another meanwhile) is dropped.
pub fn store_auto_detected(config: &Entity<ConfigStore>, paths: Receiver<AutoDetected>, cx: &mut App) {
  let config = config.clone();
  cx.spawn(async move |cx| {
    while let Ok(AutoDetected { configured, detected }) = paths.recv().await {
      config.update(cx, |store, cx| {
        let current = Tool::MkvToolNix.path(store.config());
        if current.trim() == configured.trim() && current != detected {
          store.update(cx, |config| Tool::MkvToolNix.set_path(config, detected));
        }
      });
    }
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::state::Persistence;
  use core::prelude::v1::test;
  use gpui_kit::{AppContext, TestAppContext};
  use std::sync::Mutex;

  #[test]
  fn each_tool_reads_and_writes_its_own_path() {
    let mut config = Config::default();
    Tool::MkvToolNix.set_path(&mut config, "/opt/mkv".into());
    Tool::BetterMediaInfo.set_path(&mut config, "/opt/bmi".into());
    assert_eq!(Tool::MkvToolNix.path(&config), "/opt/mkv");
    assert_eq!(Tool::BetterMediaInfo.path(&config), "/opt/bmi");
    assert_eq!(
      Tool::MkvToolNix.default_path(),
      ConfigIntegration::default().mkv_toolnix_path
    );
    assert!(Tool::BetterMediaInfo.default_path().is_empty());
  }

  #[test]
  fn the_system_detector_finds_nothing_in_an_empty_directory() {
    let dir = std::env::temp_dir().join(format!("bme_tools_empty_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.to_string_lossy().to_string();
    let status = ToolDetector::system().detect(Tool::MkvToolNix, &path, Lookup::Path);
    assert!(!status.found);
    std::fs::remove_dir_all(&dir).unwrap();
  }

  #[gpui_kit::test]
  fn an_auto_detected_directory_is_stored_through_the_config_store(cx: &mut TestAppContext) {
    let written = Arc::new(Mutex::new(Vec::<Config>::new()));
    let recorder = written.clone();
    let persistence = Persistence::new(move |config| recorder.lock().unwrap().push(config));
    let store = cx.new(|cx| ConfigStore::new(Config::default(), persistence, cx));
    let (sink, paths) = auto_detected_channel();
    cx.update(|cx| store_auto_detected(&store, paths, cx));
    let detected = "/Applications/MKVToolNix-98.0.app/Contents/MacOS";
    let found = |configured: &str| AutoDetected {
      configured: configured.to_string(),
      detected: detected.to_string(),
    };
    let default = ConfigIntegration::default().mkv_toolnix_path;
    // A directory found from a path the settings no longer hold is dropped.
    sink(found("/opt/earlier"));
    cx.run_until_parked();
    assert!(written.lock().unwrap().is_empty());
    // Sent on the test thread: GPUI's test scheduler panics when another
    // thread wakes a foreground task.
    sink(found(&default));
    cx.run_until_parked();
    let configured = store.read_with(cx, |store, _| Tool::MkvToolNix.path(store.config()).to_string());
    assert_eq!(configured, detected);
    assert_eq!(
      written
        .lock()
        .unwrap()
        .last()
        .map(|c| Tool::MkvToolNix.path(c).to_string())
        .as_deref(),
      Some(detected)
    );
    // The same directory again changes nothing.
    sink(found(detected));
    cx.run_until_parked();
    assert_eq!(written.lock().unwrap().len(), 1);
  }
}
