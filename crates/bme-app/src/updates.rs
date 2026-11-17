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

//! The startup update check: once the configured interval has passed, GitHub
//! is asked on the background executor; otherwise the version found by the
//! last check is reused. The result goes to [`crate::state::UiState`] and the
//! time and version of a check into the config store, so a later settings edit
//! never writes an older `update` block over them.

use std::sync::Arc;

use anyhow::Result;
use bme_core::protocol::UpdateCheckResult;
use bme_core::update::{self, StartupUpdateCheck};
use gpui_kit::App;

use crate::state::Stores;

/// The GitHub releases page the banner links to.
pub const RELEASES_URL: &str = "https://github.com/caoccao/BatchMkvExtract/releases";

type Check = dyn Fn(&str) -> Result<UpdateCheckResult> + Send + Sync;

/// Asks for the newest release: GitHub in the app, a stand-in in tests.
#[derive(Clone)]
pub struct UpdateSource(Arc<Check>);

impl UpdateSource {
  pub fn new(check: impl Fn(&str) -> Result<UpdateCheckResult> + Send + Sync + 'static) -> Self {
    Self(Arc::new(check))
  }

  /// `bme_core::update::check_for_updates`, a blocking network call.
  pub fn github() -> Self {
    Self::new(update::check_for_updates)
  }

  /// Runs the check; a panic counts as a failed check, as in `run_check`.
  fn check(&self, version: &str) -> Result<UpdateCheckResult> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (self.0)(version)))
      .unwrap_or_else(|_| Err(anyhow::anyhow!("Update check panicked")))
  }
}

/// The version to announce, if any.
fn announced(result: &UpdateCheckResult) -> Option<String> {
  result.latest_version.clone().filter(|_| result.has_update)
}

/// Decides at startup whether to ask `source` and shows the outcome. A failed
/// check is logged and leaves the config alone, so the next start tries
/// again.
pub fn start(stores: &Stores, source: UpdateSource, cx: &mut App) {
  let config = stores.config.read(cx).config().clone();
  match update::startup_check(&config, update::current_timestamp(), crate::APP_VERSION) {
    StartupUpdateCheck::Cached(result) => {
      stores
        .ui
        .update(cx, |ui, cx| ui.set_update_available(announced(&result), cx));
    }
    StartupUpdateCheck::CheckNow => {
      let stores = stores.clone();
      cx.spawn(async move |cx| {
        let checked = cx
          .background_executor()
          .spawn(async move { source.check(crate::APP_VERSION) })
          .await;
        match checked {
          Ok(result) => {
            stores.config.update(cx, |config, cx| {
              let mut shown = None;
              config.update(cx, |config| {
                shown = announced(&update::record_check(
                  &mut config.update,
                  result,
                  update::current_timestamp(),
                ));
              });
              stores.ui.update(cx, |ui, cx| ui.set_update_available(shown, cx));
            });
          }
          Err(error) => log::warn!("Update check failed: {error}"),
        }
      })
      .detach();
    }
  }
}

/// Closes the banner; with `skip`, the version is not announced again.
pub fn dismiss(stores: &Stores, version: &str, skip: bool, cx: &mut App) {
  if skip {
    let version = version.to_string();
    stores.config.update(cx, |config, cx| {
      config.update(cx, |config| config.update.ignore_version = version)
    });
  }
  stores.ui.update(cx, |ui, cx| ui.set_update_available(None, cx));
}
