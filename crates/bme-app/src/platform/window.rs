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

//! The main window: initial geometry from the config, geometry persistence
//! and the close request.
//!
//! Positions are logical pixels; sizes are the content size. GPUI's macOS
//! window reports its frame (title bar included) but opens with a content
//! size, so the saved size comes from the viewport, not the window bounds.
//!
//! On Windows positions are global: a display left of or above the primary
//! one has negative coordinates, and GPUI opens a window at the given bounds
//! only when their centre lies on the display it is opened for (the primary
//! one unless `display_id` names another); otherwise it falls back to a
//! default size centred on that display. GPUI divides each display's desktop
//! coordinates by that display's scale, so displays at different scales
//! overlap: a 100 % display of 1920 px and a 200 % one right of it share the
//! logical x 960–1920. On macOS every display reports its bounds at the
//! origin and a window its position relative to its screen. Either way the
//! position alone cannot always tell the displays apart, so the geometry is
//! saved with the display's UUID, and the window reopens on the display it
//! names; only an older config, or a display that is gone, falls back to the
//! display under the position. A window whose centre or origin is on a
//! display reopens where it was, partly off the screen too; one whose centre
//! is off its display moves just far enough for the centre to be on it, as
//! GPUI otherwise drops the saved bounds.

use bme_core::config::{ConfigWindow, ConfigWindowPosition, ConfigWindowSize};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, DisplayId, Pixels, Point, Size, TitlebarOptions, Window, WindowBounds,
  WindowOptions, point, px, size,
};

use super::close_guard;
use crate::state::Stores;
use crate::ui::root::AppView;

/// The smallest window, and the smallest geometry that is saved.
pub const MIN_WINDOW_SIZE: (u32, u32) = (600, 450);

/// The saved size, raised to the minimum (a hand-edited config may hold less).
pub fn sanitize_size(size: &ConfigWindowSize) -> (u32, u32) {
  (size.width.max(MIN_WINDOW_SIZE.0), size.height.max(MIN_WINDOW_SIZE.1))
}

/// Whether a geometry of this size is worth saving.
pub fn is_persistable_size(width: u32, height: u32) -> bool {
  width >= MIN_WINDOW_SIZE.0 && height >= MIN_WINDOW_SIZE.1
}

/// The saved window's bounds, its size raised to the minimum; `None` when it
/// was never saved (the default `-1, -1` position). A position may be
/// negative on a display left of or above the primary one.
pub fn saved_bounds(saved: &ConfigWindow) -> Option<Bounds<Pixels>> {
  let (width, height) = sanitize_size(&saved.size);
  let position = &saved.position;
  let never_saved = ConfigWindowPosition::default();
  if position.x == never_saved.x && position.y == never_saved.y {
    return None;
  }
  let origin = point(px(position.x as f32), px(position.y as f32));
  Some(Bounds::new(origin, size(px(width as f32), px(height as f32))))
}

/// The display a window in `bounds` opens on, as an index into `displays`:
/// the one under its centre, else the one under its origin; the `primary`
/// display wins when it qualifies, so a window that fits it opens there as
/// it would without a display.
pub fn placement_display(
  bounds: &Bounds<Pixels>,
  displays: &[Bounds<Pixels>],
  primary: Option<usize>,
) -> Option<usize> {
  let under = |at: Point<Pixels>| {
    primary
      .filter(|&index| displays.get(index).is_some_and(|display| display.contains(&at)))
      .or_else(|| displays.iter().position(|display| display.contains(&at)))
  };
  under(bounds.center()).or_else(|| under(bounds.origin))
}

/// `bounds` moved as little as needed for its centre to lie on `display`, at
/// least 1 px inside. GPUI's Windows backend keeps the bounds a window opens
/// with only when their centre is on the display it opens for, and otherwise
/// opens it centred at a default size.
pub fn with_centre_on(bounds: Bounds<Pixels>, display: &Bounds<Pixels>) -> Bounds<Pixels> {
  let clamp = |value: f32, low: f32, high: f32| value.min(high - 1.0).max(low + 1.0);
  let centre = bounds.center();
  let (x, y) = (f32::from(centre.x), f32::from(centre.y));
  let dx = clamp(x, f32::from(display.left()), f32::from(display.right())) - x;
  let dy = clamp(y, f32::from(display.top()), f32::from(display.bottom())) - y;
  if dx == 0.0 && dy == 0.0 {
    return bounds;
  }
  let origin = point(bounds.origin.x + px(dx), bounds.origin.y + px(dy));
  Bounds::new(origin, bounds.size)
}

