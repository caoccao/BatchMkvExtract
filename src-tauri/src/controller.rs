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
use crate::protocol::{About, BetterMediaInfoStatus};

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

fn better_media_info_exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "BetterMediaInfo.exe"
    } else {
        "BetterMediaInfo"
    }
}

pub fn find_running_process_dir(exe_name: &str) -> Option<PathBuf> {
    let sys = sysinfo::System::new_all();
    for process in sys.processes().values() {
        let name = process.name().to_string_lossy();
        if !name.eq_ignore_ascii_case(exe_name) {
            continue;
        }
        if let Some(exe) = process.exe() {
            if let Some(parent) = exe.parent() {
                return Some(parent.to_path_buf());
            }
        }
    }
    None
}

fn find_running_better_media_info_dir() -> Option<PathBuf> {
    find_running_process_dir(better_media_info_exe_name())
}

fn find_better_media_info_dir(path: &Path) -> Option<PathBuf> {
    if !path.exists() {
        return None;
    }
    let exe_name = better_media_info_exe_name();
    if path.is_file() {
        let matches = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case(exe_name))
            .unwrap_or(false);
        if matches {
            return path.parent().map(|p| p.to_path_buf());
        }
        return None;
    }
    if path.is_dir() && path.join(exe_name).is_file() {
        return Some(path.to_path_buf());
    }
    None
}

fn common_better_media_info_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for env_var in ["LOCALAPPDATA"] {
            if let Ok(value) = std::env::var(env_var) {
                if !value.is_empty() {
                    dirs.push(PathBuf::from(value).join("Programs").join("BetterMediaInfo"));
                }
            }
        }
        for env_var in ["ProgramFiles", "ProgramFiles(x86)"] {
            if let Ok(value) = std::env::var(env_var) {
                if !value.is_empty() {
                    dirs.push(PathBuf::from(value).join("BetterMediaInfo"));
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        dirs.push(PathBuf::from(
            "/Applications/BetterMediaInfo.app/Contents/MacOS",
        ));
    }
    #[cfg(target_os = "linux")]
    {
        dirs.push(PathBuf::from("/usr/bin"));
        dirs.push(PathBuf::from("/usr/local/bin"));
    }
    dirs
}

pub async fn launch_better_media_info(paths: Vec<String>) -> Result<()> {
    let cfg = config::get_config();
    let configured = cfg.external_tools.better_media_info_path.trim().to_owned();
    if configured.is_empty() {
        anyhow::bail!("BetterMediaInfo path is not set");
    }
    let exe = Path::new(&configured).join(better_media_info_exe_name());
    if !exe.is_file() {
        anyhow::bail!(
            "BetterMediaInfo executable not found at {}",
            exe.display()
        );
    }
    let mut cmd = std::process::Command::new(&exe);
    cmd.args(&paths);
    cmd.spawn()
        .map_err(|e| anyhow::anyhow!("Failed to launch BetterMediaInfo: {}", e))?;
    Ok(())
}

pub async fn detect_better_media_info(
    user_path: String,
    check_running: bool,
) -> Result<BetterMediaInfoStatus> {
    if check_running {
        if let Some(dir) = find_running_better_media_info_dir() {
            return Ok(BetterMediaInfoStatus {
                found: true,
                path: dir.to_string_lossy().to_string(),
            });
        }
    }
    let trimmed = user_path.trim();
    if !trimmed.is_empty() {
        if let Some(found) = find_better_media_info_dir(Path::new(trimmed)) {
            return Ok(BetterMediaInfoStatus {
                found: true,
                path: found.to_string_lossy().to_string(),
            });
        }
    }
    for dir in common_better_media_info_dirs() {
        if let Some(found) = find_better_media_info_dir(&dir) {
            return Ok(BetterMediaInfoStatus {
                found: true,
                path: found.to_string_lossy().to_string(),
            });
        }
    }
    Ok(BetterMediaInfoStatus {
        found: false,
        path: String::new(),
    })
}
