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

//! The views, one module per screen.
//!
//! Sizes follow one grid: spacing units are 8 px ([`space`]), and the type
//! scale is built on a 12 px body text ([`text`]).

pub mod about;
pub mod actions;
pub mod main_content;
pub mod root;
pub mod status_bar;
pub mod toolbar;
pub mod update_banner;
pub mod welcome;

#[cfg(test)]
mod tests;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::{ElementId, Hsla, Pixels, Rgba, Styled, px};

const SHELL_MARGIN: Pixels = px(2.0);

/// The standard minimum width for a button with a visible text label.
pub(crate) fn text_button(id: impl Into<ElementId>) -> Button {
  Button::new(id).min_w(px(88.0))
}

/// The wider primary action in a confirmation or dialog.
pub(crate) fn confirm_button(id: impl Into<ElementId>) -> Button {
  text_button(id).primary().min_w(px(112.0))
}

/// `n` spacing units (8 px each).
pub fn space(n: f32) -> Pixels {
  px(8.0 * n)
}

/// The type scale: a 14 px-based scale drawn at 12/14 of its size.
pub mod text {
  use gpui_kit::{Pixels, px};

  pub fn h3() -> Pixels {
    px(41.0)
  }

  pub fn h4() -> Pixels {
    px(29.0)
  }

  pub fn h6() -> Pixels {
    px(17.0)
  }

  pub fn body2() -> Pixels {
    px(12.0)
  }

  pub fn caption() -> Pixels {
    px(10.0)
  }
}

/// A CSS `rgba(r, g, b, a)` colour.
pub fn rgba(r: u8, g: u8, b: u8, a: f32) -> Hsla {
  Rgba {
    r: f32::from(r) / 255.0,
    g: f32::from(g) / 255.0,
    b: f32::from(b) / 255.0,
    a,
  }
  .into()
}

/// A CSS `#rrggbb` colour.
pub fn hex(value: u32) -> Hsla {
  gpui_kit::rgb(value).into()
}
