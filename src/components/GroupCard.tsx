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
  Checkbox,
  IconButton,
  LinearProgress,
  List,
  ListItem,
  Paper,
  Snackbar,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Tooltip,
  Typography,
} from "@mui/material";
import CancelIcon from "@mui/icons-material/Cancel";
import ClosedCaptionIcon from "@mui/icons-material/ClosedCaption";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import ContentCutIcon from "@mui/icons-material/ContentCut";
import DeleteIcon from "@mui/icons-material/Delete";
import HelpOutlineOutlinedIcon from "@mui/icons-material/HelpOutlineOutlined";
import ImageIcon from "@mui/icons-material/Image";
import MusicNoteIcon from "@mui/icons-material/MusicNote";
import SmartButtonIcon from "@mui/icons-material/SmartButton";
import VideocamIcon from "@mui/icons-material/Videocam";
import { dirname } from "@tauri-apps/api/path";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useTranslation } from "react-i18next";
import {
  buildCommandString,
  buildExtractArgs,
  getFileName,
  getParentDir,
  shouldSelectTrackType,
} from "../extract-utils";
import { QueueItemStatus } from "../protocol";
import { cancelExtract, enqueueExtract } from "../service";
import { useMkvStore } from "../store";

interface GroupCardProps {
  files: string[];
}

function TrackTypeIcon({ type }: { type: string }) {
  const sx = { fontSize: 18 };
  switch (type) {
    case "video":
      return (
        <Tooltip title="video">
          <VideocamIcon sx={sx} />
        </Tooltip>
      );
    case "audio":
      return (
        <Tooltip title="audio">
          <MusicNoteIcon sx={sx} />
        </Tooltip>
      );
    case "subtitles":
      return (
        <Tooltip title="subtitles">
          <ClosedCaptionIcon sx={sx} />
        </Tooltip>
      );
    case "buttons":
      return (
        <Tooltip title="buttons">
          <SmartButtonIcon sx={sx} />
        </Tooltip>
      );
    case "images":
      return (
        <Tooltip title="images">
          <ImageIcon sx={sx} />
        </Tooltip>
      );
    default:
      return (
        <Tooltip title={type}>
          <HelpOutlineOutlinedIcon sx={sx} />
        </Tooltip>
      );
  }
}

function statusColor(status: QueueItemStatus | undefined): string {
  switch (status) {
    case QueueItemStatus.Extracting:
      return "success.main";
    case QueueItemStatus.Completed:
      return "text.secondary";
    case QueueItemStatus.Cancelled:
    case QueueItemStatus.Failed:
      return "error.main";
    case QueueItemStatus.Waiting:
      return "text.primary";
    default:
      return "text.secondary";
  }
}

