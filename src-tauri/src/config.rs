/*
 *   Copyright (c) 2024-2026. caoccao.com Sam Cao
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

use anyhow::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

use crate::constants::APP_NAME;

static CONFIG: OnceLock<RwLock<Config>> = OnceLock::new();

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub window: ConfigWindow,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            window: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigWindow {
    #[serde(default)]
    pub position: ConfigWindowPosition,
    #[serde(default)]
    pub size: ConfigWindowSize,
}

impl Default for ConfigWindow {
    fn default() -> Self {
        Self {
            position: Default::default(),
            size: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigWindowPosition {
    pub x: i32,
    pub y: i32,
}

impl Default for ConfigWindowPosition {
    fn default() -> Self {
        Self { x: -1, y: -1 }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ConfigWindowSize {
    pub width: u32,
    pub height: u32,
}

impl Default for ConfigWindowSize {
    fn default() -> Self {
        Self {
            width: 1200,
            height: 900,
        }
    }
}

impl Config {
    fn new() -> Self {
        let config_path_buf = Self::get_path_buf();
        if config_path_buf.exists() {
            Self::load(config_path_buf)
        } else {
            log::debug!("Loading default config.");
            let config = Self::default();
            if let Err(err) = config.save(config_path_buf) {
                log::error!("Couldn't save the default config because {}", err);
            }
            config
        }
    }

    fn get_path_buf() -> PathBuf {
        let config_dir = Self::get_config_dir();
        if !config_dir.exists() {
            if let Err(err) = std::fs::create_dir_all(&config_dir) {
                log::warn!("Couldn't create config dir {}: {}", config_dir.display(), err);
            }
        }
        config_dir.join(format!("{}.json", APP_NAME))
    }

    fn get_exe_dir() -> PathBuf {
        std::env::current_exe().unwrap().parent().unwrap().to_path_buf()
    }

    #[cfg(target_os = "linux")]
    fn get_config_dir() -> PathBuf {
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            if !xdg.is_empty() {
                return PathBuf::from(xdg).join(APP_NAME);
            }
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config").join(APP_NAME);
        }
        Self::get_exe_dir()
    }

    #[cfg(target_os = "macos")]
    fn get_config_dir() -> PathBuf {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_NAME);
        }
        Self::get_exe_dir()
    }

    #[cfg(target_os = "windows")]
    fn get_config_dir() -> PathBuf {
        let exe_dir = Self::get_exe_dir();
        let exe_path_lc = exe_dir.to_string_lossy().to_ascii_lowercase();
        let starts_with_env = |env_var: &str| -> bool {
            std::env::var(env_var)
                .ok()
                .map(|p| !p.is_empty() && exe_path_lc.starts_with(&p.to_ascii_lowercase()))
                .unwrap_or(false)
        };
        let is_installed = starts_with_env("LOCALAPPDATA")
            || starts_with_env("ProgramFiles")
            || starts_with_env("ProgramFiles(x86)");
        if is_installed {
            if let Ok(appdata) = std::env::var("APPDATA") {
                if !appdata.is_empty() {
                    return PathBuf::from(appdata).join(APP_NAME);
                }
            }
        }
        exe_dir
    }

    fn load(path: PathBuf) -> Self {
        let cloned_path = path.clone();
        let path_string = cloned_path.to_str().unwrap();
        log::debug!("Loading config from {}.", path_string);
        let file = File::open(path).expect(format!("Couldn't open config file {}.", path_string).as_str());
        let buf_reader = BufReader::new(file);
        serde_json::from_reader(buf_reader).expect(format!("Couldn't parse config file {}.", path_string).as_str())
    }

    fn save(&self, path: PathBuf) -> Result<()> {
        let cloned_path = path.clone();
        let path_string = cloned_path.to_str().unwrap();
        log::debug!("Saving config to {}.", path_string);
        let file = File::create(path).expect(format!("Couldn't create config file {}.", path_string).as_str());
        let buf_writer = BufWriter::new(file);
        serde_json::to_writer_pretty(buf_writer, &self).map_err(Error::msg)
    }
}

pub fn get_config() -> Config {
    CONFIG
        .get_or_init(|| RwLock::new(Config::new()))
        .read()
        .unwrap()
        .clone()
}

pub fn set_config(config: Config) -> Result<()> {
    let config_path_buf = Config::get_path_buf();
    let result = config.save(config_path_buf);
    CONFIG
        .get_or_init(|| RwLock::new(Config::new()))
        .write()
        .unwrap()
        .clone_from(&config);
    result
}