/// Where the saved window reopens: its bounds and the display it opens on,
/// as an index into `displays` (each with its UUID, if the platform names
/// one). The display the config names wins when it is connected and holds
/// the window's centre or its origin, so a window that straddles two
/// displays or hangs off one keeps its own; else [`placement_display`]. A
/// window whose centre is off that display moves just onto it
/// ([`with_centre_on`]), keeping its size. `None` centres the window: it was
/// never saved, or neither its centre nor its origin is on a connected
/// display.
pub fn initial_placement(
  saved: &ConfigWindow,
  displays: &[(Bounds<Pixels>, Option<String>)],
  primary: Option<usize>,
) -> Option<(Bounds<Pixels>, usize)> {
  let bounds = saved_bounds(saved)?;
  let display_bounds: Vec<Bounds<Pixels>> = displays.iter().map(|(bounds, _)| *bounds).collect();
  let named = displays.iter().position(|(display, uuid)| {
    !saved.display.is_empty()
      && uuid.as_deref() == Some(saved.display.as_str())
      && (display.contains(&bounds.center()) || display.contains(&bounds.origin))
  });
  let display = named.or_else(|| placement_display(&bounds, &display_bounds, primary))?;
  Some((with_centre_on(bounds, &display_bounds[display]), display))
}

/// The geometry to save for a window in `bounds` showing `content` on
/// `display` (its UUID, or empty): only a normal (not maximized, not
/// fullscreen) window of at least the minimum size is saved, so the restore
/// geometry survives maximizing.
pub fn geometry_to_save(bounds: WindowBounds, content: Size<Pixels>, display: String) -> Option<ConfigWindow> {
  let WindowBounds::Windowed(bounds) = bounds else {
    return None;
  };
  let width = f32::from(content.width).round().max(0.0) as u32;
  let height = f32::from(content.height).round().max(0.0) as u32;
  if !is_persistable_size(width, height) {
    return None;
  }
  Some(ConfigWindow {
    display,
    position: ConfigWindowPosition {
      x: f32::from(bounds.origin.x).round() as i32,
      y: f32::from(bounds.origin.y).round() as i32,
    },
    size: ConfigWindowSize { width, height },
  })
}

fn same_geometry(a: &ConfigWindow, b: &ConfigWindow) -> bool {
  a.display == b.display
    && a.position.x == b.position.x
    && a.position.y == b.position.y
    && a.size.width == b.size.width
    && a.size.height == b.size.height
}

/// Options of the main window: saved geometry on the display it was saved
/// on, or centred on the primary display; the minimum size, the title, the
/// desktop id as the Wayland app id.
pub fn main_window_options(saved: &ConfigWindow, cx: &App) -> WindowOptions {
  let displays = cx.displays();
  let described: Vec<(Bounds<Pixels>, Option<String>)> = displays
    .iter()
    .map(|display| (display.bounds(), display.uuid().ok().map(|uuid| uuid.to_string())))
    .collect();
  let primary = cx
    .primary_display()
    .and_then(|primary| displays.iter().position(|display| display.id() == primary.id()));
  let placement = initial_placement(saved, &described, primary);
  let display_id = placement.map(|(_, index)| displays[index].id());
  let bounds = placement.map(|(bounds, _)| bounds).unwrap_or_else(|| {
    let (width, height) = sanitize_size(&saved.size);
    Bounds::centered(None, size(px(width as f32), px(height as f32)), cx)
  });
  WindowOptions {
    window_bounds: Some(WindowBounds::Windowed(bounds)),
    display_id,
    titlebar: Some(TitlebarOptions {
      title: Some(crate::window_title()),
      ..Default::default()
    }),
    window_min_size: Some(size(px(MIN_WINDOW_SIZE.0 as f32), px(MIN_WINDOW_SIZE.1 as f32))),
    app_id: Some(crate::DESKTOP_ID.into()),
    ..Default::default()
  }
}

