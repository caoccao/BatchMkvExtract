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

//! Colour themes and light/dark mode.
//!
//! Each of the twenty configured themes is a gpui-kit theme set under
//! `themes/` with a light and a dark variant. They set the primary, ring and
//! link tokens from the theme's main colour and keep every other token at
//! gpui-kit's defaults. The files are derived from [`THEMES`] by fixed rules
//! (hover and active = the main colour darkened by 30 %, the dark-mode link
//! hover lightened; text on the main colour white or 87 % black by contrast
//! ratio); the test `every_theme_file_is_derived_from_its_main_colour` checks
//! them and rewrites them when `BME_WRITE_THEMES=1` is set.

use bme_core::config::{Config, DisplayMode, Theme as ThemeName};
use gpui_kit::component::{Theme, ThemeMode, ThemeRegistry, scroll::ScrollbarMode};
use gpui_kit::{App, Hsla, WindowAppearance};

/// One configured theme: its registry names, its main colour and its embedded
/// theme set.
pub struct ThemeSpec {
  pub theme: ThemeName,
  /// The theme set's name and file stem: `Ocean`, `themes/ocean.json`.
  pub name: &'static str,
  pub light: &'static str,
  pub dark: &'static str,
  /// The main (primary) colour, as `0xRRGGBB`.
  pub main: u32,
  set: &'static str,
}

macro_rules! theme {
  ($theme:ident, $file:literal, $main:literal) => {
    ThemeSpec {
      theme: ThemeName::$theme,
      name: stringify!($theme),
      light: concat!(stringify!($theme), " Light"),
      dark: concat!(stringify!($theme), " Dark"),
      main: $main,
      set: include_str!(concat!("../themes/", $file, ".json")),
    }
  };
}

/// The twenty themes in the config's order, with their main colours.
pub const THEMES: [ThemeSpec; 20] = [
  theme!(Ocean, "ocean", 0x0288d1),
  theme!(Aqua, "aqua", 0x00acc1),
  theme!(Sky, "sky", 0x42a5f5),
  theme!(Arctic, "arctic", 0x4fc3f7),
  theme!(Glacier, "glacier", 0x5c6bc0),
  theme!(Mist, "mist", 0x90a4ae),
  theme!(Slate, "slate", 0x546e7a),
  theme!(Charcoal, "charcoal", 0x37474f),
  theme!(Midnight, "midnight", 0x1a237e),
  theme!(Indigo, "indigo", 0x3f51b5),
  theme!(Violet, "violet", 0x7e57c2),
  theme!(Lavender, "lavender", 0x9575cd),
  theme!(Rose, "rose", 0xc2185b),
  theme!(Blush, "blush", 0xec407a),
  theme!(Coral, "coral", 0xff7043),
  theme!(Sunset, "sunset", 0xef6c00),
  theme!(Amber, "amber", 0xff8f00),
  theme!(Sand, "sand", 0xbcaaa4),
  theme!(Forest, "forest", 0x2e7d32),
  theme!(Emerald, "emerald", 0x00897b),
];

/// The table entry of a configured theme.
pub fn spec(theme: ThemeName) -> &'static ThemeSpec {
  THEMES
    .iter()
    .find(|spec| spec.theme == theme)
    .expect("every configured theme has a table entry")
}

/// Installs the app's scrollbar policy and registers its embedded theme sets.
/// Call once after `gpui_kit::init`.
pub fn install(cx: &mut App) {
  Theme::set_scrollbar_mode(ScrollbarMode::Scrolling, cx);
  let registry = ThemeRegistry::global_mut(cx);
  for spec in &THEMES {
    if let Err(error) = registry.load_themes_from_str(spec.set) {
      log::error!("Failed to load the {} theme set: {error}", spec.name);
    }
  }
}

/// Registry names of the light and dark variant of a configured theme.
pub fn theme_names(theme: ThemeName) -> (&'static str, &'static str) {
  let spec = spec(theme);
  (spec.light, spec.dark)
}

/// The mode a display mode shows under the given system appearance; Auto
/// follows the system, like `prefers-color-scheme`.
pub fn resolve_mode(display_mode: DisplayMode, appearance: WindowAppearance) -> ThemeMode {
  match display_mode {
    DisplayMode::Light => ThemeMode::Light,
    DisplayMode::Dark => ThemeMode::Dark,
    DisplayMode::Auto => match appearance {
      WindowAppearance::Dark | WindowAppearance::VibrantDark => ThemeMode::Dark,
      WindowAppearance::Light | WindowAppearance::VibrantLight => ThemeMode::Light,
    },
  }
}

/// Tint selections with the active theme's color at the input selection opacity.
pub(crate) fn selection_highlight(theme: &Theme) -> Hsla {
  let color = if theme.is_dark() {
    theme.link_hover
  } else {
    theme.primary
  };
  color.opacity(theme.selection.a)
}

