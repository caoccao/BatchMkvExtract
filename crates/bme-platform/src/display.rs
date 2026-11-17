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

//! The display a window is on, as the OS sees it.
//!
//! GPUI's Windows backend notices a window changing displays only when its
//! logical centre leaves the old display's logical bounds, and logical
//! coordinates of displays at different scales overlap: a window moved onto a
//! 200 % display right of a 100 % one keeps the first display for a while.
//! Windows itself assigns a window to the monitor it overlaps most, the one
//! whose scale the window takes.

use raw_window_handle::HasWindowHandle;

/// The raw id of the display `window` is on, the value GPUI's `DisplayId`
/// wraps (the `HMONITOR`); `None` where GPUI's own display is right.
#[cfg(target_os = "windows")]
pub fn window_display_id(window: &impl HasWindowHandle) -> Option<u64> {
  use raw_window_handle::RawWindowHandle;
  use windows::Win32::Foundation::HWND;
  use windows::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromWindow};

  let RawWindowHandle::Win32(handle) = window.window_handle().ok()?.as_raw() else {
    return None;
  };
  let hwnd = HWND(handle.hwnd.get() as *mut core::ffi::c_void);
  // SAFETY: `hwnd` is the live window GPUI handed out; MonitorFromWindow only
  // reads its position.
  let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) };
  (!monitor.is_invalid()).then_some(monitor.0 as u64)
}

/// The raw id of the display `window` is on; `None` where GPUI's own display
/// is right.
#[cfg(not(target_os = "windows"))]
pub fn window_display_id(_window: &impl HasWindowHandle) -> Option<u64> {
  None
}
