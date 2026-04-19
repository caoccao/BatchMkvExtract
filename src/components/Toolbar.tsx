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

import { useCallback, useEffect, useRef, useState } from "react";
import {
  Box,
  ButtonGroup,
  IconButton,
  ListItemIcon,
  ListItemText,
  Menu,
  MenuItem,
  Tooltip,
} from "@mui/material";
import CancelIcon from "@mui/icons-material/Cancel";
import CheckIcon from "@mui/icons-material/Check";
import ContentCutIcon from "@mui/icons-material/ContentCut";
import DeleteIcon from "@mui/icons-material/Delete";
import FolderCopyIcon from "@mui/icons-material/FolderCopy";
import InfoIcon from "@mui/icons-material/Info";
import PersonIcon from "@mui/icons-material/Person";
import SettingsIcon from "@mui/icons-material/Settings";
import { dirname } from "@tauri-apps/api/path";
import { useTranslation } from "react-i18next";
import { buildExtractArgs } from "../extract-utils";
import { QueueItemStatus } from "../protocol";
import { cancelExtract, ensureOutputPath, enqueueExtract } from "../service";
import { useMkvStore } from "../store";

export default function Toolbar() {
  const { t } = useTranslation();
  const tRef = useRef(t);
  tRef.current = t;
  const activeTab = useMkvStore((s) => s.activeTab);
  const files = useMkvStore((s) => s.files);
  const openSettings = useMkvStore((s) => s.openSettings);
  const openAbout = useMkvStore((s) => s.openAbout);
  const clearFiles = useMkvStore((s) => s.clearFiles);

  const hasFiles = files.length > 0;
  const fileSelectedIdsMap = useMkvStore((s) => s.fileSelectedIds);
  const canExtractAll = files.some(
    (f) => (fileSelectedIdsMap[f]?.length ?? 0) > 0,
  );
  const queueItems = useMkvStore((s) => s.queueItems);
  const hasActiveJobs = Object.values(queueItems).some(
    (item) =>
      item.status === QueueItemStatus.Waiting ||
      item.status === QueueItemStatus.Extracting,
  );
  const canClear = hasFiles && !hasActiveJobs;
  const groupByFile = useMkvStore((s) => s.groupByFile);
  const setGroupByFile = useMkvStore((s) => s.setGroupByFile);
  const config = useMkvStore((s) => s.config);
  const setActiveProfile = useMkvStore((s) => s.setActiveProfile);
  const [profileAnchor, setProfileAnchor] = useState<null | HTMLElement>(null);
  const profileButtonRef = useRef<HTMLButtonElement | null>(null);
  const profiles = config?.profiles ?? [];
  const activeProfileName = config?.activeProfile ?? "";

  const openProfileMenu = useCallback(() => {
    if (profileButtonRef.current) {
      setProfileAnchor(profileButtonRef.current);
    }
  }, []);

  const runCancelAll = useCallback(async () => {
    const state = useMkvStore.getState();
    const activeFiles = Object.values(state.queueItems)
      .filter(
        (item) =>
          item.status === QueueItemStatus.Waiting ||
          item.status === QueueItemStatus.Extracting,
      )
      .map((item) => item.file);
    await Promise.all(
      activeFiles.map(async (file) => {
        state.markCancelRequested(file);
        try {
          await cancelExtract(file);
        } catch (err) {
          console.error("Cancel failed for", file, err);
        }
      }),
    );
  }, []);

  const runExtractAll = useCallback(async () => {
    const state = useMkvStore.getState();
    const cfg = state.config;
    const profile = cfg
      ? cfg.profiles.find((p) => p.name === cfg.activeProfile) ??
        cfg.profiles[0] ??
        null
      : null;
    if (!profile) {
      return;
    }
    for (const file of state.files) {
      const tracks = state.fileTracks[file] ?? [];
      const ids = new Set(state.fileSelectedIds[file] ?? []);
      if (ids.size === 0 || tracks.length === 0) {
        continue;
      }
      const status = state.queueItems[file]?.status;
      if (
        status === QueueItemStatus.Waiting ||
        status === QueueItemStatus.Extracting
      ) {
        continue;
      }
      const selectedTracks = tracks.filter((track) => ids.has(track.id));
      if (selectedTracks.length === 0) {
        continue;
      }
      try {
        const override = state.fileOutputDirs[file];
        const outputDir =
          override && override.length > 0 ? override : await dirname(file);
        try {
          await ensureOutputPath(outputDir);
        } catch {
          state.showNotification(
            "error",
            file,
            tRef.current("notification.failedCreateOutput", {
              path: outputDir,
            }),
          );
          continue;
        }
        const args = await buildExtractArgs(
          file,
          outputDir,
          selectedTracks,
          profile,
        );
        await enqueueExtract(file, args);
        state.addToQueue(file);
      } catch (err) {
        console.error("Extract All failed for", file, err);
      }
    }
  }, []);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (
        event.key === "F3" ||
        event.key === "F4" ||
        event.key === "F8" ||
        event.key === "F9" ||
        event.key === "F10"
      ) {
        event.preventDefault();
      }
    };
    const handleKeyUp = (event: KeyboardEvent) => {
      if (
        event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        (event.key === "q" || event.key === "Q")
      ) {
        event.stopPropagation();
        const state = useMkvStore.getState();
        const hasActive = Object.values(state.queueItems).some(
          (item) =>
            item.status === QueueItemStatus.Waiting ||
            item.status === QueueItemStatus.Extracting,
        );
        if (state.files.length > 0 && !hasActive) {
          clearFiles();
        }
      } else if (
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.key === "F3"
      ) {
        event.preventDefault();
        event.stopPropagation();
        const state = useMkvStore.getState();
        if (
          state.files.some((f) => (state.fileSelectedIds[f]?.length ?? 0) > 0)
        ) {
          runExtractAll();
        }
      } else if (
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.key === "F4"
      ) {
        event.preventDefault();
        event.stopPropagation();
        const hasActive = Object.values(
          useMkvStore.getState().queueItems,
        ).some(
          (item) =>
            item.status === QueueItemStatus.Waiting ||
            item.status === QueueItemStatus.Extracting,
        );
        if (hasActive) {
          runCancelAll();
        }
      } else if (
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.key === "F8"
      ) {
        event.preventDefault();
        event.stopPropagation();
        setGroupByFile(!useMkvStore.getState().groupByFile);
      } else if (
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.key === "F9"
      ) {
        event.preventDefault();
        event.stopPropagation();
        openProfileMenu();
      } else if (
        !event.ctrlKey &&
        !event.altKey &&
        !event.shiftKey &&
        event.key === "F10"
      ) {
        event.preventDefault();
        event.stopPropagation();
        openSettings();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    document.addEventListener("keyup", handleKeyUp);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.removeEventListener("keyup", handleKeyUp);
    };
  }, [
    clearFiles,
    runExtractAll,
    runCancelAll,
    openProfileMenu,
    openSettings,
    setGroupByFile,
  ]);

  const buttonSx = {
    width: 28,
    height: 28,
    margin: "2px",
    borderRadius: 1,
  };
  const activeButtonSx = {
    ...buttonSx,
    color: "primary.main",
  };

  return (
    <Box sx={{ mx: 1, my: 0, display: "flex", gap: 1 }}>
      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.extractAll")}>
          <span>
            <IconButton
              sx={buttonSx}
              disabled={!canExtractAll}
              onClick={runExtractAll}
            >
              <ContentCutIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
        <Tooltip title={t("toolbar.cancelAll")}>
          <span>
            <IconButton
              sx={buttonSx}
              color="error"
              disabled={!hasActiveJobs}
              onClick={runCancelAll}
            >
              <CancelIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
        <Tooltip title={t("toolbar.clear")}>
          <span>
            <IconButton
              sx={buttonSx}
              disabled={!canClear}
              onClick={clearFiles}
            >
              <DeleteIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
        <Tooltip title={t("toolbar.groupByFile")}>
          <IconButton
            sx={groupByFile ? activeButtonSx : buttonSx}
            onClick={() => setGroupByFile(!groupByFile)}
          >
            <FolderCopyIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </ButtonGroup>

      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.profile")}>
          <span>
            <IconButton
              ref={profileButtonRef}
              sx={buttonSx}
              disabled={profiles.length === 0}
              onClick={(e) => setProfileAnchor(e.currentTarget)}
            >
              <PersonIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
        <Tooltip title={t("toolbar.settings")}>
          <IconButton
            sx={activeTab === "settings" ? activeButtonSx : buttonSx}
            onClick={openSettings}
          >
            <SettingsIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <Tooltip title={t("toolbar.about")}>
          <IconButton
            sx={activeTab === "about" ? activeButtonSx : buttonSx}
            onClick={openAbout}
          >
            <InfoIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </ButtonGroup>

      <Menu
        anchorEl={profileAnchor}
        open={Boolean(profileAnchor)}
        onClose={() => setProfileAnchor(null)}
        anchorOrigin={{ vertical: "bottom", horizontal: "left" }}
        transformOrigin={{ vertical: "top", horizontal: "left" }}
      >
        {profiles.map((p) => (
          <MenuItem
            key={p.name}
            selected={p.name === activeProfileName}
            onClick={() => {
              setActiveProfile(p.name);
              setProfileAnchor(null);
            }}
          >
            <ListItemIcon sx={{ minWidth: 28 }}>
              {p.name === activeProfileName && (
                <CheckIcon fontSize="small" color="primary" />
              )}
            </ListItemIcon>
            <ListItemText slotProps={{ primary: { variant: "body2" } }}>
              {p.name}
            </ListItemText>
          </MenuItem>
        ))}
      </Menu>
    </Box>
  );
}
