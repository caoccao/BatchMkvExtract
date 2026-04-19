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

import { create } from "zustand";
import { getDriveKey } from "./extract-utils";
import type {
  About,
  Config,
  ConfigProfile,
  ExtractEntry,
  ExtractOutcome,
  MkvTrack,
} from "./protocol";
import {
  DEFAULT_PROFILE_NAME,
  QueueItemStatus,
  createDefaultProfile,
} from "./protocol";
export { QueueItemStatus } from "./protocol";
import { getAbout, getConfig, setConfig } from "./service";

export type TabType = "fileList" | "queue" | "settings" | "about";

export interface TrackCounts {
  video: number;
  audio: number;
  subtitles: number;
}

function isTerminalStatus(status: QueueItemStatus): boolean {
  return (
    status === QueueItemStatus.Completed ||
    status === QueueItemStatus.Cancelled ||
    status === QueueItemStatus.Failed
  );
}

export interface QueueItem {
  file: string;
  drive: string;
  status: QueueItemStatus;
  progress: number;
  extractionStartedAt: number | null;
  extractionEndedAt: number | null;
  cancelRequested: boolean;
  error: string | null;
}

interface MkvStore {
  files: string[];
  activeTab: TabType;
  showSettings: boolean;
  showAbout: boolean;
  about: About | null;
  config: Config | null;
  queueItems: Record<string, QueueItem>;
  queueOrder: string[];
  fileTracks: Record<string, MkvTrack[]>;
  fileTrackCounts: Record<string, TrackCounts>;
  fileSelectedIds: Record<string, number[]>;
  groupByFile: boolean;
  addFiles: (paths: string[]) => void;
  removeFile: (path: string) => void;
  clearFiles: () => void;
  setActiveTab: (type: TabType) => void;
  openSettings: () => void;
  openAbout: () => void;
  closeSettings: () => void;
  closeAbout: () => void;
  initAbout: () => Promise<void>;
  initConfig: () => Promise<void>;
  updateConfig: (patch: Partial<Config>) => Promise<void>;
  updateActiveProfile: (patch: Partial<ConfigProfile>) => Promise<void>;
  addProfile: (name: string) => Promise<void>;
  deleteActiveProfile: () => Promise<void>;
  setActiveProfile: (name: string) => Promise<void>;
  resetActiveProfileTemplates: () => Promise<void>;
  applyExtractSnapshot: (entries: ExtractEntry[]) => void;
  addToQueue: (file: string) => void;
  removeFromQueue: (file: string) => void;
  markCancelRequested: (file: string) => void;
  recordFinishedOutcome: (
    file: string,
    outcome: ExtractOutcome,
    error: string | null,
  ) => void;
  clearCompletedInDrive: (drive: string) => void;
  setFileTracks: (file: string, tracks: MkvTrack[]) => void;
  setFileTrackCounts: (file: string, counts: TrackCounts) => void;
  setFileSelectedIds: (file: string, ids: number[]) => void;
  setGroupSelectedIds: (files: string[], ids: number[]) => void;
  setGroupByFile: (value: boolean) => void;
}

