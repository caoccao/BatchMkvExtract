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
import type { About, Config, ExtractEntry } from "./protocol";
import { getAbout, getConfig, setConfig } from "./service";

export type TabType = "fileList" | "queue" | "settings" | "about";

export type QueueItemStatus = "queued" | "extracting" | "completed";

export interface QueueItem {
  file: string;
  drive: string;
  status: QueueItemStatus;
  progress: number;
  extractionStartedAt: number | null;
  extractionEndedAt: number | null;
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
  fileExtractHandlers: Record<string, () => void>;
  fileHasSelection: Record<string, boolean>;
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
  applyExtractSnapshot: (entries: ExtractEntry[]) => void;
  addToQueue: (file: string) => void;
  clearCompletedInDrive: (drive: string) => void;
  registerExtractHandler: (file: string, handler: () => void) => void;
  unregisterExtractHandler: (file: string) => void;
  setFileHasSelection: (file: string, hasSelection: boolean) => void;
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
  fileExtractHandlers: {},
  fileHasSelection: {},
  addFiles: (paths) =>
    set((state) => {
      const existing = new Set(state.files);
      const toAdd = paths.filter((p) => !existing.has(p));
      return { files: [...state.files, ...toAdd] };
    }),
  removeFile: (path) =>
    set((state) => ({ files: state.files.filter((f) => f !== path) })),
  clearFiles: () => set({ files: [] }),
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
    if (!current) return;
    const next = { ...current, ...patch };
    set({ config: next });
    try {
      const saved = await setConfig(next);
      set({ config: saved });
    } catch (err) {
      console.error("Failed to save config", err);
    }
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
            entry.status === "extracting" ? now : null,
          extractionEndedAt: null,
        };
        nextOrder.push(entry.file);
      } else {
        let startedAt = existing.extractionStartedAt;
        if (entry.status === "extracting" && startedAt === null) {
          startedAt = now;
        }
        nextItems[entry.file] = {
          ...existing,
          status: entry.status,
          progress: entry.progress,
          extractionStartedAt: startedAt,
        };
      }
    }

    for (const file of Object.keys(nextItems)) {
      const item = nextItems[file];
      if (item.status !== "completed" && !snap.has(file)) {
        nextItems[file] = {
          ...item,
          status: "completed",
          extractionEndedAt: item.extractionEndedAt ?? now,
          progress: 100,
        };
      }
    }

    set({ queueItems: nextItems, queueOrder: nextOrder });
  },
  addToQueue: (file) => {
    const items = get().queueItems;
    if (items[file]) return;
    const order = get().queueOrder;
    const item: QueueItem = {
      file,
      drive: getDriveKey(file),
      status: "queued",
      progress: 0,
      extractionStartedAt: null,
      extractionEndedAt: null,
    };
    set({
      queueItems: { ...items, [file]: item },
      queueOrder: [...order, file],
    });
  },
  clearCompletedInDrive: (drive) =>
    set((state) => {
      const nextItems: Record<string, QueueItem> = { ...state.queueItems };
      const nextOrder: string[] = [];
      for (const file of state.queueOrder) {
        const item = nextItems[file];
        if (!item) continue;
        if (item.drive === drive && item.status === "completed") {
          delete nextItems[file];
        } else {
          nextOrder.push(file);
        }
      }
      return { queueItems: nextItems, queueOrder: nextOrder };
    }),
  registerExtractHandler: (file, handler) =>
    set((state) => ({
      fileExtractHandlers: { ...state.fileExtractHandlers, [file]: handler },
    })),
  unregisterExtractHandler: (file) =>
    set((state) => {
      const next = { ...state.fileExtractHandlers };
      delete next[file];
      return { fileExtractHandlers: next };
    }),
  setFileHasSelection: (file, hasSelection) =>
    set((state) => {
      const current = state.fileHasSelection[file] ?? false;
      if (current === hasSelection) return {};
      const next = { ...state.fileHasSelection };
      if (hasSelection) next[file] = true;
      else delete next[file];
      return { fileHasSelection: next };
    }),
}));
