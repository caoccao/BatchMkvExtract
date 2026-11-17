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

//! Application images, toolbar and card icons, and Lucide icons beyond
//! gpui-kit's default bundle.

use std::borrow::Cow;

use gpui_kit::assets::{Assets, IconName, icon_assets};
use gpui_kit::component::Icon;
use gpui_kit::{AssetSource, Result, SharedString};

/// Asset path of the application icon (512×512 PNG).
pub const APP_ICON: &str = "images/icon.png";
/// Logos of the sibling applications on the Welcome page.
pub const BATCH_MKV_MERGE_ICON: &str = "images/batchmkvmerge.png";
pub const BETTER_MEDIA_INFO_ICON: &str = "images/bettermediainfo.png";
pub const BD_MASTER_ICON: &str = "images/bdmaster.png";

pub const OUTPUT_DIRECTORY_ICON_NAME: &str = "output-directory";
pub const QUIT_ICON_NAME: &str = "log-out";
pub const SETTINGS_ICON_NAME: &str = "settings";

pub fn toolbar_icon(name: &str) -> Icon {
  match name {
    QUIT_ICON_NAME => Icon::new(IconName::LogOut),
    _ => Icon::default().path(format!("icons/toolbar/{name}.svg")),
  }
}

pub fn card_icon(name: &str) -> Icon {
  Icon::default().path(format!("icons/cards/{name}.svg"))
}

const IMAGES: &[(&str, &[u8])] = &[
  (APP_ICON, include_bytes!("../assets/icon.png")),
  (BATCH_MKV_MERGE_ICON, include_bytes!("../assets/batchmkvmerge.png")),
  (BETTER_MEDIA_INFO_ICON, include_bytes!("../assets/bettermediainfo.png")),
  (BD_MASTER_ICON, include_bytes!("../assets/bdmaster.png")),
];

macro_rules! wrapped_icons {
  ($icons:ident, $folder:literal; $($name:literal),+ $(,)?) => {
    const $icons: &[(&str, &[u8])] = &[
      $((
        concat!("icons/", $folder, "/", $name, ".svg"),
        include_bytes!(concat!(env!("OUT_DIR"), "/", $folder, "/", $name, ".svg")),
      )),+
    ];
  };
}

// `extract-all` and `group-mode` were drawn for this app; the others are
// BatchMkvMerge's.
wrapped_icons!(TOOLBAR_ICONS, "toolbar";
  "about",
  "cancel-all",
  "clear-completed",
  "clear-list",
  "extract-all",
  "group-mode",
  "output-directory",
  "profile",
  "resume-all",
  "settings",
  "system-notification",
  "topmost-notification",
);
wrapped_icons!(CARD_ICONS, "cards"; "output-folder", "copy-command", "delete");

// Lucide icons the UI needs that are not part of gpui-kit's default 106-icon
// bundle; every icon is in the `IconName` enum, only the SVGs are opt-in.
icon_assets!(
  ExtraIcons,
  [Blocks, ChevronsRight, Clock, LogOut, RotateCcw, Scissors, SunMoon, X]
);

/// The asset source registered with the application.
pub struct AppAssets;