export const useMkvStore = create<MkvStore>((set, get) => ({
  files: [],
  activeTab: "fileList",
  showSettings: false,
  showAbout: false,
  about: null,
  config: null,
  queueItems: {},
  queueOrder: [],
  fileTracks: {},
  fileTrackCounts: {},
  fileSelectedIds: {},
  groupByFile: false,
  addFiles: (paths) =>
    set((state) => {
      const existing = new Set(state.files);
      const toAdd = paths.filter((p) => !existing.has(p));
      return { files: [...state.files, ...toAdd] };
    }),
  removeFile: (path) =>
    set((state) => {
      const nextTracks = { ...state.fileTracks };
      delete nextTracks[path];
      const nextCounts = { ...state.fileTrackCounts };
      delete nextCounts[path];
      const nextSelected = { ...state.fileSelectedIds };
      delete nextSelected[path];
      return {
        files: state.files.filter((f) => f !== path),
        fileTracks: nextTracks,
        fileTrackCounts: nextCounts,
        fileSelectedIds: nextSelected,
      };
    }),
  clearFiles: () =>
    set({
      files: [],
      fileTracks: {},
      fileTrackCounts: {},
      fileSelectedIds: {},
    }),
  setActiveTab: (type) => set({ activeTab: type }),
  openSettings: () => set({ showSettings: true, activeTab: "settings" }),
  openAbout: () => set({ showAbout: true, activeTab: "about" }),
  closeSettings: () =>
    set((state) => ({
      showSettings: false,
      activeTab: state.activeTab === "settings" ? "fileList" : state.activeTab,
    })),
  closeAbout: () =>
    set((state) => ({
      showAbout: false,
      activeTab: state.activeTab === "about" ? "fileList" : state.activeTab,
    })),
  initAbout: async () => {
    try {
      const about = await getAbout();
      set({ about });
    } catch (err) {
      console.error("Failed to load about info", err);
    }
  },
  initConfig: async () => {
    try {
      const config = await getConfig();
      set({ config });
    } catch (err) {
      console.error("Failed to load config", err);
    }
  },
  updateConfig: async (patch) => {
    const current = get().config;
    if (!current) {
      return;
    }
    const next = { ...current, ...patch };
    set({ config: next });
    try {
      await setConfig(next);
    } catch (err) {
      console.error("Failed to save config", err);
    }
  },
  updateActiveProfile: async (patch) => {
    const current = get().config;
    if (!current) {
      return;
    }
    const profiles = current.profiles.map((p) =>
      p.name === current.activeProfile ? { ...p, ...patch } : p,
    );
    await get().updateConfig({ profiles });
  },
  addProfile: async (name) => {
    const trimmed = name.trim();
    if (!trimmed) {
      return;
    }
    const current = get().config;
    if (!current) {
      return;
    }
    if (current.profiles.some((p) => p.name === trimmed)) {
      return;
    }
    const fresh = createDefaultProfile(trimmed);
    await get().updateConfig({
      profiles: [...current.profiles, fresh],
      activeProfile: trimmed,
    });
  },
  deleteActiveProfile: async () => {
    const current = get().config;
    if (!current) {
      return;
    }
    if (current.activeProfile === DEFAULT_PROFILE_NAME) {
      return;
    }
    const profiles = current.profiles.filter(
      (p) => p.name !== current.activeProfile,
    );
    await get().updateConfig({
      profiles,
      activeProfile: DEFAULT_PROFILE_NAME,
    });
  },
  setActiveProfile: async (name) => {
    const current = get().config;
    if (!current) {
      return;
    }
    if (!current.profiles.some((p) => p.name === name)) {
      return;
    }
    await get().updateConfig({ activeProfile: name });
  },
  resetActiveProfileTemplates: async () => {
    const current = get().config;
    if (!current) {
      return;
    }
    const fresh = createDefaultProfile(current.activeProfile);
    const profiles = current.profiles.map((p) =>
      p.name === current.activeProfile ? fresh : p,
    );
    await get().updateConfig({ profiles });
  },
  applyExtractSnapshot: (entries) => {
    const now = Date.now();
    const snap = new Map(entries.map((e) => [e.file, e]));
    const prev = get().queueItems;
    const prevOrder = get().queueOrder;
    const nextItems: Record<string, QueueItem> = { ...prev };
    const nextOrder = [...prevOrder];

    for (const entry of entries) {
      const existing = nextItems[entry.file];
      if (!existing) {
        nextItems[entry.file] = {
          file: entry.file,
          drive: getDriveKey(entry.file),
          status: entry.status,
          progress: entry.progress,
          extractionStartedAt:
            entry.status === QueueItemStatus.Extracting ? now : null,
          extractionEndedAt: null,
          cancelRequested: false,
          error: null,
        };
        nextOrder.push(entry.file);
      } else {
        const wasTerminal = isTerminalStatus(existing.status);
        let startedAt = wasTerminal ? null : existing.extractionStartedAt;
        const endedAt = wasTerminal ? null : existing.extractionEndedAt;
        const cancelRequested = wasTerminal ? false : existing.cancelRequested;
        const error = wasTerminal ? null : existing.error;
        if (entry.status === QueueItemStatus.Extracting && startedAt === null) {
          startedAt = now;
        }
        nextItems[entry.file] = {
          ...existing,
          status: entry.status,
          progress: entry.progress,
          extractionStartedAt: startedAt,
          extractionEndedAt: endedAt,
          cancelRequested,
          error,
        };
      }
    }

    for (const file of Object.keys(nextItems)) {
      const item = nextItems[file];
      if (!isTerminalStatus(item.status) && !snap.has(file)) {
        const fallback = item.cancelRequested
          ? QueueItemStatus.Cancelled
          : QueueItemStatus.Completed;
        nextItems[file] = {
          ...item,
          status: fallback,
          extractionEndedAt: item.extractionEndedAt ?? now,
          progress:
            fallback === QueueItemStatus.Completed ? 100 : item.progress,
        };
      }
    }

    set({ queueItems: nextItems, queueOrder: nextOrder });
  },
  addToQueue: (file) => {
    const items = get().queueItems;
    const existing = items[file];
    if (
      existing &&
      (existing.status === QueueItemStatus.Waiting ||
        existing.status === QueueItemStatus.Extracting)
    ) {
      return;
    }
    const fresh: QueueItem = {
      file,
      drive: getDriveKey(file),
      status: QueueItemStatus.Waiting,
      progress: 0,
      extractionStartedAt: null,
      extractionEndedAt: null,
      cancelRequested: false,
      error: null,
    };
    if (existing) {
      set({ queueItems: { ...items, [file]: fresh } });
    } else {
      set({
        queueItems: { ...items, [file]: fresh },
        queueOrder: [...get().queueOrder, file],
      });
    }
  },
  removeFromQueue: (file) =>
    set((state) => {
      if (!state.queueItems[file]) {
        return {};
      }
      const nextItems = { ...state.queueItems };
      delete nextItems[file];
      return {
        queueItems: nextItems,
        queueOrder: state.queueOrder.filter((f) => f !== file),
      };
    }),
  markCancelRequested: (file) =>
    set((state) => {
      const existing = state.queueItems[file];
      if (!existing) {
        return {};
      }
      if (isTerminalStatus(existing.status)) {
        return {};
      }
      return {
        queueItems: {
          ...state.queueItems,
          [file]: { ...existing, cancelRequested: true },
        },
      };
    }),
  recordFinishedOutcome: (file, outcome, error) =>
    set((state) => {
      const existing = state.queueItems[file];
      const now = Date.now();
      if (!existing) {
        return {
          queueItems: {
            ...state.queueItems,
            [file]: {
              file,
              drive: getDriveKey(file),
              status: outcome,
              progress: outcome === QueueItemStatus.Completed ? 100 : 0,
              extractionStartedAt: null,
              extractionEndedAt: now,
              cancelRequested: false,
              error,
            },
          },
          queueOrder: [...state.queueOrder, file],
        };
      }
      return {
        queueItems: {
          ...state.queueItems,
          [file]: {
            ...existing,
            status: outcome,
            extractionEndedAt: existing.extractionEndedAt ?? now,
            progress:
              outcome === QueueItemStatus.Completed ? 100 : existing.progress,
            error,
          },
        },
      };
    }),
  clearCompletedInDrive: (drive) =>
    set((state) => {
      const nextItems: Record<string, QueueItem> = { ...state.queueItems };
      const nextOrder: string[] = [];
      for (const file of state.queueOrder) {
        const item = nextItems[file];
        if (!item) {
          continue;
        }
        if (item.drive === drive && isTerminalStatus(item.status)) {
          delete nextItems[file];
        } else {
          nextOrder.push(file);
        }
      }
      return { queueItems: nextItems, queueOrder: nextOrder };
    }),
  setFileTracks: (file, tracks) =>
    set((state) => ({
      fileTracks: { ...state.fileTracks, [file]: tracks },
    })),
  setFileTrackCounts: (file, counts) =>
    set((state) => ({
      fileTrackCounts: { ...state.fileTrackCounts, [file]: counts },
    })),
  setFileSelectedIds: (file, ids) =>
    set((state) => ({
      fileSelectedIds: { ...state.fileSelectedIds, [file]: ids },
    })),
  setGroupSelectedIds: (files, ids) =>
    set((state) => {
      const next = { ...state.fileSelectedIds };
      for (const f of files) {
        next[f] = ids;
      }
      return { fileSelectedIds: next };
    }),
  setGroupByFile: (value) => set({ groupByFile: value }),
}));
