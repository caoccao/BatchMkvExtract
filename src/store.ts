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
import type { About, Config } from "./protocol";
import { getAbout, getConfig, setConfig } from "./service";

export type TabType = "fileList" | "settings" | "about";

interface MkvStore {
  files: string[];
  activeTab: TabType;
  showSettings: boolean;
  showAbout: boolean;
  about: About | null;
  config: Config | null;
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
}

export const useMkvStore = create<MkvStore>((set, get) => ({
  files: [],
  activeTab: "fileList",
  showSettings: false,
  showAbout: false,
  about: null,
  config: null,
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
}));
