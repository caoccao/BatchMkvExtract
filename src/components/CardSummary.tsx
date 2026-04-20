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

import { Typography } from "@mui/material";
import { useTranslation } from "react-i18next";
import type { TrackCounts } from "../store";

interface Props {
  counts?: TrackCounts;
  outputPath?: string;
}

export function CardSummary({ counts, outputPath }: Props) {
  const { t } = useTranslation();
  const pieces: string[] = [];
  if (counts) {
    const parts: string[] = [];
    if (counts.video > 0) {
      parts.push(`${counts.video} 🎞️`);
    }
    if (counts.audio > 0) {
      parts.push(`${counts.audio} 🔊`);
    }
    if (counts.subtitles > 0) {
      parts.push(`${counts.subtitles} 💬`);
    }
    if (counts.chapters > 0) {
      parts.push(`${counts.chapters} 📑`);
    }
    if (counts.attachments > 0) {
      parts.push(`${counts.attachments} 📎`);
    }
    if (parts.length > 0) {
      pieces.push(`${parts.join(" ")}`);
    }
  }
  if (outputPath) {
    pieces.push(t("card.summary.toPath", { path: outputPath }));
  }
  if (pieces.length === 0) {
    return null;
  }
  return (
    <Typography
      variant="caption"
      sx={{
        display: "block",
        color: "text.secondary",
        wordBreak: "break-all",
      }}
    >
      {pieces.join(" ")}
    </Typography>
  );
}
