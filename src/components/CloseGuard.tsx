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
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
} from "@mui/material";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useTranslation } from "react-i18next";
import { QueueItemStatus } from "../protocol";
import { cancelExtract } from "../service";
import { useMkvStore } from "../store";

export function CloseGuard() {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWebviewWindow();
    const unlistenPromise = appWindow.onCloseRequested((event) => {
      const items = Object.values(useMkvStore.getState().queueItems);
      const hasActive = items.some(
        (i) =>
          i.status === QueueItemStatus.Waiting ||
          i.status === QueueItemStatus.Extracting,
      );
      if (hasActive) {
        event.preventDefault();
        setOpen(true);
      }
    });
    return () => {
      unlistenPromise.then((fn) => fn());
    };
  }, []);

  const handleNo = () => {
    setOpen(false);
  };

  const handleYes = async () => {
    const state = useMkvStore.getState();
    const activeFiles = Object.values(state.queueItems)
      .filter(
        (i) =>
          i.status === QueueItemStatus.Waiting ||
          i.status === QueueItemStatus.Extracting,
      )
      .map((i) => i.file);
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
    setOpen(false);
    try {
      await getCurrentWebviewWindow().destroy();
    } catch (err) {
      console.error("Failed to destroy window", err);
    }
  };

  return (
    <Dialog open={open} onClose={handleNo}>
      <DialogTitle>{t("confirm.exitTitle")}</DialogTitle>
      <DialogContent>
        <DialogContentText>{t("confirm.exitMessage")}</DialogContentText>
      </DialogContent>
      <DialogActions>
        <Button
          onClick={handleNo}
          variant="contained"
          color="success"
          autoFocus
        >
          {t("confirm.no")}
        </Button>
        <Button onClick={handleYes} variant="contained" color="error">
          {t("confirm.yes")}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
