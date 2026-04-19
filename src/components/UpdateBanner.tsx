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

import { useEffect, useRef, useState } from "react";
import {
  Alert,
  Box,
  Checkbox,
  FormControlLabel,
  Link,
  Typography,
} from "@mui/material";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import { getUpdateResult, skipVersion } from "../service";

const RELEASES_URL = "https://github.com/caoccao/BatchMkvExtract/releases";
const POLL_INTERVAL_MS = 1000;

export function UpdateBanner() {
  const { t } = useTranslation();
  const [newVersion, setNewVersion] = useState<string | null>(null);
  const [skipChecked, setSkipChecked] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | undefined>(undefined);

  useEffect(() => {
    pollRef.current = setInterval(async () => {
      try {
        const result = await getUpdateResult();
        if (!result) {
          return;
        }
        if (pollRef.current) {
          clearInterval(pollRef.current);
          pollRef.current = undefined;
        }
        if (result.hasUpdate && result.latestVersion) {
          setNewVersion(result.latestVersion);
        }
      } catch {
        // ignore transient errors
      }
    }, POLL_INTERVAL_MS);
    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = undefined;
      }
    };
  }, []);

  if (!newVersion) {
    return null;
  }

  const handleClose = async () => {
    if (skipChecked) {
      try {
        await skipVersion(newVersion);
      } catch {
        // ignore
      }
    }
    setNewVersion(null);
    setSkipChecked(false);
  };

  return (
    <Alert
      severity="info"
      onClose={handleClose}
      sx={{ flexShrink: 0, "& .MuiAlert-message": { flex: 1 } }}
    >
      <Box sx={{ display: "flex", alignItems: "center", gap: 1 }}>
        <Link
          component="button"
          variant="body2"
          onClick={() => openUrl(RELEASES_URL)}
          sx={{ cursor: "pointer" }}
        >
          {t("update.newVersionAvailable", { version: newVersion })}
        </Link>
        <Box sx={{ flex: 1 }} />
        <FormControlLabel
          control={
            <Checkbox
              size="small"
              sx={{ p: 0.5 }}
              checked={skipChecked}
              onChange={(e) => setSkipChecked(e.target.checked)}
            />
          }
          label={
            <Typography variant="body2">
              {t("update.skipThisVersion")}
            </Typography>
          }
          sx={{ mr: 0 }}
        />
      </Box>
    </Alert>
  );
}
