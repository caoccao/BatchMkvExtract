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

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct About {
    #[serde(rename = "appVersion")]
    pub app_version: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MkvToolNixStatus {
    pub found: bool,
    #[serde(rename = "mkvToolNixPath")]
    pub mkv_toolnix_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BetterMediaInfoStatus {
    pub found: bool,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MkvTrack {
    pub id: i64,
    pub number: i64,
    #[serde(rename = "type")]
    pub track_type: String,
    pub codec: String,
    #[serde(rename = "codecId")]
    pub codec_id: String,
    #[serde(rename = "trackName")]
    pub track_name: String,
    pub language: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractEntry {
    pub file: String,
    pub status: String,
    pub progress: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtractSnapshot {
    pub entries: Vec<ExtractEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractionFinishedEvent {
    pub file: String,
    pub outcome: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCheckResult {
    #[serde(rename = "hasUpdate")]
    pub has_update: bool,
    #[serde(rename = "latestVersion")]
    pub latest_version: Option<String>,
}

pub struct UpdateCheckState {
    pub result: Arc<Mutex<Option<UpdateCheckResult>>>,
}
