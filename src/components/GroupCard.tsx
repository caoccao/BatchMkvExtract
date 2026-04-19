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

import { useEffect, useMemo, useRef, useState } from "react";
import {
  Alert,
  Box,
  Button,
  IconButton,
  LinearProgress,
  List,
  ListItem,
  Paper,
  Snackbar,
  Tooltip,
  Typography,
} from "@mui/material";
import CancelIcon from "@mui/icons-material/Cancel";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import ContentCutIcon from "@mui/icons-material/ContentCut";
import DeleteIcon from "@mui/icons-material/Delete";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import betterMediaInfoIcon from "../assets/bettermediainfo.png";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useTranslation } from "react-i18next";
import {
  cancelExtraction,
  enqueueSelectedTracksForFile,
} from "../actions/extractionActions";
import {
  buildCommandString,
  formatHMS,
  getFileName,
  getParentDir,
  makeTrackSelector,
  resolveOutputDir,
  trackKey,
} from "../extract-utils";
import { QueueItemStatus } from "../protocol";
import { launchBetterMediaInfo } from "../service";
import { useMkvStore } from "../store";
import { CardSummary } from "./CardSummary";
import { FileStatusIcon } from "./FileStatusIcon";
import { OutputPathDialog } from "./OutputPathDialog";
import { TrackSelectionTable } from "./TrackSelectionTable";

interface GroupCardProps {
  files: string[];
}

