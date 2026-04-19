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

import { useEffect, useMemo, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  CardHeader,
  Checkbox,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  IconButton,
  LinearProgress,
  Snackbar,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
  Tooltip,
  Typography,
} from "@mui/material";
import CancelIcon from "@mui/icons-material/Cancel";
import ClosedCaptionIcon from "@mui/icons-material/ClosedCaption";
import ContentCopyIcon from "@mui/icons-material/ContentCopy";
import ContentCutIcon from "@mui/icons-material/ContentCut";
import DeleteIcon from "@mui/icons-material/Delete";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import HelpOutlineOutlinedIcon from "@mui/icons-material/HelpOutlineOutlined";
import ImageIcon from "@mui/icons-material/Image";
import MusicNoteIcon from "@mui/icons-material/MusicNote";
import SmartButtonIcon from "@mui/icons-material/SmartButton";
import VideocamIcon from "@mui/icons-material/Videocam";
import { dirname } from "@tauri-apps/api/path";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  buildCommandString,
  buildExtractArgs,
  formatHMS,
  shouldSelectTrackType,
} from "../extract-utils";
import { QueueItemStatus } from "../protocol";
import {
  cancelExtract,
  checkOutputPathWritable,
  ensureOutputPath,
  enqueueExtract,
  getMkvTracks,
} from "../service";
import { useMkvStore } from "../store";
import { FileStatusIcon } from "./FileStatusIcon";

