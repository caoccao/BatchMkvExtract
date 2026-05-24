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

import { Box, Button, TextField, Typography } from "@mui/material";

interface IntegrationPathRowProps {
  label: string;
  value: string;
  status: boolean | null;
  foundLabel: string;
  notFoundLabel: string;
  browseLabel: string;
  detectLabel: string;
  onChange: (value: string) => void;
  onBlur: () => void;
  onBrowse: () => void | Promise<void>;
  onDetect: () => void | Promise<void>;
}

export function IntegrationPathRow({
  label,
  value,
  status,
  foundLabel,
  notFoundLabel,
  browseLabel,
  detectLabel,
  onChange,
  onBlur,
  onBrowse,
  onDetect,
}: IntegrationPathRowProps) {
  return (
    <>
      <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
        {label}
      </Typography>
      <Box sx={{ display: "flex", alignItems: "center", gap: 1 }}>
        <TextField
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={onBlur}
          size="small"
          fullWidth
        />
        <Button
          variant="outlined"
          size="small"
          onClick={onBrowse}
          sx={{ minWidth: 90, height: 36, textTransform: "none" }}
        >
          {browseLabel}
        </Button>
        <Button
          variant="outlined"
          size="small"
          onClick={onDetect}
          sx={{ minWidth: 90, height: 36, textTransform: "none" }}
        >
          {detectLabel}
        </Button>
      </Box>
      {status !== null ? (
        <Typography
          variant="caption"
          sx={{
            mt: 0.75,
            display: "block",
            color: status ? "success.main" : "error.main",
          }}
        >
          {status ? foundLabel : notFoundLabel}
        </Typography>
      ) : null}
    </>
  );
}
