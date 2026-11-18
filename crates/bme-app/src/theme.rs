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
//! gpui-kit's defaults. The files are derived from [`THEMES`] by fixed rules:
//! the ring and the link are the main colour, the dark-mode link hover is it
//! lightened by 20 %; a primary button's text is white or 87 % black, as
//! 0.3.0 chose by the contrast ratio 3, and the button's colours are fitted
//! to it so the text reaches 4.5:1 (WCAG's level for normal text) at rest,
//! on hover and while pressed: under white text the main colour darkened as
//! far as needed, then by 30 % on hover; under dark text the main colour
//! (lightened as far as needed), then lightened by 30 % on hover. The test
//! `every_theme_file_is_derived_from_its_main_colour` checks the files and
//! rewrites them when `BME_WRITE_THEMES=1` is set, and
//! `every_button_label_reaches_the_normal_text_contrast` reads the contrast
//! from the files themselves.

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
  /// colour is at least 3, else 87 % black (0.3.0's choice).
  fn contrast_text(color: Rgb) -> &'static str {
    let ratio = (1.0 + 0.05) / (luminance(color) + 0.05);
    if ratio >= 3.0 { "#ffffff" } else { "#000000de" }
  }

  /// The contrast WCAG asks of normal-sized text, as on a button.
  const TEXT_CONTRAST: f64 = 4.5;

  /// The exact WCAG relative luminance (no rounding).
  fn exact_luminance(color: Rgb) -> f64 {
    let [red, green, blue] = color.map(|channel| {
      let value = channel / 255.0;
      if value <= 0.03928 {
        value / 12.92
      } else {
        ((value + 0.055) / 1.055).powf(2.4)
      }
    });
    0.2126 * red + 0.7152 * green + 0.0722 * blue
  }

  /// The contrast ratio of two opaque colours.
  fn contrast(first: Rgb, second: Rgb) -> f64 {
    let (a, b) = (exact_luminance(first), exact_luminance(second));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
  }

  /// Text of `color` (with its alpha, 0-1) drawn over `background`, as the
  /// window composites it.
  fn over(color: Rgb, alpha: f64, background: Rgb) -> Rgb {
    [0, 1, 2].map(|index| color[index] * alpha + background[index] * (1.0 - alpha))
  }

  /// The contrast of `foreground` (`#ffffff` or `#000000de`) on the colour
  /// `background` as a theme file spells it.
  fn text_contrast(foreground: &str, background: Rgb) -> f64 {
    let text = if foreground == "#ffffff" {
      [255.0; 3]
    } else {
      over([0.0; 3], 222.0 / 255.0, background)
    };
    contrast(text, background)
  }

  /// A primary button's colours: rest, hover (and active), and its text.
  /// The text is 0.3.0's choice; the colours are moved as little as needed
  /// for it to reach [`TEXT_CONTRAST`] in every state.
  fn button_colours(main: Rgb) -> (String, String, &'static str) {
    let foreground = contrast_text(main);
    let white = foreground == "#ffffff";
    let shift = |color: Rgb, coefficient: f64| {
      if white {
        darken(color, coefficient)
      } else {
        lighten(color, coefficient)
      }
    };
    // Rounded to a file's colour before it is measured.
    let file_colour = |color: Rgb| channels(u32::from_str_radix(&hex(color)[1..], 16).unwrap());
    let rest = (0..=100)
      .map(|step| file_colour(shift(main, f64::from(step) / 100.0)))
      .find(|rest| text_contrast(foreground, *rest) >= TEXT_CONTRAST)
      .expect("black and white reach any contrast");
    let hover = file_colour(shift(rest, 0.3));
    (hex(rest), hex(hover), foreground)
  }

  /// The theme-set file of a table entry, byte for byte.
  fn derive(spec: &ThemeSpec) -> String {
    let main = channels(spec.main);
    let primary = hex(main);
    let (button, pressed, foreground) = button_colours(main);
    let dark = hex(darken(main, 0.3));
    let light = hex(lighten(main, 0.2));
    let variant = |name: &str, mode: &str, link_hover: &str| {
      format!(
        r#"    {{
      "name": "{name}",
      "mode": "{mode}",
      "font.size": 14,
      "colors": {{
        "primary.background": "{button}",
        "primary.hover.background": "{pressed}",
        "primary.active.background": "{pressed}",
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

  /// Read from the files: a primary button's text reaches 4.5:1 on its
  /// background, its hover and its active colour, in both modes of every
  /// theme, with the text's alpha composited.
  #[test]
  fn every_button_label_reaches_the_normal_text_contrast() {
    for spec in &THEMES {
      let set: serde_json::Value = serde_json::from_str(spec.set).unwrap();
      for variant in set["themes"].as_array().unwrap() {
        let colors = &variant["colors"];
        let foreground = colors["primary.foreground"].as_str().unwrap();
        for state in [
          "primary.background",
          "primary.hover.background",
          "primary.active.background",
        ] {
          let background = colors[state].as_str().unwrap();
          let background = channels(u32::from_str_radix(&background[1..], 16).unwrap());
          let ratio = text_contrast(foreground, background);
          assert!(
            ratio >= TEXT_CONTRAST,
            "{}: {foreground} on {state} {} is {ratio:.2}:1",
            variant["name"],
            hex(background)
          );
        }
      }
    }
  }

  #[test]
  fn button_colours_keep_the_text_and_move_the_colour_as_little_as_needed() {
    let parse = |color: &str| channels(u32::from_str_radix(&color[1..], 16).unwrap());
    // Ocean keeps white text on a slightly deeper blue (3.86:1 before); its
    // hover is darker still.
    let (rest, hover, text) = button_colours(channels(0x0288d1));
    assert_eq!((rest.as_str(), hover.as_str(), text), ("#027cbe", "#015785", "#ffffff"));
    assert!(text_contrast(text, parse(&rest)) >= TEXT_CONTRAST);
    assert!(text_contrast(text, channels(0x0288d1)) < TEXT_CONTRAST);
    assert!(exact_luminance(parse(&hover)) < exact_luminance(parse(&rest)));
    // A colour that passes stays as it is.
    assert_eq!(button_colours(channels(0x1a237e)).0, "#1a237e");
    // Amber keeps dark text; its hover is lighter, not darker, where the
    // darker one gave 4.44:1.
    let (rest, hover, text) = button_colours(channels(0xff8f00));
    assert_eq!((rest.as_str(), text), ("#ff8f00", "#000000de"));
    assert!(exact_luminance(parse(&hover)) > exact_luminance(parse(&rest)));
    assert!(text_contrast(text, parse(&hover)) >= TEXT_CONTRAST);
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
        let main: gpui_kit::Hsla = rgb(spec.main).into();
        let button = button_colours(channels(spec.main)).0;
        let primary: gpui_kit::Hsla = rgb(u32::from_str_radix(&button[1..], 16).unwrap()).into();
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
          assert_eq!(cx.theme().ring, main, "{name}");
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
      let button = button_colours(channels(0x0288d1)).0;
      let primary: gpui_kit::Hsla = rgb(u32::from_str_radix(&button[1..], 16).unwrap()).into();

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
