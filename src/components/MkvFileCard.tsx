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

import {
  Box,
  Card,
  CardHeader,
  IconButton,
  Tooltip,
  Typography,
} from "@mui/material";
import DeleteIcon from "@mui/icons-material/Delete";
import { useTranslation } from "react-i18next";
import { useMkvStore } from "../store";

interface MkvFileCardProps {
  path: string;
}

export function MkvFileCard({ path }: MkvFileCardProps) {
  const { t } = useTranslation();
  const removeFile = useMkvStore((s) => s.removeFile);

  return (
    <Card variant="outlined" sx={{ mt: 1 }}>
      <CardHeader
        title={
          <Typography variant="body2" sx={{ wordBreak: "break-all" }}>
            {path}
          </Typography>
        }
        action={
          <Box sx={{ display: "flex", gap: 0.5 }}>
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
        }
        sx={{ pb: 1 }}
      />
    </Card>
  );
}
