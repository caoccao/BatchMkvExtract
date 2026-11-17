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

//! The queue's progress on the taskbar button (Windows), the Dock tile (macOS)
//! or the launcher icon (Linux): shown while an extraction waits or runs,
//! hidden otherwise.

use std::rc::Rc;

use gpui_kit::Window;

use crate::state::queue::QueueSummary;

/// Sets the progress of one window: a percentage, or `None` to hide it.
pub type ProgressFn = Box<dyn FnMut(Option<u8>)>;

/// Makes the progress indicator of a window: the platform's in the app, a
/// recorder or nothing in tests.
#[derive(Clone)]
pub struct Taskbar(Rc<dyn Fn(&Window) -> ProgressFn>);

impl Taskbar {
  pub fn new(attach: impl Fn(&Window) -> ProgressFn + 'static) -> Self {
    Self(Rc::new(attach))
  }

  /// `bme-platform`'s indicator.
  pub fn system() -> Self {
    Self::new(|window| {
      let mut progress = bme_platform::TaskbarProgress::new(window, crate::DESKTOP_ID);
      Box::new(move |percent| progress.set(percent))
    })
  }

  /// No indicator.
  pub fn none() -> Self {
    Self::new(|_| Box::new(|_| {}))
  }

  /// The indicator of `window`.
  pub fn attach(&self, window: &Window) -> ProgressFn {
    (self.0)(window)
  }
}

/// What the indicator shows for the queue: the whole queue's progress while
/// an extraction waits or runs, nothing otherwise.
pub fn progress(summary: &QueueSummary) -> Option<u8> {
  (summary.active && summary.total > 0).then(|| summary.percent.min(100) as u8)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_progress_shows_only_while_an_extraction_is_active() {
    assert_eq!(progress(&QueueSummary::default()), None);
    let active = QueueSummary {
      done: 1,
      total: 3,
      active: true,
      percent: 45,
    };
    assert_eq!(progress(&active), Some(45));
    let finished = QueueSummary {
      done: 3,
      total: 3,
      active: false,
      percent: 100,
    };
    assert_eq!(progress(&finished), None);
  }
}