/// Applies the configured theme and mode; `appearance` is the system's (the
/// window's once one exists). Refreshes every window.
pub fn apply(config: &Config, appearance: WindowAppearance, cx: &mut App) {
  let (light, dark) = theme_names(config.theme);
  let registry = ThemeRegistry::global(cx);
  let light = registry.themes().get(light).cloned();
  let dark = registry.themes().get(dark).cloned();
  match (light, dark) {
    (Some(light), Some(dark)) => {
      let theme = Theme::global_mut(cx);
      theme.light_theme = light;
      theme.dark_theme = dark;
    }
    _ => log::error!("Theme {:?} is not registered", config.theme),
  }
  Theme::change(resolve_mode(config.display_mode, appearance), None, cx);
}

#[cfg(test)]
mod tests {
  use super::*;
  use core::prelude::v1::test;
  use gpui_kit::component::ActiveTheme;
  use gpui_kit::{TestAppContext, px, rgb};

  /// An sRGB colour as three 0-255 channels.
  type Rgb = [f64; 3];

  fn channels(color: u32) -> Rgb {
    [
      f64::from((color >> 16) & 0xff),
      f64::from((color >> 8) & 0xff),
      f64::from(color & 0xff),
    ]
  }

  fn hex(color: Rgb) -> String {
    let [red, green, blue] = color.map(|channel| channel.round().clamp(0.0, 255.0) as u8);
    format!("#{red:02x}{green:02x}{blue:02x}")
  }

  /// Darkens: every channel times `1 - coefficient`.
  fn darken(color: Rgb, coefficient: f64) -> Rgb {
    color.map(|channel| channel * (1.0 - coefficient))
  }

  /// Lightens: every channel moved towards white by `coefficient`.
  fn lighten(color: Rgb, coefficient: f64) -> Rgb {
    color.map(|channel| channel + (255.0 - channel) * coefficient)
  }

  /// The WCAG relative luminance, rounded to three decimals.
  fn luminance(color: Rgb) -> f64 {
    let [red, green, blue] = color.map(|channel| {
      let value = channel / 255.0;
      if value <= 0.03928 {
        value / 12.92
      } else {
        ((value + 0.055) / 1.055).powf(2.4)
      }
    });
    ((0.2126 * red + 0.7152 * green + 0.0722 * blue) * 1000.0).round() / 1000.0
  }

