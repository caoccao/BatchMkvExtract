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
import { Box, Typography } from "@mui/material";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useTranslation } from "react-i18next";
import { getParentDir } from "../extract-utils";
import type { ExtractionFinishedEvent } from "../protocol";
import { getExtractStatus } from "../service";
import { useMkvStore } from "../store";
import { GroupCard } from "./GroupCard";
import { MkvFileCard } from "./MkvFileCard";

type RenderEntry =
  | { kind: "single"; file: string }
  | { kind: "group"; key: string; files: string[] };

const EXTRACT_POLL_INTERVAL_MS = 200;

export default function FileList() {
  const { t } = useTranslation();
  const files = useMkvStore((s) => s.files);
  const addFiles = useMkvStore((s) => s.addFiles);
  const applyExtractSnapshot = useMkvStore((s) => s.applyExtractSnapshot);
  const recordFinishedOutcome = useMkvStore((s) => s.recordFinishedOutcome);
  const groupByFile = useMkvStore((s) => s.groupByFile);
  const fileTrackCounts = useMkvStore((s) => s.fileTrackCounts);

  const entries = useMemo<RenderEntry[]>(() => {
    if (!groupByFile) {
      return files.map((file) => ({ kind: "single", file }));
    }
    const buckets = new Map<string, string[]>();
    const bucketOrder: string[] = [];
    const ungroupable: string[] = [];
    for (const file of files) {
      const counts = fileTrackCounts[file];
      if (!counts) {
        ungroupable.push(file);
        continue;
      }
      const key = `${getParentDir(file)}|v=${counts.video}|a=${counts.audio}|s=${counts.subtitles}`;
      let bucket = buckets.get(key);
      if (!bucket) {
        bucket = [];
        buckets.set(key, bucket);
        bucketOrder.push(key);
      }
      bucket.push(file);
    }
    const result: RenderEntry[] = [];
    for (const key of bucketOrder) {
      const groupFiles = buckets.get(key) ?? [];
      if (groupFiles.length >= 2) {
        result.push({ kind: "group", key, files: groupFiles });
      } else {
        for (const file of groupFiles) {
          result.push({ kind: "single", file });
        }
      }
    }
    for (const file of ungroupable) {
      result.push({ kind: "single", file });
    }
    return result;
  }, [files, groupByFile, fileTrackCounts]);

  useEffect(() => {
    let cancelled = false;
    const poll = async () => {
      try {
        const snap = await getExtractStatus();
        if (!cancelled) {
          applyExtractSnapshot(snap.entries);
        }
      } catch (err) {
        if (!cancelled) {
          console.error("Failed to fetch extract status", err);
        }
      }
    };
    poll();
    const id = setInterval(poll, EXTRACT_POLL_INTERVAL_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [applyExtractSnapshot]);

  useEffect(() => {
    const unlistenPromise = listen<ExtractionFinishedEvent>(
      "extraction-finished",
      (event) => {
        const { file, outcome, error } = event.payload;
        recordFinishedOutcome(file, outcome, error);
      },
    );
    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, [recordFinishedOutcome]);

  useEffect(() => {
    const unlistenPromise = getCurrentWebviewWindow().onDragDropEvent(
      async (event) => {
        if (event.payload.type !== "drop") {
          return;
        }
        const paths = event.payload.paths;
        if (!paths || paths.length === 0) {
          return;
        }
        try {
          const mkvFiles = await invoke<string[]>("get_mkv_files", { paths });
          if (mkvFiles.length > 0) {
            addFiles(mkvFiles);
          }
        } catch (err) {
          console.error("Failed to resolve dropped paths", err);
        }
      },
    );
    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, [addFiles]);

  if (files.length === 0) {
    return (
      <Box
        sx={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "100%",
          color: "text.secondary",
          border: "2px dashed",
          borderColor: "divider",
          borderRadius: 2,
        }}
      >
        <Typography variant="body1">{t("app.dropHere")}</Typography>
      </Box>
    );
  }

  return (
    <Box>
      {entries.map((entry) =>
        entry.kind === "single" ? (
          <MkvFileCard key={entry.file} path={entry.file} />
        ) : (
          <GroupCard key={entry.key} files={entry.files} />
        ),
      )}
    </Box>
  );
}
