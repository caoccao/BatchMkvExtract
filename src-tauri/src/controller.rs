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

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::protocol::About;

const MKV_EXTENSION: &str = "mkv";

pub async fn get_about() -> Result<About> {
    Ok(About {
        app_version: get_app_version().to_owned(),
    })
}

pub fn get_app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub async fn get_mkv_files(paths: Vec<String>) -> Result<Vec<String>> {
    let mut result: Vec<String> = Vec::new();
    for input in paths {
        let path = Path::new(input.as_str());
        if !path.exists() {
            continue;
        }
        if path.is_dir() {
            let mut entries: Vec<PathBuf> = path
                .read_dir()
                .map_err(anyhow::Error::msg)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.is_file() && is_mkv(p))
                .collect();
            entries.sort();
            for p in entries {
                if let Some(s) = p.to_str() {
                    result.push(s.to_owned());
                }
            }
        } else if path.is_file() && is_mkv(path) {
            result.push(input);
        }
    }
    Ok(result)
}

fn is_mkv(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case(MKV_EXTENSION))
        .unwrap_or(false)
}