/// Opens the main window with the application shell; `None` if it failed.
pub fn open_main(stores: &Stores, cx: &mut App) -> Option<AnyWindowHandle> {
  let options = main_window_options(&stores.config.read(cx).config().window, cx);
  let stores = stores.clone();
  match gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| AppView::new(stores, window, cx))) {
    Ok((handle, _)) => Some(handle),
    Err(error) => {
      log::error!("Failed to open the main window: {error}");
      None
    }
  }
}

/// The UUID of the display `window` is on; empty when the platform names
/// none. On Windows the OS says which display that is, as GPUI's own can lag
/// behind at mixed scales (`bme_platform::window_display_id`).
fn window_display(window: &Window, cx: &App) -> String {
  let display = match bme_platform::window_display_id(window) {
    Some(id) => cx
      .displays()
      .into_iter()
      .find(|display| display.id() == DisplayId::from(id)),
    None => window.display(cx),
  };
  display
    .and_then(|display| display.uuid().ok())
    .map(|uuid| uuid.to_string())
    .unwrap_or_default()
}

/// Saves the window's geometry if it is persistable and changed.
pub fn save_geometry(stores: &Stores, window: &Window, cx: &mut App) {
  let display = window_display(window, cx);
  let Some(geometry) = geometry_to_save(window.window_bounds(), window.viewport_size(), display) else {
    return;
  };
  if same_geometry(&stores.config.read(cx).config().window, &geometry) {
    return;
  }
  stores
    .config
    .update(cx, |config, cx| config.update(cx, |config| config.window = geometry));
}

