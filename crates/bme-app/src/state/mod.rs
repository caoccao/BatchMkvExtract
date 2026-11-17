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

//! Application state, split by how often it changes, so a change repaints
//! only the views observing it. The shell has the configuration and the
//! navigation state; the file list and the queue get their stores with the
//! track loading and the extract events.

pub mod config;
pub mod ui;

use bme_core::config::Config;
use gpui_kit::{App, AppContext, Entity};

pub use config::{ConfigEvent, ConfigStore, Persistence};
pub use ui::{TabType, UiState};

/// What the stores talk to outside the UI: the config file. The app uses the
/// real one; tests pass a recorder.
#[derive(Clone)]
pub struct Services {
  pub persistence: Persistence,
}

/// Handles to every store; cheap to clone and handed to every view.
#[derive(Clone)]
pub struct Stores {
  pub config: Entity<ConfigStore>,
  pub ui: Entity<UiState>,
}

impl Stores {
  pub fn new(config: Config, services: Services, cx: &mut App) -> Self {
    let Services { persistence } = services;
    Self {
      config: cx.new(|cx| ConfigStore::new(config, persistence, cx)),
      ui: cx.new(|_| UiState::new()),
    }
  }

  /// Whether an extraction is waiting or running. Nothing can be queued
  /// before the queue store exists, so nothing is.
  pub fn has_active_extracts(&self, _cx: &App) -> bool {
    false
  }
}
