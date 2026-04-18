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
  Card,
  CardContent,
  CardHeader,
  IconButton,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  Tooltip,
} from "@mui/material";
import DeleteSweepIcon from "@mui/icons-material/DeleteSweep";
import { useTranslation } from "react-i18next";
import { formatHMS } from "../extract-utils";
import type { QueueItem, QueueItemStatus } from "../store";
import { useMkvStore } from "../store";

const TICK_INTERVAL_MS = 200;

function statusColor(status: QueueItemStatus): string {
  switch (status) {
    case "extracting":
      return "success.main";
    case "completed":
      return "text.secondary";
    case "queued":
    default:
      return "text.primary";
  }
}

function elapsed(item: QueueItem, now: number): string {
  if (item.status === "queued" || item.extractionStartedAt === null) {
    return "--:--:--";
  }
  const end = item.extractionEndedAt ?? now;
  return formatHMS(end - item.extractionStartedAt);
}

function eta(item: QueueItem, now: number): string {
  if (
    item.status !== "extracting" ||
    item.extractionStartedAt === null ||
    item.progress <= 0 ||
    item.progress >= 100
  ) {
    return "--:--:--";
  }
  const elapsedMs = now - item.extractionStartedAt;
  const etaMs = (elapsedMs * (100 - item.progress)) / item.progress;
  return formatHMS(etaMs);
}

function formatClockTime(ms: number | null): string {
  if (ms === null) return "--:--:--";
  const d = new Date(ms);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export default function Queue() {
  const { t } = useTranslation();
  const queueItems = useMkvStore((s) => s.queueItems);
  const queueOrder = useMkvStore((s) => s.queueOrder);
  const clearCompletedInDrive = useMkvStore((s) => s.clearCompletedInDrive);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), TICK_INTERVAL_MS);
    return () => clearInterval(id);
  }, []);

  const groups = useMemo(() => {
    const byDrive = new Map<string, QueueItem[]>();
    for (const file of queueOrder) {
      const item = queueItems[file];
      if (!item) continue;
      const list = byDrive.get(item.drive) ?? [];
      list.push(item);
      byDrive.set(item.drive, list);
    }
    return Array.from(byDrive.entries());
  }, [queueItems, queueOrder]);

  const statusLabel = (status: QueueItemStatus) => t(`queue.status.${status}`);

  return (
    <Stack spacing={2} sx={{ p: 1 }}>
      {groups.map(([drive, items]) => {
        const hasCompleted = items.some((i) => i.status === "completed");
        return (
        <Card variant="outlined" key={drive}>
          <CardHeader
            action={
              <Tooltip title={t("queue.clearCompleted")}>
                <span>
                  <IconButton
                    size="small"
                    disabled={!hasCompleted}
                    onClick={() => clearCompletedInDrive(drive)}
                  >
                    <DeleteSweepIcon fontSize="small" />
                  </IconButton>
                </span>
              </Tooltip>
            }
            sx={{ pb: 0 }}
          />
          <CardContent sx={{ pt: 0, "&.MuiCardContent-root:last-child": { pb: 2 } }}>
            <TableContainer>
              <Table size="small">
                <TableHead>
                  <TableRow>
                    <TableCell>{t("queue.header.filePath")}</TableCell>
                    <TableCell>{t("queue.header.status")}</TableCell>
                    <TableCell>{t("queue.header.start")}</TableCell>
                    <TableCell>{t("queue.header.end")}</TableCell>
                    <TableCell>{t("queue.header.elapsed")}</TableCell>
                    <TableCell>{t("queue.header.eta")}</TableCell>
                  </TableRow>
                </TableHead>
                <TableBody>
                  {items.map((item) => (
                    <TableRow key={item.file}>
                      <TableCell sx={{ wordBreak: "break-all" }}>
                        {item.file}
                      </TableCell>
                      <TableCell sx={{ color: statusColor(item.status) }}>
                        {statusLabel(item.status)}
                        {item.status === "extracting"
                          ? ` ${item.progress}%`
                          : ""}
                      </TableCell>
                      <TableCell>
                        {formatClockTime(item.extractionStartedAt)}
                      </TableCell>
                      <TableCell>
                        {formatClockTime(item.extractionEndedAt)}
                      </TableCell>
                      <TableCell>{elapsed(item, now)}</TableCell>
                      <TableCell>{eta(item, now)}</TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </TableContainer>
          </CardContent>
        </Card>
        );
      })}
    </Stack>
  );
}
