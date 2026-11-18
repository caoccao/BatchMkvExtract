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

//! UI-free core of BatchMkvExtract.
//!
//! The config file, the per-drive extract queue, the `mkvextract` arguments
//! and output file names, MKVToolNix detection, the `mkvmerge -J` track
//! listing and the `mkvextract` launch, file intake and BetterMediaInfo, and
//! the update check. Nothing in this crate depends on a
//! GUI toolkit or an async runtime: long-running work (extractions, update
//! checks) runs on plain threads, and callers that need it off their UI
//! thread move the call there themselves.

pub mod config;
pub mod constants;
pub mod extract;
pub mod extract_args;
pub mod mkvtoolnix;
pub mod protocol;
pub mod services;
mod unique_file;
pub mod update;
