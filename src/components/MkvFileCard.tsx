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

import { useEffect, useState } from "react";
import {
  Alert,
  Box,
  Button,
  Card,
  CardContent,
  CardHeader,
  Checkbox,
  CircularProgress,
  IconButton,
  LinearProgress,
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
import { basename, dirname, extname, join, sep as getSep } from "@tauri-apps/api/path";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useTranslation } from "react-i18next";
import type { MkvTrack } from "../protocol";
import { cancelExtract, enqueueExtract, getMkvTracks } from "../service";
import { useMkvStore } from "../store";

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

function getTrackExtension(codecId: string, trackType: string): string {
  if (codecId.startsWith("V_")) {
    switch (codecId) {
      case "V_MPEGH/ISO/HEVC":
        return "h265";
      case "V_MPEG4/ISO/AVC":
        return "h264";
      case "V_MPEG1":
      case "V_MPEG2":
        return "mpg";
      case "V_MPEG4/ISO/SP":
      case "V_MPEG4/ISO/ASP":
      case "V_MPEG4/ISO/AP":
      case "V_MPEG4/MS/V3":
        return "mpeg4";
      case "V_MS/VFW/FOURCC":
        return "avi";
      case "V_VP8":
      case "V_VP9":
      case "V_AV1":
        return "ivf";
      case "V_THEORA":
        return "ogg";
      case "V_PRORES":
        return "prores";
      case "V_FFV1":
        return "ffv1";
    }
    if (codecId.startsWith("V_REAL/")) return "rm";
    return "bin";
  }
  if (codecId.startsWith("A_")) {
    switch (codecId) {
      case "A_AC3":
      case "A_AC3/BSID9":
      case "A_AC3/BSID10":
        return "ac3";
      case "A_EAC3":
        return "eac3";
      case "A_TRUEHD":
        return "thd";
      case "A_MLP":
        return "mlp";
      case "A_MPEG/L1":
        return "mp1";
      case "A_MPEG/L2":
        return "mp2";
      case "A_MPEG/L3":
        return "mp3";
      case "A_FLAC":
        return "flac";
      case "A_VORBIS":
        return "ogg";
      case "A_OPUS":
        return "opus";
      case "A_WAVPACK4":
        return "wv";
      case "A_TTA1":
        return "tta";
      case "A_ALAC":
        return "caf";
      default:
        if (codecId.startsWith("A_PCM/")) return "wav";
        if (codecId.startsWith("A_AAC")) return "aac";
        if (codecId.startsWith("A_DTS")) return "dts";
        if (codecId.startsWith("A_REAL/")) return "rm";
        return "bin";
    }
  }
  if (codecId.startsWith("S_")) {
    switch (codecId) {
      case "S_TEXT/UTF8":
      case "S_TEXT/ASCII":
        return "srt";
      case "S_TEXT/ASS":
      case "S_ASS":
        return "ass";
      case "S_TEXT/SSA":
      case "S_SSA":
        return "ssa";
      case "S_TEXT/WEBVTT":
        return "vtt";
      case "S_TEXT/USF":
        return "usf";
      case "S_VOBSUB":
        return "sub";
      case "S_HDMV/PGS":
        return "sup";
      case "S_HDMV/TEXTST":
        return "textst";
      case "S_KATE":
        return "ogg";
      default:
        return "bin";
    }
  }
  switch (trackType) {
    case "video":
    case "audio":
      return "bin";
    case "subtitles":
      return "srt";
    default:
      return "bin";
  }
}

async function getFileNameWithoutExt(filePath: string): Promise<string> {
  const ext = await extname(filePath);
  return await basename(filePath, ext ? `.${ext}` : undefined);
}

function buildOutputFileName(
  fileNameWithoutExt: string,
  track: MkvTrack,
): string {
  const ext = getTrackExtension(track.codecId, track.type);
  return `${fileNameWithoutExt}_${track.number}_${track.language}.${ext}`;
}

async function buildExtractArgs(
  file: string,
  outputDir: string,
  tracks: MkvTrack[],
): Promise<string[]> {
  const fileNameWithoutExt = await getFileNameWithoutExt(file);
  const results: string[] = [];
  for (const track of tracks) {
    const outFile = await join(
      outputDir,
      buildOutputFileName(fileNameWithoutExt, track),
    );
    results.push(`${track.id}:${outFile}`);
  }
  return results;
}

async function buildCommandString(
  file: string,
  outputDir: string,
  mkvToolNixPath: string,
  tracks: MkvTrack[],
): Promise<string> {
  const sep = getSep();
  const mkvextractPath = `${mkvToolNixPath}${sep}mkvextract`;
  const fileNameWithoutExt = await getFileNameWithoutExt(file);
  const args: string[] = [];
  for (const track of tracks) {
    const outFile = await join(
      outputDir,
      buildOutputFileName(fileNameWithoutExt, track),
    );
    args.push(`${track.id}:"${outFile}"`);
  }
  return `"${mkvextractPath}" "${file}" tracks ${args.join(" ")}`;
}