export function GroupCard({ files }: GroupCardProps) {
  const { t } = useTranslation();
  const [snackbar, setSnackbar] = useState<{
    message: string;
    severity: "success" | "error";
  } | null>(null);
  const [leftWidth, setLeftWidth] = useState(240);
  const splitContainerRef = useRef<HTMLDivElement | null>(null);

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
    (s) => s.config?.mkv?.mkvToolNixPath ?? "",
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
  const queueItems = useMkvStore((s) => s.queueItems);
  const addToQueue = useMkvStore((s) => s.addToQueue);
  const markCancelRequested = useMkvStore((s) => s.markCancelRequested);
  const removeFile = useMkvStore((s) => s.removeFile);
  const removeFromQueue = useMkvStore((s) => s.removeFromQueue);
  const setGroupSelectedIds = useMkvStore((s) => s.setGroupSelectedIds);

  const firstFile = files[0];
  const tracks = firstFile ? fileTracksMap[firstFile] ?? [] : [];
  const storedSelected = firstFile ? fileSelectedIdsMap[firstFile] : undefined;
  const selectedIds = useMemo(
    () => new Set<number>(storedSelected ?? []),
    [storedSelected],
  );

  useEffect(() => {
    if (!activeProfile) {
      return;
    }
    const initFile = files.find((f) => fileSelectedIdsMap[f] !== undefined);
    let groupIds: number[];
    if (initFile) {
      groupIds = fileSelectedIdsMap[initFile] ?? [];
    } else if (tracks.length > 0) {
      groupIds = [];
      for (const track of tracks) {
        if (shouldSelectTrackType(activeProfile, track.type)) {
          groupIds.push(track.id);
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
  const canExtractAll = hasSelection && !hasActiveInGroup;
  const canCopyAll = hasSelection;
  const canClearAll = files.length > 0 && !hasActiveInGroup;

  const parentDir = firstFile ? getParentDir(firstFile) : "";

  const toggleAll = (checked: boolean) => {
    setGroupSelectedIds(files, checked ? tracks.map((t) => t.id) : []);
  };

  const toggleOne = (id: number, checked: boolean) => {
    const current = storedSelected ?? [];
    const next = checked
      ? [...current, id]
      : current.filter((v) => v !== id);
    setGroupSelectedIds(files, next);
  };

  const selectedTracksFor = (file: string) => {
    const fileTracks = fileTracksMap[file] ?? [];
    return fileTracks.filter((track) => selectedIds.has(track.id));
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
        const outputDir = await dirname(file);
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
      const status = useMkvStore.getState().queueItems[file]?.status;
      if (
        status === QueueItemStatus.Waiting ||
        status === QueueItemStatus.Extracting
      ) {
        continue;
      }
      const selectedTracks = selectedTracksFor(file);
      if (selectedTracks.length === 0) {
        continue;
      }
      try {
        const outputDir = await dirname(file);
        const args = await buildExtractArgs(
          file,
          outputDir,
          selectedTracks,
          activeProfile,
        );
        await enqueueExtract(file, args);
        addToQueue(file);
      } catch (err) {
        setSnackbar({ message: String(err), severity: "error" });
        return;
      }
    }
  };

  const handleCancel = async (file: string) => {
    markCancelRequested(file);
    try {
      await cancelExtract(file);
    } catch {
      // ignore
    }
  };

  const handleClearAll = async () => {
    for (const file of [...files]) {
      const current = useMkvStore.getState().queueItems[file];
      if (current?.status === QueueItemStatus.Extracting) {
        continue;
      }
      if (current?.status === QueueItemStatus.Waiting) {
        markCancelRequested(file);
        try {
          await cancelExtract(file);
        } catch {
          // ignore; continue
        }
      }
      removeFromQueue(file);
      removeFile(file);
    }
  };

  return (
    <Paper
      variant="outlined"
      sx={{ mt: 1, p: 1, borderColor: "primary.main" }}
    >
      <Box
        sx={{
          display: "flex",
          alignItems: "center",
          gap: 1,
          mb: 1,
        }}
      >
        <Typography
          variant="body2"
          sx={{
            flex: 1,
            minWidth: 0,
            ml: 2,
            wordBreak: "break-all",
            color: "text.secondary",
          }}
        >
          {parentDir}
        </Typography>
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
        sx={{ display: "flex", minHeight: 200, position: "relative" }}
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
              const isWaiting = entry?.status === QueueItemStatus.Waiting;
              const statusText = entry
                ? t(`queue.status.${entry.status.toLowerCase()}`)
                : "";
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
                  <Typography
                    variant="body2"
                    sx={{ wordBreak: "break-all", width: "100%" }}
                  >
                    {getFileName(file)}
                  </Typography>
                  {isExtracting ? (
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
                  ) : isWaiting ? (
                    <Box
                      sx={{
                        display: "flex",
                        alignItems: "center",
                        gap: 0.5,
                        width: "100%",
                      }}
                    >
                      <Typography
                        variant="caption"
                        sx={{ flex: 1, color: "text.secondary" }}
                      >
                        {statusText}
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
                  ) : entry ? (
                    <Typography
                      variant="caption"
                      sx={{ color: statusColor(entry.status) }}
                    >
                      {statusText}
                    </Typography>
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
          {tracks.length === 0 ? (
            <Typography variant="body2" color="text.secondary" sx={{ p: 1 }}>
              {t("extract.noTracks")}
            </Typography>
          ) : (
            <TableContainer>
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell padding="checkbox">
                      <Checkbox
                        size="small"
                        disabled={hasActiveInGroup}
                        checked={
                          tracks.length > 0 &&
                          selectedIds.size === tracks.length
                        }
                        indeterminate={
                          selectedIds.size > 0 &&
                          selectedIds.size < tracks.length
                        }
                        onChange={(e) => toggleAll(e.target.checked)}
                      />
                    </TableCell>
                    <TableCell>{t("extract.header.id")}</TableCell>
                    <TableCell>{t("extract.header.number")}</TableCell>
                    <TableCell>{t("extract.header.type")}</TableCell>
                    <TableCell>{t("extract.header.codec")}</TableCell>
                    <TableCell>{t("extract.header.trackName")}</TableCell>
                    <TableCell>{t("extract.header.language")}</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {tracks.map((track) => (
                    <TableRow key={track.id}>
                      <TableCell padding="checkbox">
                        <Checkbox
                          size="small"
                          disabled={hasActiveInGroup}
                          checked={selectedIds.has(track.id)}
                          onChange={(e) =>
                            toggleOne(track.id, e.target.checked)
                          }
                        />
                      </TableCell>
                      <TableCell>{track.id}</TableCell>
                      <TableCell>{track.number}</TableCell>
                      <TableCell>
                        <TrackTypeIcon type={track.type} />
                      </TableCell>
                      <TableCell>{track.codec}</TableCell>
                      <TableCell>{track.trackName}</TableCell>
                      <TableCell>{track.language}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          )}
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
    </Paper>
  );
}
