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

//! Dock tile progress: the tile's content view is replaced by the application
//! icon with a bar-style `NSProgressIndicator` along its bottom edge, and
//! restored (content view `nil`) when the progress is hidden.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
  NSApplication, NSImageScaling, NSImageView, NSProgressIndicator, NSProgressIndicatorStyle, NSView,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use raw_window_handle::HasWindowHandle;

/// Height of the bar, as a fraction of the tile height.
const BAR_HEIGHT_RATIO: f64 = 0.14;
/// Inset of the bar from the tile's left, right and bottom edges.
const BAR_INSET_RATIO: f64 = 0.08;

struct TileViews {
  content: Retained<NSImageView>,
  bar: Retained<NSProgressIndicator>,
}

pub(super) struct Progress {
  views: Option<TileViews>,
}

impl Progress {
  pub(super) fn new(_window: &impl HasWindowHandle, _desktop_id: &str) -> Self {
    Self { views: None }
  }

  pub(super) fn set(&mut self, percent: Option<u8>) {
    let Some(mtm) = MainThreadMarker::new() else {
      log::warn!("Dock progress must be set on the main thread");
      return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let tile = app.dockTile();
    match percent {
      Some(percent) => {
        if self.views.is_none() {
          self.views = Some(create_views(&app, tile.size(), mtm));
        }
        if let Some(views) = &self.views {
          views.bar.setDoubleValue(f64::from(percent));
          let content: &NSView = &views.content;
          tile.setContentView(Some(content));
        }
      }
      None => {
        tile.setContentView(None);
      }
    }
    tile.display();
  }
}

fn create_views(app: &NSApplication, size: NSSize, mtm: MainThreadMarker) -> TileViews {
  let frame = NSRect::new(NSPoint::new(0.0, 0.0), size);
  let content = NSImageView::initWithFrame(mtm.alloc(), frame);
  content.setImageScaling(NSImageScaling::ScaleProportionallyUpOrDown);
  if let Some(icon) = app.applicationIconImage() {
    content.setImage(Some(&icon));
  }

  let inset = size.width * BAR_INSET_RATIO;
  let bar_frame = NSRect::new(
    NSPoint::new(inset, inset),
    NSSize::new(size.width - 2.0 * inset, size.height * BAR_HEIGHT_RATIO),
  );
  let bar = NSProgressIndicator::initWithFrame(mtm.alloc(), bar_frame);
  bar.setStyle(NSProgressIndicatorStyle::Bar);
  bar.setIndeterminate(false);
  bar.setMinValue(0.0);
  bar.setMaxValue(100.0);
  content.addSubview(&bar);

  TileViews { content, bar }
}
