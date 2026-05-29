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

import { buildExtractArgs, resolveOutputDir, trackKey } from "../extract-utils";
import type { Config, ConfigProfile, MkvTrack } from "../protocol";
import { QueueItemStatus } from "../protocol";
import { cancelExtract, ensureOutputPath, enqueueExtract } from "../service";
import { useMkvStore } from "../store";

type TranslateFn = (
  key: string,
  options?: Record<string, string | number>,
) => string;

type MkvStoreState = ReturnType<typeof useMkvStore.getState>;

function isActiveStatus(status: QueueItemStatus | undefined): boolean {
  return (
    status === QueueItemStatus.Waiting || status === QueueItemStatus.Extracting
  );
}

export function getActiveProfile(config: Config | null): ConfigProfile | null {
  if (!config) {
    return null;
  }
  return (
    config.profiles.find((profile) => profile.name === config.activeProfile) ??
    config.profiles[0] ??
    null
  );
}

export function getSelectedTracksForFile(
  file: string,
  state: MkvStoreState = useMkvStore.getState(),
): MkvTrack[] {
  const tracks = state.fileTracks[file] ?? [];
  const selectedIds = new Set<string>(state.fileSelectedIds[file] ?? []);
  if (tracks.length === 0 || selectedIds.size === 0) {
    return [];
  }
  return tracks.filter((track) => selectedIds.has(trackKey(track)));
}

export interface EnqueueSelectedTracksOptions {
  file: string;
  selectedTracks: MkvTrack[];
  profile: ConfigProfile;
  t: TranslateFn;
  skipIfActive?: boolean;
}

export async function enqueueSelectedTracksForFile(
  options: EnqueueSelectedTracksOptions,
): Promise<boolean> {
  const {
    file,
    selectedTracks,
    profile,
    t,
    skipIfActive = true,
  } = options;
  if (selectedTracks.length === 0) {
    return false;
  }
  const state = useMkvStore.getState();
  const status = state.queueItems[file]?.status;
  if (skipIfActive && isActiveStatus(status)) {
    return false;
  }
  const outputDir = await resolveOutputDir(
    file,
    state.fileOutputDirs[file],
    state.globalOutputDir,
  );
  try {
    await ensureOutputPath(outputDir);
  } catch {
    state.showNotification(
      "error",
      file,
      t("notification.failedCreateOutput", { path: outputDir }),
    );
    return false;
  }
  const args = await buildExtractArgs(file, outputDir, selectedTracks, profile);
  await enqueueExtract(file, args);
  state.addToQueue(file);
  return true;
}

export async function cancelExtraction(
  file: string,
  onError?: (error: unknown, file: string) => void,
): Promise<void> {
  useMkvStore.getState().markCancelRequested(file);
  try {
    await cancelExtract(file);
  } catch (error) {
    onError?.(error, file);
  }
}

export async function cancelExtractions(
  files: string[],
  onError?: (error: unknown, file: string) => void,
): Promise<void> {
  await Promise.all(files.map((file) => cancelExtraction(file, onError)));
}
