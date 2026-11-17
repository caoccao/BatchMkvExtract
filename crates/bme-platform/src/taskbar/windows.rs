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

//! Taskbar button progress through `ITaskbarList3` on the window's `HWND`.

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx};
use windows::Win32::UI::Shell::{ITaskbarList3, TBPF_NOPROGRESS, TBPF_NORMAL, TaskbarList};

pub(super) struct Progress {
  hwnd: Option<HWND>,
  taskbar: Option<ITaskbarList3>,
}

impl Progress {
  pub(super) fn new(window: &impl HasWindowHandle, _desktop_id: &str) -> Self {
    let hwnd = match window.window_handle().map(|handle| handle.as_raw()) {
      Ok(RawWindowHandle::Win32(handle)) => Some(HWND(handle.hwnd.get() as *mut core::ffi::c_void)),
      Ok(other) => {
        log::warn!("Taskbar progress needs a Win32 window handle, got {other:?}");
        None
      }
      Err(error) => {
        log::warn!("Taskbar progress has no window handle: {error}");
        None
      }
    };
    Self { hwnd, taskbar: None }
  }

  pub(super) fn set(&mut self, percent: Option<u8>) {
    let Some(hwnd) = self.hwnd else {
      return;
    };
    if self.taskbar.is_none() {
      self.taskbar = create_taskbar_list();
    }
    let Some(taskbar) = &self.taskbar else {
      return;
    };
    // SAFETY: `hwnd` is the live top-level window this indicator was created
    // for, and the COM object was initialised on this thread.
    let result = unsafe {
      match percent {
        Some(percent) => taskbar
          .SetProgressState(hwnd, TBPF_NORMAL)
          .and_then(|_| taskbar.SetProgressValue(hwnd, u64::from(percent), 100)),
        None => taskbar.SetProgressState(hwnd, TBPF_NOPROGRESS),
      }
    };
    if let Err(error) = result {
      log::warn!("Failed to update taskbar progress: {error}");
    }
  }
}

fn create_taskbar_list() -> Option<ITaskbarList3> {
  // SAFETY: plain COM calls. GPUI initialises OLE on its main thread already;
  // a repeated apartment-threaded init then returns S_FALSE and a mismatching
  // one RPC_E_CHANGED_MODE, both of which leave COM usable.
  unsafe {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    match CoCreateInstance::<_, ITaskbarList3>(&TaskbarList, None, CLSCTX_INPROC_SERVER) {
      Ok(taskbar) => match taskbar.HrInit() {
        Ok(()) => Some(taskbar),
        Err(error) => {
          log::warn!("ITaskbarList3::HrInit failed: {error}");
          None
        }
      },
      Err(error) => {
        log::warn!("Failed to create ITaskbarList3: {error}");
        None
      }
    }
  }
}