impl AssetSource for AppAssets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    if let Some((_, bytes)) = IMAGES
      .iter()
      .chain(TOOLBAR_ICONS)
      .chain(CARD_ICONS)
      .find(|(name, _)| *name == path)
    {
      return Ok(Some(Cow::Borrowed(bytes)));
    }
    if let Some(bytes) = ExtraIcons.load(path)? {
      return Ok(Some(bytes));
    }
    Assets.load(path)
  }

  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    let mut names: Vec<SharedString> = IMAGES
      .iter()
      .chain(TOOLBAR_ICONS)
      .chain(CARD_ICONS)
      .filter(|(name, _)| name.starts_with(path))
      .map(|(name, _)| SharedString::new_static(name))
      .collect();
    names.extend(ExtraIcons.list(path)?);
    names.extend(Assets.list(path)?);
    names.sort();
    names.dedup();
    Ok(names)
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use gpui_kit::{DevicePixels, SvgRenderer, SvgSize, size};

  use super::*;

  #[test]
  fn images_and_icons_are_served_over_the_default_bundle() {
    for (path, _) in IMAGES {
      let image = AppAssets.load(path).unwrap().expect("the image");
      assert_eq!(&image[..8], b"\x89PNG\r\n\x1a\n", "{path}");
    }
    let images = AppAssets.list("images/").unwrap();
    assert_eq!(images.len(), IMAGES.len(), "{images:?}");
    // The extra Lucide icons and gpui-kit's own come through as well.
    assert!(AppAssets.load("icons/log-out.svg").unwrap().is_some());
    assert!(AppAssets.load("icons/user.svg").unwrap().is_some());
    assert!(!matches!(AppAssets.load("icons/missing.svg"), Ok(Some(_))));
    let toolbar = AppAssets.list("icons/toolbar/").unwrap();
    assert!(toolbar.contains(&SharedString::new_static("icons/toolbar/extract-all.svg")));
    assert!(toolbar.contains(&SharedString::new_static("icons/toolbar/group-mode.svg")));
  }

  /// Every Lucide icon named in `src/` is served, from gpui-kit's bundle or
  /// the extra icons above; a missing one draws nothing and only logs.
  #[test]
  fn every_icon_the_sources_name_is_bundled() {
    fn sources(directory: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
      for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
          sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
          out.push(path);
        }
      }
    }
    // Lucide's file name of an `IconName` variant: `RotateCcw` is
    // `rotate-ccw`, `Settings2` is `settings-2`.
    fn file_name(variant: &str) -> String {
      let mut name = String::new();
      let mut previous: Option<char> = None;
      for c in variant.chars() {
        let boundary = previous.is_some_and(|previous| {
          (c.is_ascii_uppercase() && (previous.is_ascii_lowercase() || previous.is_ascii_digit()))
            || (c.is_ascii_digit() && previous.is_ascii_alphabetic())
        });
        if boundary {
          name.push('-');
        }
        name.push(c.to_ascii_lowercase());
        previous = Some(c);
      }
      format!("icons/{name}.svg")
    }
    assert_eq!(file_name("RotateCcw"), "icons/rotate-ccw.svg");
    assert_eq!(file_name("Settings2"), "icons/settings-2.svg");
    let mut files = Vec::new();
    sources(
      &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
      &mut files,
    );
    let marker = "IconName::";
    let mut named = std::collections::BTreeSet::new();
    for file in files {
      let text = std::fs::read_to_string(&file).unwrap();
      for (start, _) in text.match_indices(marker) {
        let rest = &text[start + marker.len()..];
        let end = rest.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(rest.len());
        if rest[..end].starts_with(|c: char| c.is_ascii_uppercase()) {
          named.insert(rest[..end].to_string());
        }
      }
    }
    assert!(named.contains("RotateCcw") && named.contains("CircleX"), "{named:?}");
    let missing: Vec<String> = named
      .iter()
      .map(|variant| file_name(variant))
      .filter(|path| !matches!(AppAssets.load(path), Ok(Some(_))))
      .collect();
    assert!(missing.is_empty(), "not bundled: {missing:?}");
  }

  #[test]
  fn generated_icons_rasterize_with_transparent_backgrounds() {
    let renderer = SvgRenderer::new(Arc::new(AppAssets));
    for (icons, logical_size) in [(TOOLBAR_ICONS, 32), (CARD_ICONS, 24)] {
      for (path, _) in icons {
        let svg = AppAssets.load(path).unwrap().unwrap();
        let svg = renderer.parse_svg(&svg).unwrap();
        for scale in 1..=4 {
          let side = DevicePixels(logical_size * scale);
          let dimensions = size(side, side);
          let image = renderer.render_parsed(&svg, SvgSize::ExactSize(dimensions)).unwrap();
          assert_eq!(image.size(0), dimensions, "{path}");
          let pixels = image.as_bytes(0).unwrap().chunks_exact(4);
          assert!(pixels.clone().any(|pixel| pixel[3] == 0), "{path} has no transparency");
          assert!(pixels.clone().any(|pixel| pixel[3] > 128), "{path} is blank");
        }
      }
    }
  }
}
