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

//! Progress shown on the application's taskbar button (Windows), Dock tile
//! (macOS) or launcher icon (Linux desktops that implement
//! `com.canonical.Unity.LauncherEntry`: KDE Plasma, GNOME with Dash to Dock,
//! Unity).

use raw_window_handle::HasWindowHandle;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod unsupported;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "linux")]
use linux as imp;
#[cfg(target_os = "macos")]
use macos as imp;
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
use unsupported as imp;
#[cfg(target_os = "windows")]
use windows as imp;

/// Progress indicator of one application window.
///
/// Must be created and used on the thread that owns the window (GPUI's main
/// thread). Failures are logged and never surface to the caller: the progress
/// bar is decoration, a missing one must not break an extraction.
pub struct TaskbarProgress {
  inner: imp::Progress,
  last: Option<Option<u8>>,
}

impl TaskbarProgress {
  /// Creates the indicator for `window`.
  ///
  /// `desktop_id` is the basename of the installed `.desktop` file without the
  /// extension (Linux only: the launcher matches the progress to the icon by
  /// it). macOS shows the progress on the Dock tile, which belongs to the
  /// application rather than to the window.
  pub fn new(window: &impl HasWindowHandle, desktop_id: &str) -> Self {
    Self {
      inner: imp::Progress::new(window, desktop_id),
      last: None,
    }
  }

  /// Shows the bar at `percent` (clamped to 100), or hides it with `None`.
  /// Repeating the current value is a no-op.
  pub fn set(&mut self, percent: Option<u8>) {
    let percent = percent.map(|p| p.min(100));
    if self.last == Some(percent) {
      return;
    }
    self.last = Some(percent);
    self.inner.set(percent);
  }
}

impl Drop for TaskbarProgress {
  fn drop(&mut self) {
    if matches!(self.last, Some(Some(_))) {
      self.inner.set(None);
    }
  }
}
