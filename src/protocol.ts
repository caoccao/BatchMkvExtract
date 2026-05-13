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

export interface About {
  appVersion: string;
}

export const DisplayMode = {
  Auto: "Auto",
  Light: "Light",
  Dark: "Dark",
} as const;
export type DisplayMode = (typeof DisplayMode)[keyof typeof DisplayMode];

export const Theme = {
  Ocean: "Ocean",
  Aqua: "Aqua",
  Sky: "Sky",
  Arctic: "Arctic",
  Glacier: "Glacier",
  Mist: "Mist",
  Slate: "Slate",
  Charcoal: "Charcoal",
  Midnight: "Midnight",
  Indigo: "Indigo",
  Violet: "Violet",
  Lavender: "Lavender",
  Rose: "Rose",
  Blush: "Blush",
  Coral: "Coral",
  Sunset: "Sunset",
  Amber: "Amber",
  Sand: "Sand",
  Forest: "Forest",
  Emerald: "Emerald",
} as const;
export type Theme = (typeof Theme)[keyof typeof Theme];

export const Language = {
  De: "de",
  EnUS: "en-US",
  Es: "es",
  Fr: "fr",
  It: "it",
  Ja: "ja",
  ZhCN: "zh-CN",
  ZhHK: "zh-HK",
  ZhTW: "zh-TW",
} as const;
export type Language = (typeof Language)[keyof typeof Language];

export interface ConfigWindowPosition {
  x: number;
  y: number;
}

export interface ConfigWindowSize {
  width: number;
  height: number;
}

export interface ConfigWindow {
  position: ConfigWindowPosition;
  size: ConfigWindowSize;
}

export const UpdateCheckInterval = {
  Daily: "Daily",
  Weekly: "Weekly",
  Monthly: "Monthly",
} as const;
export type UpdateCheckInterval =
  (typeof UpdateCheckInterval)[keyof typeof UpdateCheckInterval];

export interface ConfigUpdate {
  checkInterval: UpdateCheckInterval;
  lastChecked: number;
  lastVersion: string;
  ignoreVersion: string;
}

export interface UpdateCheckResult {
  hasUpdate: boolean;
  latestVersion: string | null;
}

export interface ConfigExternalTools {
  mkvToolNixPath: string;
  betterMediaInfoPath: string;
}

export interface ConfigProfile {
  name: string;
  videoTemplate: string;
  audioTemplate: string;
  subtitleTemplate: string;
  chaptersTemplate: string;
  attachmentsTemplate: string;
  selectVideo: boolean;
  selectAudio: boolean;
  selectSubtitle: boolean;
  selectChapters: boolean;
  selectAttachments: boolean;
  videoLanguages: string;
  audioLanguages: string;
  subtitleLanguages: string;
  defaultGroupMode: boolean;
}

export interface Config {
  displayMode: DisplayMode;
  theme: Theme;
  language: Language;
  externalTools: ConfigExternalTools;
  profiles: ConfigProfile[];
  activeProfile: string;
  window: ConfigWindow;
  update: ConfigUpdate;
}

export const DEFAULT_PROFILE_NAME = "Default";
export const DEFAULT_TEMPLATE = "{file_name}.{track_id}.{language}";
export const DEFAULT_TEMPLATE_NO_LANGUAGE = "{file_name}.{track_id}";
export const DEFAULT_SUBTITLE_LANGUAGES = "eng, chi, spa, ger, fre, jpn";

export function createDefaultProfile(name = DEFAULT_PROFILE_NAME): ConfigProfile {
  const isDefault = name === DEFAULT_PROFILE_NAME;
  return {
    name,
    videoTemplate: DEFAULT_TEMPLATE,
    audioTemplate: DEFAULT_TEMPLATE,
    subtitleTemplate: DEFAULT_TEMPLATE,
    chaptersTemplate: DEFAULT_TEMPLATE_NO_LANGUAGE,
    attachmentsTemplate: DEFAULT_TEMPLATE_NO_LANGUAGE,
    selectVideo: false,
    selectAudio: false,
    selectSubtitle: isDefault,
    selectChapters: false,
    selectAttachments: false,
    videoLanguages: "",
    audioLanguages: "",
    subtitleLanguages: DEFAULT_SUBTITLE_LANGUAGES,
    defaultGroupMode: isDefault,
  };
}

export interface MkvToolNixStatus {
  found: boolean;
  mkvToolNixPath: string;
}

export interface BetterMediaInfoStatus {
  found: boolean;
  path: string;
}

export interface MkvTrack {
  id: number;
  number: number;
  type: string;
  codec: string;
  codecId: string;
  trackName: string;
  language: string;
}

export enum QueueItemStatus {
  Waiting = "Waiting",
  Extracting = "Extracting",
  Completed = "Completed",
  Cancelled = "Cancelled",
  Failed = "Failed",
}

export type ExtractActiveStatus =
  | QueueItemStatus.Waiting
  | QueueItemStatus.Extracting;

export type ExtractOutcome =
  | QueueItemStatus.Completed
  | QueueItemStatus.Cancelled
  | QueueItemStatus.Failed;

export interface ExtractEntry {
  file: string;
  status: ExtractActiveStatus;
  progress: number;
}

export interface ExtractSnapshot {
  entries: ExtractEntry[];
}

export interface ExtractionFinishedEvent {
  file: string;
  outcome: ExtractOutcome;
  error: string | null;
}

export function getDisplayModes(): DisplayMode[] {
  return [DisplayMode.Auto, DisplayMode.Light, DisplayMode.Dark];
}

export function getThemes(): Theme[] {
  return Object.values(Theme);
}

export function getLanguages(): Language[] {
  return Object.values(Language);
}

const LANGUAGE_LABELS: Record<Language, string> = {
  "de": "Deutsch",
  "en-US": "English (US)",
  "es": "Español",
  "fr": "Français",
  "it": "Italiano",
  "ja": "日本語",
  "zh-CN": "简体中文",
  "zh-HK": "繁體中文 (香港)",
  "zh-TW": "繁體中文 (台灣)",
};

export function getLanguageLabel(language: Language): string {
  return LANGUAGE_LABELS[language];
}
