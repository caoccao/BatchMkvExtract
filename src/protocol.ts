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

export interface ConfigMkv {
  mkvToolNixPath: string;
}

export interface Config {
  displayMode: DisplayMode;
  theme: Theme;
  language: Language;
  mkv: ConfigMkv;
  window: ConfigWindow;
}

export interface MkvextractStatus {
  found: boolean;
  mkvToolNixPath: string;
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
  "ja": "日本語",
  "zh-CN": "简体中文",
  "zh-HK": "繁體中文 (香港)",
  "zh-TW": "繁體中文 (台灣)",
};

export function getLanguageLabel(language: Language): string {
  return LANGUAGE_LABELS[language];
}
