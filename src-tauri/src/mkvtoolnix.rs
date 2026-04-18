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
#[cfg(target_os = "macos")]
use std::cmp::Ordering;
#[cfg(target_os = "macos")]
use std::fs;
use std::path::{Path, PathBuf};

use crate::config;
use crate::protocol::MkvextractStatus;

struct MkvToolNixResolution {
    path: PathBuf,
    auto_detected: bool,
}

fn has_tool(path: &Path, tool: &str) -> bool {
    let tool_path = path.join(tool);
    if tool_path.exists() && tool_path.is_file() {
        return true;
    }
    #[cfg(target_os = "windows")]
    {
        let tool_exe_path = path.join(format!("{}.exe", tool));
        if tool_exe_path.exists() && tool_exe_path.is_file() {
            return true;
        }
    }
    false
}

#[cfg(target_os = "macos")]
fn compare_version_parts(left: &[u32], right: &[u32]) -> Ordering {
    let len = left.len().max(right.len());
    for i in 0..len {
        let l = left.get(i).copied().unwrap_or(0);
        let r = right.get(i).copied().unwrap_or(0);
        match l.cmp(&r) {
            Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    Ordering::Equal
}

#[cfg(target_os = "macos")]
fn parse_version_parts(version: &str) -> Vec<u32> {
    version
        .split('.')
        .filter_map(|part| {
            let digits: String = part.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                None
            } else {
                digits.parse::<u32>().ok()
            }
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn is_default_macos_mkvtoolnix_path(path: &str) -> bool {
    path.trim().trim_end_matches('/') == "/Applications/MKVToolNix.app/Contents/MacOS"
}

#[cfg(target_os = "macos")]
fn find_latest_versioned_macos_mkvtoolnix_path(tool: &str) -> Option<PathBuf> {
    let entries = fs::read_dir("/Applications").ok()?;
    let mut latest: Option<(Vec<u32>, PathBuf)> = None;
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let app_name = match file_name.to_str() {
            Some(value) => value,
            None => continue,
        };
        if !app_name.starts_with("MKVToolNix-") || !app_name.ends_with(".app") {
            continue;
        }
        let version = &app_name["MKVToolNix-".len()..app_name.len() - ".app".len()];
        let version_parts = parse_version_parts(version);
        if version_parts.is_empty() {
            continue;
        }
        let mkvtoolnix_path = entry.path().join("Contents").join("MacOS");
        if !has_tool(&mkvtoolnix_path, tool) {
            continue;
        }
        match &latest {
            None => latest = Some((version_parts, mkvtoolnix_path)),
            Some((latest_version, _)) => {
                if compare_version_parts(&version_parts, latest_version) == Ordering::Greater {
                    latest = Some((version_parts, mkvtoolnix_path));
                }
            }
        }
    }
    latest.map(|(_, path)| path)
}

fn resolve_mkvtoolnix(path: &str, tool: &str) -> MkvToolNixResolution {
    let trimmed_path = path.trim();
    let configured_path = PathBuf::from(trimmed_path);
    if has_tool(&configured_path, tool) {
        return MkvToolNixResolution {
            path: configured_path,
            auto_detected: false,
        };
    }
    #[cfg(target_os = "macos")]
    {
        if is_default_macos_mkvtoolnix_path(trimmed_path) {
            if let Some(latest_path) = find_latest_versioned_macos_mkvtoolnix_path(tool) {
                return MkvToolNixResolution {
                    path: latest_path,
                    auto_detected: true,
                };
            }
        }
    }
    MkvToolNixResolution {
        path: configured_path,
        auto_detected: false,
    }
}

fn persist_mkvtoolnix_path_if_auto_detected(resolution: &MkvToolNixResolution) -> Result<()> {
    if !resolution.auto_detected {
        return Ok(());
    }
    let path = resolution.path.to_string_lossy().to_string();
    let mut cfg = config::get_config();
    if cfg.mkv.mkv_toolnix_path == path {
        return Ok(());
    }
    cfg.mkv.mkv_toolnix_path = path;
    config::set_config(cfg)?;
    Ok(())
}

pub async fn is_mkvextract_found(path: String) -> Result<MkvextractStatus> {
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return Ok(MkvextractStatus {
            found: false,
            mkv_toolnix_path: String::new(),
        });
    }
    let resolution = resolve_mkvtoolnix(trimmed_path, "mkvextract");
    let found = has_tool(&resolution.path, "mkvextract");
    if found {
        persist_mkvtoolnix_path_if_auto_detected(&resolution)?;
    }
    Ok(MkvextractStatus {
        found,
        mkv_toolnix_path: resolution.path.to_string_lossy().to_string(),
    })
}