export function GroupCard({ files }: GroupCardProps) {
  const { t } = useTranslation();
  const [snackbar, setSnackbar] = useState<{
    message: string;
    severity: "success" | "error";
  } | null>(null);
  const [leftWidth, setLeftWidth] = useState(240);
  const [now, setNow] = useState(() => Date.now());
  const [outputDialogOpen, setOutputDialogOpen] = useState(false);
  const [outputDialogInitial, setOutputDialogInitial] = useState("");
  const splitContainerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 200);
    return () => clearInterval(id);
  }, []);

  const startResize = (e: React.MouseEvent<HTMLDivElement>) => {
    e.preventDefault();
    const container = splitContainerRef.current;
    if (!container) {
      return;
    }
    const onMove = (ev: MouseEvent) => {
      const rect = container.getBoundingClientRect();
      const min = 160;
      const max = Math.max(min, rect.width - 240);
      const next = Math.max(min, Math.min(max, ev.clientX - rect.left));
      setLeftWidth(next);
    };
    const onUp = () => {
      document.removeEventListener("mousemove", onMove);
      document.removeEventListener("mouseup", onUp);
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
    document.addEventListener("mousemove", onMove);
    document.addEventListener("mouseup", onUp);
  };

  const mkvToolNixPath = useMkvStore(
    (s) => s.config?.externalTools?.mkvToolNixPath ?? "",
  );
  const activeProfile = useMkvStore((s) => {
    const cfg = s.config;
    if (!cfg) {
      return null;
    }
    return (
      cfg.profiles.find((p) => p.name === cfg.activeProfile) ??
      cfg.profiles[0] ??
      null
    );
  });
  const fileTracksMap = useMkvStore((s) => s.fileTracks);
  const fileSelectedIdsMap = useMkvStore((s) => s.fileSelectedIds);
  const fileOutputDirs = useMkvStore((s) => s.fileOutputDirs);
  const fileTrackCounts = useMkvStore((s) => s.fileTrackCounts);
  const betterMediaInfoAvailable = useMkvStore(
    (s) => s.betterMediaInfoAvailable,
  );
  const setGroupOutputDir = useMkvStore((s) => s.setGroupOutputDir);
  const clearGroupOutputDir = useMkvStore((s) => s.clearGroupOutputDir);
  const queueItems = useMkvStore((s) => s.queueItems);
  const removeFile = useMkvStore((s) => s.removeFile);
  const removeFromQueue = useMkvStore((s) => s.removeFromQueue);
  const setGroupSelectedIds = useMkvStore((s) => s.setGroupSelectedIds);

  const firstFile = files[0];
  const tracks = firstFile ? fileTracksMap[firstFile] ?? [] : [];
  const storedSelected = firstFile ? fileSelectedIdsMap[firstFile] : undefined;
  const selectedIds = useMemo(
    () => new Set<string>(storedSelected ?? []),
    [storedSelected],
  );

  useEffect(() => {
    if (!activeProfile) {
      return;
    }
    const initFile = files.find((f) => fileSelectedIdsMap[f] !== undefined);
    let groupIds: string[];
    if (initFile) {
      groupIds = fileSelectedIdsMap[initFile] ?? [];
    } else if (tracks.length > 0) {
      groupIds = [];
      const selectTrack = makeTrackSelector(activeProfile);
      for (const track of tracks) {
        if (selectTrack(track)) {
          groupIds.push(trackKey(track));
        }
      }
    } else {
      return;
    }
    const needsWrite = files.some(
      (f) => fileSelectedIdsMap[f] !== groupIds,
    );
    if (!needsWrite) {
      return;
    }
    setGroupSelectedIds(files, groupIds);
  }, [files, tracks, activeProfile, fileSelectedIdsMap, setGroupSelectedIds]);

  const hasSelection = selectedIds.size > 0;
  const hasActiveInGroup = files.some((f) => {
    const status = queueItems[f]?.status;
    return (
      status === QueueItemStatus.Waiting ||
      status === QueueItemStatus.Extracting
    );
  });
  const hasWaitingInGroup = files.some(
    (f) => queueItems[f]?.status === QueueItemStatus.Waiting,
  );
  const canExtractAll = hasSelection && !hasActiveInGroup;
  const canCopyAll = hasSelection;
  const canClearAll = files.length > 0 && !hasActiveInGroup;

  const parentDir = firstFile ? getParentDir(firstFile) : "";

  const groupOutputDir = useMemo(() => {
    if (files.length === 0) {
      return undefined;
    }
    const first = fileOutputDirs[files[0]];
    if (!first || first.length === 0) {
      return undefined;
    }
    for (let i = 1; i < files.length; i += 1) {
      if (fileOutputDirs[files[i]] !== first) {
        return undefined;
      }
    }
    return first;
  }, [files, fileOutputDirs]);

  const handleOpenOutputDialog = () => {
    setOutputDialogInitial(groupOutputDir ?? parentDir);
    setOutputDialogOpen(true);
  };

  const handleOpenInBetterMediaInfo = async () => {
    try {
      await launchBetterMediaInfo(files);
    } catch (err) {
      setSnackbar({ message: String(err), severity: "error" });
    }
  };

  const handleOutputConfirm = (value: string) => {
    if (value.length === 0) {
      clearGroupOutputDir(files);
    } else {
      setGroupOutputDir(files, value);
    }
  };

  const toggleAll = (checked: boolean) => {
    setGroupSelectedIds(
      files,
      checked ? tracks.map((t) => trackKey(t)) : [],
    );
  };

  const toggleOne = (key: string, checked: boolean) => {
    const current = storedSelected ?? [];
    const next = checked
      ? [...current, key]
      : current.filter((v) => v !== key);
    setGroupSelectedIds(files, next);
  };

  const selectedTracksFor = (file: string) => {
    const fileTracks = fileTracksMap[file] ?? [];
    return fileTracks.filter((track) => selectedIds.has(trackKey(track)));
  };

  const handleCopyAll = async () => {
    if (!activeProfile || !hasSelection) {
      return;
    }
    const commands: string[] = [];
    try {
      for (const file of files) {
        const selectedTracks = selectedTracksFor(file);
        if (selectedTracks.length === 0) {
          continue;
        }
        const outputDir = await resolveOutputDir(file, fileOutputDirs[file]);
        const command = await buildCommandString(
          file,
          outputDir,
          mkvToolNixPath,
          selectedTracks,
          activeProfile,
        );
        commands.push(command);
      }
      if (commands.length === 0) {
        return;
      }
      await writeText(commands.join("\n"));
      setSnackbar({
        message: t("extract.commandCopied"),
        severity: "success",
      });
    } catch (err) {
      setSnackbar({ message: String(err), severity: "error" });
    }
  };

  const handleExtractAll = async () => {
    if (!activeProfile || !hasSelection) {
      return;
    }
    for (const file of files) {
      const selectedTracks = selectedTracksFor(file);
      if (selectedTracks.length === 0) {
        continue;
      }
      try {
        await enqueueSelectedTracksForFile({
          file,
          selectedTracks,
          profile: activeProfile,
          t,
        });
      } catch (err) {
        setSnackbar({ message: String(err), severity: "error" });
        return;
      }
    }
  };

  const handleCancel = async (file: string) => {
    await cancelExtraction(file);
  };

  const handleClearAll = async () => {
    for (const file of [...files]) {
      const current = useMkvStore.getState().queueItems[file];
      if (current?.status === QueueItemStatus.Extracting) {
        continue;
      }
      if (current?.status === QueueItemStatus.Waiting) {
        await cancelExtraction(file);
      }
      removeFromQueue(file);
      removeFile(file);
    }
  };

  return (
    <Paper
      variant="outlined"
      sx={{
        mt: 1,
        p: 1,
        borderColor: "primary.main",
        bgcolor: hasWaitingInGroup ? "action.hover" : undefined,
      }}
    >
      <Box
        sx={{
          display: "flex",
          alignItems: "center",
          gap: 1,
          mb: 1,
        }}
      >
        <Box sx={{ flex: 1, minWidth: 0, ml: 2 }}>
          <Typography
            variant="body2"
            sx={{ wordBreak: "break-all", color: "text.secondary" }}
          >
            {parentDir}
          </Typography>
          <CardSummary
            counts={firstFile ? fileTrackCounts[firstFile] : undefined}
            outputPath={groupOutputDir}
          />
        </Box>
        <Tooltip title={t("extract.setOutputPath")}>
          <span>
            <IconButton
              size="small"
              disabled={hasActiveInGroup}
              onClick={handleOpenOutputDialog}
            >
              <FolderOpenIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
        {betterMediaInfoAvailable && (
          <Tooltip title={t("extract.openInBetterMediaInfo")}>
            <span>
              <IconButton size="small" onClick={handleOpenInBetterMediaInfo}>
                <Box
                  component="img"
                  src={betterMediaInfoIcon}
                  alt="BetterMediaInfo"
                  sx={{ width: 20, height: 20 }}
                />
              </IconButton>
            </span>
          </Tooltip>
        )}
        <Tooltip title={t("group.copyAllCommands")}>
          <span>
            <IconButton
              size="small"
              disabled={!canCopyAll}
              onClick={handleCopyAll}
            >
              <ContentCopyIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
        <Tooltip title={t("group.extractAll")}>
          <span>
            <Button
              variant="outlined"
              size="small"
              startIcon={<ContentCutIcon />}
              disabled={!canExtractAll}
              onClick={handleExtractAll}
              sx={{ textTransform: "none", whiteSpace: "nowrap" }}
            >
              {t("group.extractAll")}
            </Button>
          </span>
        </Tooltip>
        <Tooltip title={t("group.clearAll")}>
          <span>
            <IconButton
              size="small"
              color="error"
              disabled={!canClearAll}
              onClick={handleClearAll}
            >
              <DeleteIcon fontSize="small" />
            </IconButton>
          </span>
        </Tooltip>
      </Box>
      <Box
        ref={splitContainerRef}
        sx={{ display: "flex", position: "relative" }}
      >
        <Box
          sx={{
            width: leftWidth,
            flexShrink: 0,
            overflow: "auto",
          }}
        >
          <List dense>
            {files.map((file) => {
              const entry = queueItems[file];
              const isExtracting = entry?.status === QueueItemStatus.Extracting;
              const startedAt = entry?.extractionStartedAt ?? null;
              const elapsedMs =
                isExtracting && startedAt !== null ? now - startedAt : 0;
              const progressPct = entry?.progress ?? 0;
              const elapsedStr = isExtracting
                ? formatHMS(elapsedMs)
                : "--:--:--";
              const etaStr =
                isExtracting && progressPct > 0 && progressPct < 100
                  ? formatHMS((elapsedMs * (100 - progressPct)) / progressPct)
                  : "--:--:--";
              return (
                <ListItem
                  key={file}
                  sx={{
                    py: 0.5,
                    alignItems: "flex-start",
                    flexDirection: "column",
                    gap: 0.5,
                  }}
                >
                  <Box
                    sx={{
                      display: "flex",
                      alignItems: "center",
                      gap: 0.5,
                      width: "100%",
                    }}
                  >
                    <FileStatusIcon status={entry?.status} />
                    <Typography
                      variant="body2"
                      sx={{ wordBreak: "break-all", flex: 1 }}
                    >
                      {getFileName(file)}
                    </Typography>
                  </Box>
                  {isExtracting ? (
                    <>
                      <Box
                        sx={{
                          display: "flex",
                          alignItems: "center",
                          gap: 0.5,
                          width: "100%",
                        }}
                      >
                        <LinearProgress
                          variant="determinate"
                          value={entry?.progress ?? 0}
                          sx={{
                            flex: 1,
                            height: 6,
                            borderRadius: 1,
                            bgcolor: "action.hover",
                            "& .MuiLinearProgress-bar": {
                              bgcolor: "success.main",
                            },
                          }}
                        />
                        <Typography
                          variant="caption"
                          sx={{
                            fontFamily:
                              "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
                            color: "text.secondary",
                            minWidth: 34,
                            textAlign: "right",
                          }}
                        >
                          {entry?.progress ?? 0}%
                        </Typography>
                        <Tooltip title={t("extract.cancel")}>
                          <IconButton
                            size="small"
                            color="error"
                            onClick={() => handleCancel(file)}
                          >
                            <CancelIcon fontSize="small" />
                          </IconButton>
                        </Tooltip>
                      </Box>
                      <Typography
                        variant="caption"
                        sx={{
                          fontFamily:
                            "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
                          color: "text.secondary",
                          width: "100%",
                        }}
                      >
                        {elapsedStr} / {etaStr}
                      </Typography>
                    </>
                  ) : null}
                </ListItem>
              );
            })}
          </List>
        </Box>
        <Box
          onMouseDown={startResize}
          sx={{
            width: 6,
            flexShrink: 0,
            cursor: "col-resize",
            bgcolor: "divider",
            "&:hover": { bgcolor: "action.hover" },
            transition: "background-color 0.15s",
          }}
        />
        <Box sx={{ flex: 1, minWidth: 0, ml: 1 }}>
          <TrackSelectionTable
            tracks={tracks}
            selectedIds={selectedIds}
            disabled={hasActiveInGroup}
            emptyText={t("extract.noTracks")}
            headers={{
              id: t("extract.header.id"),
              number: t("extract.header.number"),
              type: t("extract.header.type"),
              codec: t("extract.header.codec"),
              trackName: t("extract.header.trackName"),
              language: t("extract.header.language"),
            }}
            onToggleAll={toggleAll}
            onToggleOne={toggleOne}
          />
        </Box>
      </Box>
      <Snackbar
        open={snackbar !== null}
        autoHideDuration={3000}
        onClose={() => setSnackbar(null)}
        anchorOrigin={{ vertical: "bottom", horizontal: "center" }}
      >
        <Alert
          onClose={() => setSnackbar(null)}
          severity={snackbar?.severity ?? "success"}
          variant="filled"
        >
          {snackbar?.message}
        </Alert>
      </Snackbar>
      <OutputPathDialog
        open={outputDialogOpen}
        initialValue={outputDialogInitial}
        onConfirm={handleOutputConfirm}
        onClose={() => setOutputDialogOpen(false)}
      />
    </Paper>
  );
}
