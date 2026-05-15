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
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Stack,
  TextField,
} from "@mui/material";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import { checkOutputPathWritable } from "../service";

interface Props {
  open: boolean;
  initialValue: string;
  onConfirm: (value: string) => void;
  onClose: () => void;
}

export function OutputPathDialog({
  open: dialogOpen,
  initialValue,
  onConfirm,
  onClose,
}: Props) {
  const { t } = useTranslation();
  const [value, setValue] = useState(initialValue);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (dialogOpen) {
      setValue(initialValue);
      setError(null);
    }
  }, [dialogOpen, initialValue]);

  const handleBrowse = async () => {
    try {
      const directory = await open({
        directory: true,
        defaultPath: value.trim() || undefined,
      });
      if (typeof directory === "string" && directory.length > 0) {
        setValue(directory);
        setError(null);
      }
    } catch (err) {
      setError(String(err));
    }
  };

  const handleConfirm = async () => {
    const trimmed = value.trim();
    if (trimmed.length === 0) {
      onConfirm("");
      onClose();
      return;
    }
    try {
      const ok = await checkOutputPathWritable(trimmed);
      if (!ok) {
        setError(t("extract.outputPathNotWritable"));
        return;
      }
    } catch (err) {
      setError(String(err));
      return;
    }
    onConfirm(trimmed);
    onClose();
  };

  return (
    <Dialog
      open={dialogOpen}
      onClose={onClose}
      onKeyDown={(e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          handleConfirm();
        }
      }}
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
            value={value}
            onChange={(e) => {
              setValue(e.target.value);
              setError(null);
            }}
          />
          <Button
            variant="outlined"
            size="small"
            onClick={handleBrowse}
            sx={{ whiteSpace: "nowrap", textTransform: "none" }}
          >
            {t("extract.browse")}
          </Button>
        </Stack>
        {error ? (
          <Alert severity="error" sx={{ mt: 2 }}>
            {error}
          </Alert>
        ) : null}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose} sx={{ textTransform: "none" }}>
          {t("extract.cancel")}
        </Button>
        <Button
          onClick={handleConfirm}
          variant="contained"
          sx={{ textTransform: "none" }}
        >
          {t("extract.ok")}
        </Button>
      </DialogActions>
    </Dialog>
  );
}
