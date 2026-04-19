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

use crate::config;
use crate::mkvtoolnix::is_mkv;
use crate::protocol::About;

pub async fn get_about() -> Result<About> {
    Ok(About {
        app_version: get_app_version().to_owned(),
    })
}

pub async fn get_config() -> Result<config::Config> {
    Ok(config::get_config())
}

pub async fn set_config(config: config::Config) -> Result<config::Config> {
    config::set_config(config)?;
    Ok(config::get_config())
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

pub async fn check_output_path_writable(path: String) -> Result<bool> {
    let mut current = PathBuf::from(&path);
    loop {
        if current.exists() {
            break;
        }
        let Some(parent) = current.parent() else {
            return Ok(false);
        };
        current = parent.to_path_buf();
    }
    if !current.is_dir() {
        return Ok(false);
    }
    let test_name = format!(".batchmkvextract_writecheck_{}", std::process::id());
    let test_path = current.join(&test_name);
    match std::fs::File::create(&test_path) {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_path);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

pub async fn ensure_output_path(path: String) -> Result<()> {
    let p = Path::new(&path);
    if p.exists() {
        if !p.is_dir() {
            anyhow::bail!("{path} exists but is not a directory");
        }
        return Ok(());
    }
    std::fs::create_dir_all(p).map_err(anyhow::Error::msg)?;
    Ok(())
}
