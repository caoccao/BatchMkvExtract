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

import { useEffect, useMemo } from "react";
import {
  Box,
  CssBaseline,
  ThemeProvider,
  createTheme,
  useMediaQuery,
} from "@mui/material";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ProgressBarStatus } from "@tauri-apps/api/window";
import Layout from "./components/Layout";
import { changeLanguage } from "./i18n";
import * as Protocol from "./protocol";
import { QueueItemStatus } from "./protocol";
import { getLaunchArgs, getMkvFiles } from "./service";
import { useMkvStore } from "./store";

function getPaletteByTheme(theme: Protocol.Theme, mode: "light" | "dark") {
  switch (theme) {
    case Protocol.Theme.Ocean:
      return { mode, primary: { main: "#0288d1" }, secondary: { main: "#26c6da" } };
    case Protocol.Theme.Aqua:
      return { mode, primary: { main: "#00acc1" }, secondary: { main: "#4dd0e1" } };
    case Protocol.Theme.Sky:
      return { mode, primary: { main: "#42a5f5" }, secondary: { main: "#90caf9" } };
    case Protocol.Theme.Arctic:
      return { mode, primary: { main: "#4fc3f7" }, secondary: { main: "#b3e5fc" } };
    case Protocol.Theme.Glacier:
      return { mode, primary: { main: "#5c6bc0" }, secondary: { main: "#9fa8da" } };
    case Protocol.Theme.Mist:
      return { mode, primary: { main: "#90a4ae" }, secondary: { main: "#cfd8dc" } };
    case Protocol.Theme.Slate:
      return { mode, primary: { main: "#546e7a" }, secondary: { main: "#78909c" } };
    case Protocol.Theme.Charcoal:
      return { mode, primary: { main: "#37474f" }, secondary: { main: "#607d8b" } };
    case Protocol.Theme.Midnight:
      return { mode, primary: { main: "#1a237e" }, secondary: { main: "#3949ab" } };
    case Protocol.Theme.Indigo:
      return { mode, primary: { main: "#3f51b5" }, secondary: { main: "#7986cb" } };
    case Protocol.Theme.Violet:
      return { mode, primary: { main: "#7e57c2" }, secondary: { main: "#b39ddb" } };
    case Protocol.Theme.Lavender:
      return { mode, primary: { main: "#9575cd" }, secondary: { main: "#d1c4e9" } };
    case Protocol.Theme.Rose:
      return { mode, primary: { main: "#c2185b" }, secondary: { main: "#f06292" } };
    case Protocol.Theme.Blush:
      return { mode, primary: { main: "#ec407a" }, secondary: { main: "#f48fb1" } };
    case Protocol.Theme.Coral:
      return { mode, primary: { main: "#ff7043" }, secondary: { main: "#ffab91" } };
    case Protocol.Theme.Sunset:
      return { mode, primary: { main: "#ef6c00" }, secondary: { main: "#ff8a65" } };
    case Protocol.Theme.Amber:
      return { mode, primary: { main: "#ff8f00" }, secondary: { main: "#ffca28" } };
    case Protocol.Theme.Sand:
      return { mode, primary: { main: "#bcaaa4" }, secondary: { main: "#d7ccc8" } };
    case Protocol.Theme.Forest:
      return { mode, primary: { main: "#2e7d32" }, secondary: { main: "#66bb6a" } };
    case Protocol.Theme.Emerald:
      return { mode, primary: { main: "#00897b" }, secondary: { main: "#4db6ac" } };
    default:
      return { mode, primary: { main: "#0288d1" }, secondary: { main: "#26c6da" } };
  }
}

function App() {
  const config = useMkvStore((s) => s.config);
  const initConfig = useMkvStore((s) => s.initConfig);
  const about = useMkvStore((s) => s.about);
  const initAbout = useMkvStore((s) => s.initAbout);
  const queueItems = useMkvStore((s) => s.queueItems);

  const displayMode = config?.displayMode ?? Protocol.DisplayMode.Auto;
  const selectedTheme = config?.theme ?? Protocol.Theme.Ocean;
  const language = config?.language ?? Protocol.Language.EnUS;

  useEffect(() => {
    initConfig();
  }, [initConfig]);

  useEffect(() => {
    initAbout();
  }, [initAbout]);

  useEffect(() => {
    const base = about?.appVersion
      ? `BatchMkvExtract v${about.appVersion}`
      : "BatchMkvExtract";
    const items = Object.values(queueItems);
    const total = items.length;
    let done = 0;
    let hasActive = false;
    let progressSum = 0;
    for (const item of items) {
      if (item.status === QueueItemStatus.Waiting) {
        hasActive = true;
      } else if (item.status === QueueItemStatus.Extracting) {
        hasActive = true;
        progressSum += item.progress;
      } else {
        done += 1;
        progressSum += 100;
      }
    }
    const win = getCurrentWebviewWindow();
    const title = hasActive ? `${base} - ${done}/${total}` : base;
    win
      .setTitle(title)
      .catch((err) => console.error("Failed to set window title", err));
    if (hasActive && total > 0) {
      const overall = Math.max(
        0,
        Math.min(100, Math.floor(progressSum / total)),
      );
      win
        .setProgressBar({
          status: ProgressBarStatus.Normal,
          progress: overall,
        })
        .catch((err) => console.error("Failed to set progress bar", err));
    } else {
      win
        .setProgressBar({ status: ProgressBarStatus.None })
        .catch((err) => console.error("Failed to set progress bar", err));
    }
  }, [about, queueItems]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const args = await getLaunchArgs();
        if (cancelled || args.length === 0) {
          return;
        }
        const mkvFiles = await getMkvFiles(args);
        if (cancelled || mkvFiles.length === 0) {
          return;
        }
        useMkvStore.getState().addFiles(mkvFiles);
      } catch (err) {
        console.error("Failed to process launch args", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    changeLanguage(language);
  }, [language]);

  const prefersDarkMode = useMediaQuery("(prefers-color-scheme: dark)");

  const mode: "light" | "dark" = useMemo(() => {
    if (displayMode === Protocol.DisplayMode.Auto) {
      return prefersDarkMode ? "dark" : "light";
    }
    return displayMode === Protocol.DisplayMode.Dark ? "dark" : "light";
  }, [displayMode, prefersDarkMode]);

  const theme = useMemo(
    () =>
      createTheme({
        palette: getPaletteByTheme(selectedTheme, mode),
        typography: { fontSize: 12 },
      }),
    [mode, selectedTheme],
  );

  return (
    <ThemeProvider theme={theme}>
      <CssBaseline />
      <Box
        sx={{
          minHeight: "100vh",
          bgcolor: "background.default",
          color: "text.primary",
        }}
      >
        <Layout />
      </Box>
    </ThemeProvider>
  );
}

export default App;