export function MkvFileCard({ path }: MkvFileCardProps) {
  const { t } = useTranslation();
  const removeFile = useMkvStore((s) => s.removeFile);
  const mkvToolNixPath = useMkvStore(
    (s) => s.config?.mkv?.mkvToolNixPath ?? "",
  );
  const entry = useMkvStore((s) => s.queueItems[path]);
  const addToQueue = useMkvStore((s) => s.addToQueue);
  const registerExtractHandler = useMkvStore((s) => s.registerExtractHandler);
  const unregisterExtractHandler = useMkvStore(
    (s) => s.unregisterExtractHandler,
  );
  const setFileHasSelection = useMkvStore((s) => s.setFileHasSelection);

  const isExtracting = entry?.status === "extracting";
  const isQueued = entry?.status === "queued";
  const isActive = isExtracting || isQueued;

  const [tracks, setTracks] = useState<MkvTrack[]>([]);
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [snackbar, setSnackbar] = useState<{
    message: string;
    severity: "success" | "error";
  } | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    getMkvTracks(path)
      .then((result) => {
        if (cancelled) return;
        setTracks(result);
        setLoading(false);
      })
      .catch((err) => {
        if (cancelled) return;
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
  }, [path, t]);

  const selectedTracks = tracks.filter((track) => selectedIds.has(track.id));
  const hasSelection = selectedTracks.length > 0;

  const toggleAll = (checked: boolean) => {
    setSelectedIds(checked ? new Set(tracks.map((t) => t.id)) : new Set());
  };

  const toggleOne = (id: number, checked: boolean) => {
    setSelectedIds((prev) => {
      const next = new Set(prev);
      if (checked) {
        next.add(id);
      } else {
        next.delete(id);
      }
      return next;
    });
  };

  const handleCopyCommand = async () => {
    if (!hasSelection) return;
    try {
      const outputDir = await dirname(path);
      const command = await buildCommandString(
        path,
        outputDir,
        mkvToolNixPath,
        selectedTracks,
      );
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
    if (!hasSelection || isActive) return;
    try {
      const outputDir = await dirname(path);
      const args = await buildExtractArgs(path, outputDir, selectedTracks);
      await enqueueExtract(path, args);
      addToQueue(path);
    } catch (err) {
      setSnackbar({ message: String(err), severity: "error" });
    }
  };

  const handleCancel = async () => {
    try {
      await cancelExtract(path);
    } catch (err) {
      setSnackbar({ message: String(err), severity: "error" });
    }
  };

  useEffect(() => {
    registerExtractHandler(path, handleExtract);
    return () => {
      unregisterExtractHandler(path);
    };
  }, [path, handleExtract, registerExtractHandler, unregisterExtractHandler]);

  useEffect(() => {
    setFileHasSelection(path, hasSelection && !isActive);
    return () => {
      setFileHasSelection(path, false);
    };
  }, [path, hasSelection, isActive, setFileHasSelection]);

  const titleContent = isExtracting ? (
    <Box
      sx={{
        position: "relative",
        display: "flex",
        alignItems: "center",
        minHeight: 32,
        borderRadius: 1,
        overflow: "hidden",
      }}
    >
      <LinearProgress
        variant="determinate"
        value={entry?.progress ?? 0}
        sx={{
          position: "absolute",
          top: 0,
          left: 0,
          right: 0,
          bottom: 0,
          width: "100%",
          height: "100%",
          bgcolor: "action.hover",
          "& .MuiLinearProgress-bar": {
            bgcolor: "success.main",
          },
        }}
      />
      <Typography
        variant="body2"
        sx={{
          position: "relative",
          zIndex: 1,
          px: 1,
          wordBreak: "break-all",
          width: "100%",
        }}
      >
        {path}
      </Typography>
    </Box>
  ) : (
    <Typography variant="body2" sx={{ wordBreak: "break-all" }}>
      {path}
    </Typography>
  );

  const actionContent = isActive ? (
    <Tooltip title={t("extract.cancel")}>
      <IconButton size="small" color="error" onClick={handleCancel}>
        <CancelIcon fontSize="small" />
      </IconButton>
    </Tooltip>
  ) : (
    <Box sx={{ display: "flex", gap: 0.5 }}>
      <Tooltip title={t("extract.copyCommand")}>
        <span>
          <IconButton
            size="small"
            disabled={!hasSelection}
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
        disabled={!hasSelection}
        onClick={handleExtract}
        sx={{ textTransform: "none", whiteSpace: "nowrap" }}
      >
        {t("extract.extract")}
      </Button>
      <Tooltip title={t("list.delete")}>
        <IconButton
          size="small"
          color="error"
          onClick={() => removeFile(path)}
        >
          <DeleteIcon fontSize="small" />
        </IconButton>
      </Tooltip>
    </Box>
  );

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
        action={actionContent}
        sx={{
          pb: 1,
          "& .MuiCardHeader-content": { minWidth: 0, flex: 1 },
        }}
      />
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
    </Card>
  );
}