/// A request to close the main window (title bar, Alt+F4, Quit): saves the
/// geometry, then lets the close guard decide; its Yes stops every active
/// extraction, without the completion notifications their end would send.
/// `true` lets the window close.
pub fn close_requested(stores: &Stores, window: &mut Window, cx: &mut App) -> bool {
  save_geometry(stores, window, cx);
  let closing = stores.ui.clone();
  close_guard::should_close(stores.has_active_extracts(cx), window, cx, move |_, cx| {
    closing.update(cx, |ui, _| ui.set_closing());
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn saved(x: i32, y: i32, width: u32, height: u32) -> ConfigWindow {
    ConfigWindow {
      display: String::new(),
      position: ConfigWindowPosition { x, y },
      size: ConfigWindowSize { width, height },
    }
  }

  fn display(x: f32, y: f32, width: f32, height: f32) -> Bounds<Pixels> {
    Bounds::new(point(px(x), px(y)), size(px(width), px(height)))
  }

  /// Displays without UUIDs, as on Wayland.
  fn anonymous(displays: &[Bounds<Pixels>]) -> Vec<(Bounds<Pixels>, Option<String>)> {
    displays.iter().map(|bounds| (*bounds, None)).collect()
  }

  #[test]
  fn the_default_config_opens_centred_at_its_size() {
    let config = ConfigWindow::default();
    assert_eq!(saved_bounds(&config), None);
    let displays = anonymous(&[display(0.0, 0.0, 1920.0, 1080.0)]);
    assert_eq!(initial_placement(&config, &displays, Some(0)), None);
    assert_eq!(sanitize_size(&config.size), (1200, 900));
  }

  #[test]
  fn a_saved_position_on_a_display_is_restored() {
    let displays = anonymous(&[display(0.0, 0.0, 1920.0, 1080.0), display(1920.0, 0.0, 1280.0, 1024.0)]);
    let (bounds, index) = initial_placement(&saved(2000, 100, 800, 600), &displays, Some(0)).expect("restored");
    assert_eq!(bounds.origin, point(px(2000.0), px(100.0)));
    assert_eq!(bounds.size, size(px(800.0), px(600.0)));
    assert_eq!(index, 1);
    // Too small a saved size is raised to the minimum.
    let bounds = saved_bounds(&saved(10, 10, 100, 100)).expect("saved");
    assert_eq!(bounds.size, size(px(600.0), px(450.0)));
  }

  #[test]
  fn a_window_partly_on_a_display_is_restored_and_one_off_every_display_centred() {
    let displays = anonymous(&[display(0.0, 0.0, 1920.0, 1080.0)]);
    assert_eq!(initial_placement(&saved(5000, 100, 800, 600), &displays, Some(0)), None);
    // Hanging off the left edge: the origin is off the display, the centre on it.
    let (bounds, index) = initial_placement(&saved(-100, 100, 800, 600), &displays, Some(0)).expect("restored");
    assert_eq!((bounds.origin, index), (point(px(-100.0), px(100.0)), 0));
  }

  #[test]
  fn a_position_on_a_display_left_of_the_primary_one_is_restored() {
    let displays = anonymous(&[
      display(0.0, 0.0, 1920.0, 1080.0),
      display(-1280.0, -200.0, 1280.0, 1024.0),
    ]);
    let (bounds, index) = initial_placement(&saved(-1000, -100, 800, 600), &displays, Some(0)).expect("restored");
    assert_eq!((bounds.origin, index), (point(px(-1000.0), px(-100.0)), 1));
    // The default position still means "never saved", even with a display there.
    assert_eq!(initial_placement(&ConfigWindow::default(), &displays, Some(0)), None);
  }

  #[test]
  fn the_window_opens_on_the_display_under_its_centre() {
    let displays = [display(0.0, 0.0, 1920.0, 1080.0), display(1920.0, 0.0, 2560.0, 1440.0)];
    let on_second = display(2000.0, 100.0, 800.0, 600.0);
    assert_eq!(placement_display(&on_second, &displays, Some(0)), Some(1));
    // Mostly on the second display: its centre decides, not its origin.
    let straddling = display(1500.0, 100.0, 1200.0, 600.0);
    assert_eq!(placement_display(&straddling, &displays, Some(0)), Some(1));
    // Centre off every display: the display under the origin.
    let hanging_off = display(4000.0, 100.0, 1200.0, 600.0);
    assert_eq!(placement_display(&hanging_off, &displays, Some(0)), Some(1));
    assert_eq!(
      placement_display(&display(9000.0, 0.0, 800.0, 600.0), &displays, Some(0)),
      None
    );
  }

  #[test]
  fn the_primary_display_wins_where_displays_overlap() {
    // macOS reports every display at the origin.
    let displays = [display(0.0, 0.0, 2560.0, 1440.0), display(0.0, 0.0, 1920.0, 1080.0)];
    let fits_both = display(100.0, 100.0, 800.0, 600.0);
    assert_eq!(placement_display(&fits_both, &displays, Some(1)), Some(1));
    assert_eq!(placement_display(&fits_both, &displays, None), Some(0));
    // Only the larger display holds it.
    let fits_larger = display(1800.0, 1000.0, 800.0, 300.0);
    assert_eq!(placement_display(&fits_larger, &displays, Some(1)), Some(0));
  }

  /// A 1920 × 1080 primary display at 100 % and a 3840 × 2160 display right
  /// of it at 200 %, as GPUI reports them on Windows: they overlap at the
  /// logical x 960–1920.
  fn mixed_scales() -> [(Bounds<Pixels>, Option<String>); 2] {
    [
      (display(0.0, 0.0, 1920.0, 1080.0), Some("primary".to_string())),
      (display(960.0, 0.0, 1920.0, 1080.0), Some("scaled".to_string())),
    ]
  }

  fn on(display: &str, x: i32, y: i32) -> ConfigWindow {
    ConfigWindow {
      display: display.to_string(),
      ..saved(x, y, 800, 600)
    }
  }

  #[test]
  fn the_window_reopens_on_the_display_it_was_saved_on() {
    let displays = mixed_scales();
    // Saved at the physical x 2100 of the scaled display: the logical 1050,
    // whose centre lies on both displays.
    let (bounds, display) = initial_placement(&on("scaled", 1050, 100), &displays, Some(0)).expect("restored");
    assert_eq!(bounds.origin, point(px(1050.0), px(100.0)));
    assert_eq!(display, 1);
    let placed = |saved: &ConfigWindow| initial_placement(saved, &displays, Some(0)).map(|(_, display)| display);
    assert_eq!(placed(&on("primary", 1050, 100)), Some(0));
    // An older config, or a display that is gone: the display under the position.
    assert_eq!(placed(&on("", 1050, 100)), Some(0));
    assert_eq!(placed(&on("unplugged", 1050, 100)), Some(0));
    // Straddling both: the origin on the primary display only, the centre on
    // both. Windows put it on the scaled display, which holds the centre.
    assert_eq!(placed(&on("scaled", 900, 100)), Some(1));
    // Partly above both: the origin on no display, the centre on both.
    assert_eq!(placed(&on("scaled", 1500, -100)), Some(1));
    assert_eq!(placed(&on("", 1500, -100)), Some(0));
    // The named display no longer holds the window.
    assert_eq!(placed(&on("scaled", 100, 100)), Some(0));
    // Never saved, or off every display: centred.
    assert_eq!(placed(&ConfigWindow::default()), None);
    assert_eq!(placed(&on("scaled", 9000, 100)), None);
  }

  #[test]
  fn a_window_with_its_centre_off_its_display_moves_onto_it() {
    let displays = mixed_scales();
    // Only the origin is on the scaled display (960–2880): the centre is at
    // 3100, off every display.
    let (bounds, index) = initial_placement(&on("scaled", 2700, 100), &displays, Some(0)).expect("restored");
    assert_eq!(index, 1);
    assert_eq!(bounds.origin, point(px(2479.0), px(100.0)));
    assert_eq!(bounds.size, size(px(800.0), px(600.0)));
    assert!(displays[1].0.contains(&bounds.center()));
    // Below the primary display, found by its origin.
    let (bounds, index) = initial_placement(&on("", 300, 900), &displays, Some(0)).expect("restored");
    assert_eq!((bounds.origin, index), (point(px(300.0), px(779.0)), 0));
    assert!(displays[0].0.contains(&bounds.center()));
    // A window whose centre is on its display stays where it was.
    let inside = display(100.0, 100.0, 800.0, 600.0);
    assert_eq!(with_centre_on(inside, &displays[0].0), inside);
  }

  #[test]
  fn the_display_is_found_where_every_display_starts_at_the_origin() {
    // macOS: a window on the second display reports its position on it.
    let displays = [
      (display(0.0, 0.0, 2560.0, 1440.0), Some("built-in".to_string())),
      (display(0.0, 0.0, 1920.0, 1080.0), Some("external".to_string())),
    ];
    let on_external = on("external", 100, 100);
    assert_eq!(initial_placement(&on_external, &displays, Some(0)).unwrap().1, 1);
    // Without UUIDs (Wayland): the primary display.
    let unnamed = anonymous(&displays.map(|(bounds, _)| bounds));
    assert_eq!(initial_placement(&on_external, &unnamed, Some(0)).unwrap().1, 0);
  }

  #[test]
  fn only_a_normal_window_of_the_minimum_size_is_saved() {
    let bounds = display(10.4, 20.6, 1000.0, 830.0);
    let content = size(px(1000.0), px(802.0));
    let windowed = |content| geometry_to_save(WindowBounds::Windowed(bounds), content, "scaled".to_string());
    let geometry = windowed(content).expect("saved");
    let mut expected = saved(10, 21, 1000, 802);
    assert!(!same_geometry(&geometry, &expected), "on another display");
    expected.display = "scaled".to_string();
    assert!(same_geometry(&geometry, &expected));
    assert!(geometry_to_save(WindowBounds::Maximized(bounds), content, String::new()).is_none());
    assert!(geometry_to_save(WindowBounds::Fullscreen(bounds), content, String::new()).is_none());
    assert!(windowed(size(px(599.0), px(800.0))).is_none());
    assert!(windowed(size(px(800.0), px(449.0))).is_none());
  }
}
