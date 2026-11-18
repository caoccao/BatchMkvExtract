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

//! Application state, split by how often it changes: the configuration, the
//! files, the queue and the UI's own state are one entity each, so a change
//! repaints only the views observing it.

pub mod config;
pub mod files;
pub mod queue;
pub mod ui;

use bme_core::config::Config;
use gpui_kit::{App, AppContext, Entity};

pub use config::{ConfigEvent, ConfigStore, Persistence};
pub use files::{FileStore, TrackSource};
pub use queue::QueueStore;
pub use ui::{TabType, UiState};

use crate::extract_events::ExtractBackend;
use crate::platform::taskbar::Taskbar;
use crate::tools::ToolDetector;

/// What the stores talk to outside the UI: the config file, the track
/// listing, the extract queue, the taskbar progress and the external tools'
/// detection. The app uses the real ones; tests pass stand-ins.
#[derive(Clone)]
pub struct Services {
  pub persistence: Persistence,
  pub tracks: TrackSource,
  pub extracts: ExtractBackend,
  pub taskbar: Taskbar,
  pub tools: ToolDetector,
}

/// Handles to every store; cheap to clone and handed to every view.
#[derive(Clone)]
pub struct Stores {
  pub config: Entity<ConfigStore>,
  pub files: Entity<FileStore>,
  pub queue: Entity<QueueStore>,
  pub ui: Entity<UiState>,
  /// Enqueues and cancels extractions; their events reach `queue` through
  /// [`crate::extract_events::start`].
  pub extracts: ExtractBackend,
  /// The main window's taskbar progress.
  pub taskbar: Taskbar,
  /// Finds MKVToolNix and BetterMediaInfo.
  pub tools: ToolDetector,
}

impl Stores {
  pub fn new(config: Config, services: Services, cx: &mut App) -> Self {
    let Services {
      persistence,
      tracks,
      extracts,
      taskbar,
      tools,
    } = services;
    let config = cx.new(|cx| ConfigStore::new(config, persistence, cx));
    Self {
      files: cx.new(|cx| FileStore::new(&config, tracks, cx)),
      config,
      queue: cx.new(|_| QueueStore::new()),
      ui: cx.new(|_| UiState::new()),
      extracts,
      taskbar,
      tools,
    }
  }

  /// Whether an extraction is waiting or running.
  pub fn has_active_extracts(&self, cx: &App) -> bool {
    self.queue.read(cx).has_active()
  }
}
