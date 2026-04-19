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

import { invoke } from "@tauri-apps/api/core";
import type {
  About,
  Config,
  ExtractSnapshot,
  MkvTrack,
  MkvextractStatus,
} from "./protocol";

export async function getAbout(): Promise<About> {
  return await invoke<About>("get_about");
}

export async function getConfig(): Promise<Config> {
  return await invoke<Config>("get_config");
}

export async function setConfig(config: Config): Promise<Config> {
  return await invoke<Config>("set_config", { config });
}

export async function getMkvFiles(paths: string[]): Promise<string[]> {
  return await invoke<string[]>("get_mkv_files", { paths });
}

export async function getLaunchArgs(): Promise<string[]> {
  return await invoke<string[]>("get_launch_args");
}

export async function isMkvextractFound(
  path: string,
): Promise<MkvextractStatus> {
  return await invoke<MkvextractStatus>("is_mkvextract_found", { path });
}

export async function getMkvTracks(file: string): Promise<MkvTrack[]> {
  return await invoke<MkvTrack[]>("get_mkv_tracks", { file });
}

export async function enqueueExtract(
  file: string,
  args: string[],
): Promise<void> {
  return await invoke<void>("enqueue_extract", { file, args });
}

export async function cancelExtract(file: string): Promise<void> {
  return await invoke<void>("cancel_extract", { file });
}

export async function getExtractStatus(): Promise<ExtractSnapshot> {
  return await invoke<ExtractSnapshot>("get_extract_status");
}

export async function checkOutputPathWritable(path: string): Promise<boolean> {
  return await invoke<boolean>("check_output_path_writable", { path });
}

export async function ensureOutputPath(path: string): Promise<void> {
  return await invoke<void>("ensure_output_path", { path });
}