interface MkvFileCardProps {
  path: string;
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

export function MkvFileCard({ path }: MkvFileCardProps) {
  const { t } = useTranslation();
  const removeFile = useMkvStore((s) => s.removeFile);
  const mkvToolNixPath = useMkvStore(
    (s) => s.config?.mkv?.mkvToolNixPath ?? "",
  );
  const entry = useMkvStore((s) => s.queueItems[path]);
  const addToQueue = useMkvStore((s) => s.addToQueue);
  const markCancelRequested = useMkvStore((s) => s.markCancelRequested);
  const setFileTracks = useMkvStore((s) => s.setFileTracks);
  const setFileTrackCounts = useMkvStore((s) => s.setFileTrackCounts);
  const setFileSelectedIds = useMkvStore((s) => s.setFileSelectedIds);
  const setFileOutputDir = useMkvStore((s) => s.setFileOutputDir);
  const clearFileOutputDir = useMkvStore((s) => s.clearFileOutputDir);
  const cachedTracks = useMkvStore((s) => s.fileTracks[path]);
  const storedSelectedIds = useMkvStore((s) => s.fileSelectedIds[path]);
  const outputDirOverride = useMkvStore((s) => s.fileOutputDirs[path]);
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

  const isExtracting = entry?.status === QueueItemStatus.Extracting;
  const isQueued = entry?.status === QueueItemStatus.Waiting;
  const isActive = isExtracting || isQueued;

  const [loading, setLoading] = useState<boolean>(
    () => cachedTracks === undefined,
  );
  const [error, setError] = useState<string | null>(null);
  const tracks = cachedTracks ?? [];
  const selectedIds = useMemo(
    () => new Set<number>(storedSelectedIds ?? []),
    [storedSelectedIds],
  );
  const [snackbar, setSnackbar] = useState<{
    message: string;
    severity: "success" | "error";
  } | null>(null);
  const [outputDialogOpen, setOutputDialogOpen] = useState(false);
  const [outputDialogValue, setOutputDialogValue] = useState("");
  const [outputDialogError, setOutputDialogError] = useState<string | null>(
    null,
  );

  useEffect(() => {
    if (storedSelectedIds !== undefined) {
      return;
    }
    if (tracks.length === 0 || !activeProfile) {
      return;
    }
    const auto: number[] = [];
    for (const track of tracks) {
      if (shouldSelectTrackType(activeProfile, track.type)) {
        auto.push(track.id);
      }
    }
    setFileSelectedIds(path, auto);
  }, [path, tracks, activeProfile, storedSelectedIds, setFileSelectedIds]);

  useEffect(() => {
    if (useMkvStore.getState().fileTracks[path] !== undefined) {
      setLoading(false);
      setError(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    getMkvTracks(path)
      .then((result) => {
        if (cancelled) {
          return;
        }
        setFileTracks(path, result);
        setLoading(false);
        let video = 0;
        let audio = 0;
        let subtitles = 0;
        for (const track of result) {
          if (track.type === "video") {
            video += 1;
          } else if (track.type === "audio") {
            audio += 1;
          } else if (track.type === "subtitles") {
            subtitles += 1;
          }
        }
        setFileTrackCounts(path, { video, audio, subtitles });
      })
      .catch((err) => {
        if (cancelled) {
          return;
        }
        const msg = String(err);
        if (msg.includes("MKVMERGE_NOT_AVAILABLE:")) {
          setError(
            t("extract.error.mkvmergeNotAvailable", {
              detail: msg.split("MKVMERGE_NOT_AVAILABLE:")[1],
            }),
          );
        } else if (msg.includes("MKVMERGE_FAILED:")) {
          setError(
            t("extract.error.mkvmergeFailed", {
              detail: msg.split("MKVMERGE_FAILED:")[1],
            }),
          );
        } else if (msg.includes("MKVMERGE_PARSE_ERROR:")) {
          setError(
            t("extract.error.parseError", {
              detail: msg.split("MKVMERGE_PARSE_ERROR:")[1],
            }),
          );
        } else {
          setError(msg);
        }
        setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [path, t, setFileTracks, setFileTrackCounts]);

  const selectedTracks = tracks.filter((track) => selectedIds.has(track.id));
  const hasSelection = selectedTracks.length > 0;

  const toggleAll = (checked: boolean) => {
    setFileSelectedIds(path, checked ? tracks.map((t) => t.id) : []);
  };

  const toggleOne = (id: number, checked: boolean) => {
    const current = storedSelectedIds ?? [];
    const next = checked
      ? [...current, id]
      : current.filter((v) => v !== id);
    setFileSelectedIds(path, next);
  };

  const resolveOutputDir = async (): Promise<string> => {
    if (outputDirOverride && outputDirOverride.length > 0) {
      return outputDirOverride;
    }
    return await dirname(path);
  };

  const buildCurrentCommand = async (): Promise<string | null> => {
    if (!hasSelection || !activeProfile) {
      return null;
    }
    const outputDir = await resolveOutputDir();
    return await buildCommandString(
      path,
      outputDir,
      mkvToolNixPath,
      selectedTracks,
      activeProfile,
    );
  };

  const handleCopyCommand = async () => {
    try {
      const command = await buildCurrentCommand();
      if (!command) {
        return;
      }
      await writeText(command);
      setSnackbar({
        message: t("extract.commandCopied"),
        severity: "success",
      });
    } catch (err) {
      setSnackbar({ message: String(err), severity: "error" });
    }
  };

  const handleExtract = async () => {
    if (!hasSelection || isActive || !activeProfile) {
      return;
    }
    try {
      const outputDir = await resolveOutputDir();
      try {
        await ensureOutputPath(outputDir);
      } catch {
        useMkvStore
          .getState()
          .showNotification(
            "error",
            path,
            t("notification.failedCreateOutput", { path: outputDir }),
          );
        return;
      }
      const args = await buildExtractArgs(
        path,
        outputDir,
        selectedTracks,
        activeProfile,
      );
      await enqueueExtract(path, args);
      addToQueue(path);
    } catch (err) {
      setSnackbar({ message: String(err), severity: "error" });
    }
  };

  const handleCancel = async () => {
    markCancelRequested(path);
    try {
      await cancelExtract(path);
    } catch (err) {
      setSnackbar({ message: String(err), severity: "error" });
    }
  };

  const handleOpenOutputDialog = async () => {
    setOutputDialogError(null);
    if (outputDirOverride && outputDirOverride.length > 0) {
      setOutputDialogValue(outputDirOverride);
    } else {
      try {
        setOutputDialogValue(await dirname(path));
      } catch {
        setOutputDialogValue("");
      }
    }
    setOutputDialogOpen(true);
  };

  const handleBrowseOutputDir = async () => {
    try {
      const directory = await open({
        directory: true,
        defaultPath: outputDialogValue.trim() || undefined,
      });
      if (typeof directory === "string" && directory.length > 0) {
        setOutputDialogValue(directory);
        setOutputDialogError(null);
      }
    } catch (err) {
      setSnackbar({ message: String(err), severity: "error" });
    }
  };

  const handleConfirmOutputDir = async () => {
    const trimmed = outputDialogValue.trim();
    if (trimmed.length === 0) {
      clearFileOutputDir(path);
      setOutputDialogOpen(false);
      return;
    }
    try {
      const ok = await checkOutputPathWritable(trimmed);
      if (!ok) {
        setOutputDialogError(t("extract.outputPathNotWritable"));
        return;
      }
    } catch (err) {
      setOutputDialogError(String(err));
      return;
    }
    setFileOutputDir(path, trimmed);
    setOutputDialogOpen(false);
  };

  const handleDelete = async () => {
    const current = useMkvStore.getState().queueItems[path];
    if (current?.status === QueueItemStatus.Extracting) {
      return;
    }
    if (current?.status === QueueItemStatus.Waiting) {
      try {
        await cancelExtract(path);
      } catch {
        // ignore; backend may already have dropped the task
      }
      const later = useMkvStore.getState().queueItems[path];
      if (later?.status === QueueItemStatus.Extracting) {
        return;
      }
      useMkvStore.getState().removeFromQueue(path);
    }
    removeFile(path);
  };


  const titleContent = (
    <Box sx={{ display: "flex", alignItems: "center", gap: 0.5 }}>
      <FileStatusIcon status={entry?.status} />
      <Typography variant="body2" sx={{ wordBreak: "break-all" }}>
        {path}
      </Typography>
    </Box>
  );

  const actionContent = (
    <Box sx={{ display: "flex", gap: 0.5 }}>
      <Tooltip title={t("extract.setOutputPath")}>
        <span>
          <IconButton
            size="small"
            disabled={isActive}
            onClick={handleOpenOutputDialog}
          >
            <FolderOpenIcon fontSize="small" />
          </IconButton>
        </span>
      </Tooltip>
      <Tooltip title={t("extract.copyCommand")}>
        <span>
          <IconButton
            size="small"
            disabled={!hasSelection || isActive}
            onClick={handleCopyCommand}
          >
            <ContentCopyIcon fontSize="small" />
          </IconButton>
        </span>
      </Tooltip>
      <Button
        variant="outlined"
        size="small"
        startIcon={<ContentCutIcon />}
        disabled={!hasSelection || isActive}
        onClick={handleExtract}
        sx={{ textTransform: "none", whiteSpace: "nowrap" }}
      >
        {t("extract.extract")}
      </Button>
      <Tooltip title={t("list.delete")}>
        <span>
          <IconButton
            size="small"
            color="error"
            disabled={isExtracting}
            onClick={handleDelete}
          >
            <DeleteIcon fontSize="small" />
          </IconButton>
        </span>
      </Tooltip>
    </Box>
  );

  const progress = entry?.progress ?? 0;
  const startedAt = entry?.extractionStartedAt ?? null;
  const elapsedMs =
    isExtracting && startedAt !== null ? Date.now() - startedAt : 0;
  const elapsedStr = isExtracting ? formatHMS(elapsedMs) : "--:--:--";
  const etaStr =
    isExtracting && progress > 0 && progress < 100
      ? formatHMS((elapsedMs * (100 - progress)) / progress)
      : "--:--:--";

  return (
    <Card
      variant="outlined"
      sx={{
        mt: 1,
        bgcolor: isQueued ? "action.hover" : undefined,
      }}
    >
      <CardHeader
        title={titleContent}
        subheader={
          outputDirOverride ? (
            <Typography
              variant="caption"
              sx={{ color: "text.secondary", wordBreak: "break-all" }}
            >
              {t("extract.outputPath")}: {outputDirOverride}
            </Typography>
          ) : undefined
        }
        action={actionContent}
        sx={{
          pb: isActive ? 0 : 1,
          "& .MuiCardHeader-content": { minWidth: 0, flex: 1 },
        }}
      />
      {isActive && (
        <Box
          sx={{
            display: "flex",
            alignItems: "center",
            gap: 1,
            px: 2,
            pb: 1,
            mt: 1,
          }}
        >
          {isExtracting ? (
            <>
              <LinearProgress
                variant="determinate"
                value={progress}
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
                  whiteSpace: "nowrap",
                }}
              >
                {elapsedStr} / {etaStr}
              </Typography>
            </>
          ) : (
            <Box sx={{ flex: 1 }} />
          )}
          <Tooltip title={t("extract.cancel")}>
            <IconButton size="small" color="error" onClick={handleCancel}>
              <CancelIcon fontSize="small" />
            </IconButton>
          </Tooltip>
        </Box>
      )}
      <CardContent sx={{ pt: 0, "&.MuiCardContent-root:last-child": { pb: 2 } }}>
        {loading ? (
          <Box sx={{ display: "flex", justifyContent: "center", py: 1 }}>
            <CircularProgress size={20} />
          </Box>
        ) : error ? (
          <Typography variant="body2" color="error">
            {error}
          </Typography>
        ) : tracks.length === 0 ? (
          <Typography variant="body2" color="text.secondary">
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
                      disabled={isActive}
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
                        disabled={isActive}
                        checked={selectedIds.has(track.id)}
                        onChange={(e) => toggleOne(track.id, e.target.checked)}
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
      </CardContent>
      <Snackbar
        open={snackbar !== null}
        autoHideDuration={5000}
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
      <Dialog
        open={outputDialogOpen}
        onClose={() => setOutputDialogOpen(false)}
        fullWidth
        maxWidth="sm"
      >
        <DialogTitle>{t("extract.setOutputPath")}</DialogTitle>
        <DialogContent>
          <Stack direction="row" spacing={1} sx={{ mt: 1 }}>
            <TextField
              fullWidth
              size="small"
              label={t("extract.outputPath")}
              value={outputDialogValue}
              onChange={(e) => {
                setOutputDialogValue(e.target.value);
                setOutputDialogError(null);
              }}
            />
            <Button
              variant="outlined"
              size="small"
              onClick={handleBrowseOutputDir}
              sx={{ whiteSpace: "nowrap" }}
            >
              {t("extract.browse")}
            </Button>
          </Stack>
          {outputDialogError ? (
            <Alert severity="error" sx={{ mt: 2 }}>
              {outputDialogError}
            </Alert>
          ) : null}
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setOutputDialogOpen(false)}>
            {t("extract.cancel")}
          </Button>
          <Button onClick={handleConfirmOutputDir} variant="contained">
            {t("extract.ok")}
          </Button>
        </DialogActions>
      </Dialog>
    </Card>
  );
}
