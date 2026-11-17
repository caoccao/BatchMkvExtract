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

//! Per-OS integrations of BatchMkvExtract that GPUI does not provide.
//!
//! The crate has no GPUI dependency: callers pass anything that implements
//! [`raw_window_handle::HasWindowHandle`], which GPUI's `Window` does.

pub mod display;
pub mod taskbar;

pub use display::window_display_id;
pub use taskbar::TaskbarProgress;
