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

import { useEffect } from "react";
import { Box, Typography } from "@mui/material";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useTranslation } from "react-i18next";
import { getExtractStatus } from "../service";
import { useMkvStore } from "../store";
import { MkvFileCard } from "./MkvFileCard";

const EXTRACT_POLL_INTERVAL_MS = 200;

export default function FileList() {
  const { t } = useTranslation();
  const files = useMkvStore((s) => s.files);
  const addFiles = useMkvStore((s) => s.addFiles);
  const applyExtractSnapshot = useMkvStore((s) => s.applyExtractSnapshot);

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
      {files.map((path) => (
        <MkvFileCard key={path} path={path} />
      ))}
    </Box>
  );
}