  /// The text colour on `color`: white when its contrast ratio against the
  /// colour is at least 3, else 87 % black.
  fn contrast_text(color: Rgb) -> &'static str {
    let ratio = (1.0 + 0.05) / (luminance(color) + 0.05);
    if ratio >= 3.0 { "#ffffff" } else { "#000000de" }
  }

  /// The theme-set file of a table entry, byte for byte.
  fn derive(spec: &ThemeSpec) -> String {
    let main = channels(spec.main);
    let primary = hex(main);
    let dark = hex(darken(main, 0.3));
    let light = hex(lighten(main, 0.2));
    let foreground = contrast_text(main);
    let variant = |name: &str, mode: &str, link_hover: &str| {
      format!(
        r#"    {{
      "name": "{name}",
      "mode": "{mode}",
      "font.size": 14,
      "colors": {{
        "primary.background": "{primary}",
        "primary.hover.background": "{dark}",
        "primary.active.background": "{dark}",
        "primary.foreground": "{foreground}",
        "ring": "{primary}",
        "link": "{primary}",
        "link.hover": "{link_hover}",
        "link.active": "{link_hover}"
      }}
    }}"#
      )
    };
    format!(
      r#"{{
  "name": "{}",
  "author": "Sam Cao",
  "url": "https://github.com/caoccao/BatchMkvExtract",
  "themes": [
{},
{}
  ]
}}
"#,
      spec.name,
      variant(spec.light, "light", &dark),
      variant(spec.dark, "dark", &light),
    )
  }

  fn config(theme: ThemeName, display_mode: DisplayMode) -> Config {
    Config {
      theme,
      display_mode,
      ..Config::default()
    }
  }

  #[test]
  fn colour_rules() {
    let ocean = channels(0x0288d1);
    assert_eq!(hex(ocean), "#0288d1");
    // Ocean's hover colour (darkened) and dark-mode link hover (lightened).
    assert_eq!(hex(darken(ocean, 0.3)), "#015f92");
    assert_eq!(hex(lighten(ocean, 0.2)), "#35a0da");
    assert_eq!(contrast_text(ocean), "#ffffff");
    // Light main colours get dark text.
    for color in [0x00acc1, 0x42a5f5, 0x4fc3f7, 0x90a4ae, 0xff7043, 0xff8f00, 0xbcaaa4] {
      assert_eq!(contrast_text(channels(color)), "#000000de", "{color:06x}");
    }
    assert_eq!(contrast_text(channels(0xef6c00)), "#ffffff");
    assert_eq!(luminance(channels(0xffffff)), 1.0);
  }

  /// Checks every file under `themes/` against its derivation; with
  /// `BME_WRITE_THEMES=1` writes the derived files instead.
  #[test]
  fn every_theme_file_is_derived_from_its_main_colour() {
    let write = std::env::var("BME_WRITE_THEMES").is_ok_and(|value| value == "1");
    let directory = format!("{}/themes", env!("CARGO_MANIFEST_DIR"));
    let mut files: Vec<String> = std::fs::read_dir(&directory)
      .unwrap()
      .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
      .collect();
    files.sort();
    let mut expected: Vec<String> = THEMES
      .iter()
      .map(|spec| format!("{}.json", spec.name.to_lowercase()))
      .collect();
    expected.sort();
    assert_eq!(files, expected, "one file per theme, nothing else");
    for spec in &THEMES {
      let derived = derive(spec);
      let path = format!("{directory}/{}.json", spec.name.to_lowercase());
      if write {
        std::fs::write(&path, &derived).unwrap();
      } else {
        // The embedded set is the file on disk, which Git checks out with
        // CRLF line endings where `core.autocrlf` is set (the Windows runner).
        assert_eq!(
          spec.set.replace("\r\n", "\n"),
          derived,
          "{path} differs; run with BME_WRITE_THEMES=1"
        );
      }
    }
  }

  #[test]
  fn the_table_names_every_config_theme_once() {
    let mut names = std::collections::HashSet::new();
    for entry in &THEMES {
      // The config spells a theme as its name.
      assert_eq!(
        serde_json::to_string(&entry.theme).unwrap(),
        format!("\"{}\"", entry.name)
      );
      assert_eq!(spec(entry.theme).name, entry.name);
      assert!(names.insert(entry.name), "{} twice", entry.name);
    }
    assert_eq!(spec(ThemeName::default()).name, "Ocean");
  }

  #[test]
  fn auto_follows_the_system_appearance() {
    assert_eq!(resolve_mode(DisplayMode::Auto, WindowAppearance::Dark), ThemeMode::Dark);
    assert_eq!(
      resolve_mode(DisplayMode::Auto, WindowAppearance::VibrantDark),
      ThemeMode::Dark
    );
    assert_eq!(
      resolve_mode(DisplayMode::Auto, WindowAppearance::Light),
      ThemeMode::Light
    );
    assert_eq!(
      resolve_mode(DisplayMode::Light, WindowAppearance::Dark),
      ThemeMode::Light
    );
    assert_eq!(
      resolve_mode(DisplayMode::Dark, WindowAppearance::Light),
      ThemeMode::Dark
    );
  }

  #[gpui_kit::test]
  fn every_theme_registers_a_light_and_a_dark_variant(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      install(cx);
      for spec in &THEMES {
        let primary: gpui_kit::Hsla = rgb(spec.main).into();
        for (display_mode, name, dark) in [
          (DisplayMode::Light, spec.light, false),
          (DisplayMode::Dark, spec.dark, true),
        ] {
          let registered = ThemeRegistry::global(cx).themes().get(name).cloned();
          let registered = registered.unwrap_or_else(|| panic!("{name} is not registered"));
          assert_eq!(registered.mode.is_dark(), dark, "{name}");
          apply(&config(spec.theme, display_mode), WindowAppearance::Light, cx);
          assert_eq!(cx.theme().theme_name().as_ref(), name);
          assert_eq!(cx.theme().is_dark(), dark, "{name}");
          assert_eq!(cx.theme().primary, primary, "{name}");
          assert_eq!(cx.theme().ring, primary, "{name}");
          assert_eq!(cx.theme().font_size, px(14.0), "{name}");
        }
      }
    });
  }

  #[gpui_kit::test]
  fn ocean_is_applied_in_both_modes(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      install(cx);
      let primary: gpui_kit::Hsla = rgb(0x0288d1).into();

      apply(
        &config(ThemeName::Ocean, DisplayMode::Light),
        WindowAppearance::Dark,
        cx,
      );
      assert!(!cx.theme().is_dark());
      assert_eq!(cx.theme().primary, primary);
      assert_eq!(cx.theme().primary_foreground, rgb(0xffffff).into());
      let light_background = cx.theme().background;

      apply(&config(ThemeName::Ocean, DisplayMode::Auto), WindowAppearance::Dark, cx);
      assert!(cx.theme().is_dark());
      assert_eq!(cx.theme().primary, primary);
      // Tokens the theme does not set keep gpui-kit's per-mode defaults.
      assert_ne!(cx.theme().background, light_background);
    });
  }

  #[gpui_kit::test]
  fn light_main_colours_get_dark_button_text(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      install(cx);
      apply(
        &config(ThemeName::Amber, DisplayMode::Dark),
        WindowAppearance::Light,
        cx,
      );
      let foreground = cx.theme().primary_foreground;
      assert_eq!(foreground.a, 222.0 / 255.0);
      assert_eq!(foreground.l, 0.0);
    });
  }
}
