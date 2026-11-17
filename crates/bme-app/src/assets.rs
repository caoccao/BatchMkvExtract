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

//! Application assets: the app's own images, composed over gpui-kit's
//! default icon bundle.

use std::borrow::Cow;

use gpui_kit::assets::Assets;
use gpui_kit::{AssetSource, Result, SharedString};

/// Asset path of the application icon (512×512 PNG).
pub const APP_ICON: &str = "images/icon.png";

const IMAGES: &[(&str, &[u8])] = &[(APP_ICON, include_bytes!("../assets/icon.png"))];

/// The asset source registered with the application.
pub struct AppAssets;

impl AssetSource for AppAssets {
  fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
    if let Some((_, bytes)) = IMAGES.iter().find(|(name, _)| *name == path) {
      return Ok(Some(Cow::Borrowed(bytes)));
    }
    Assets.load(path)
  }

  fn list(&self, path: &str) -> Result<Vec<SharedString>> {
    let mut names: Vec<SharedString> = IMAGES
      .iter()
      .filter(|(name, _)| name.starts_with(path))
      .map(|(name, _)| SharedString::new_static(name))
      .collect();
    names.extend(Assets.list(path)?);
    names.sort();
    names.dedup();
    Ok(names)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_app_icon_is_served_over_the_default_bundle() {
    let icon = AppAssets.load(APP_ICON).unwrap().expect("the icon");
    assert_eq!(&icon[..8], b"\x89PNG\r\n\x1a\n");
    assert!(
      AppAssets
        .list("images/")
        .unwrap()
        .contains(&SharedString::new_static(APP_ICON))
    );
    assert!(!AppAssets.list("").unwrap().is_empty());
  }
}
