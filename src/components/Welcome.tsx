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

import { Box, Button, Link, Stack, Typography } from "@mui/material";
import ArticleIcon from "@mui/icons-material/Article";
import FolderIcon from "@mui/icons-material/Folder";
import GitHubIcon from "@mui/icons-material/GitHub";
import PersonIcon from "@mui/icons-material/Person";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useTranslation } from "react-i18next";
import appIconUrl from "../../src-tauri/icons/icon.png";
import batchMkvMergeIcon from "../assets/batchmkvmerge.png";
import bdMasterIcon from "../assets/bdmaster.png";
import betterMediaInfoIcon from "../assets/bettermediainfo.png";
import { getMkvFiles } from "../service";
import { useMkvStore } from "../store";

const AUTHOR_NAME = "Sam Cao";
const AUTHOR_URL = "https://github.com/caoccao/";
const GITHUB_URL = "https://github.com/caoccao/BatchMkvExtract";
const BATCH_MKV_MERGE_URL = "https://github.com/caoccao/BatchMkvMerge";
const BD_MASTER_URL = "https://github.com/caoccao/BDMaster";
const BETTER_MEDIA_INFO_URL = "https://github.com/caoccao/BetterMediaInfo";

interface AppCardProps {
  logo: string;
  title: string;
  intro: string;
  githubUrl: string;
  isPrimary?: boolean;
}

function AppCard({ logo, title, intro, githubUrl, isPrimary }: AppCardProps) {
  const { t } = useTranslation();
  return (
    <Box
      sx={(theme) => ({
        flex: 1,
        minWidth: 260,
        p: 3,
        borderRadius: 3,
        border: "1px solid",
        borderColor:
          theme.palette.mode === "dark"
            ? "rgba(96,165,250,0.35)"
            : "rgba(37,99,235,0.25)",
        background: isPrimary
          ? theme.palette.mode === "dark"
            ? "linear-gradient(140deg, rgba(37,99,235,0.32) 0%, rgba(14,165,233,0.18) 100%)"
            : "linear-gradient(140deg, rgba(59,130,246,0.16) 0%, rgba(14,165,233,0.10) 100%)"
          : theme.palette.mode === "dark"
            ? "linear-gradient(140deg, rgba(30,58,138,0.28) 0%, rgba(15,23,42,0.40) 100%)"
            : "linear-gradient(140deg, rgba(219,234,254,0.85) 0%, rgba(241,245,249,0.85) 100%)",
        boxShadow:
          theme.palette.mode === "dark"
            ? "0 10px 30px rgba(2,6,23,0.45)"
            : "0 10px 30px rgba(37,99,235,0.10)",
        display: "flex",
        flexDirection: "column",
        gap: 1.5,
        transition: "transform 160ms ease, box-shadow 160ms ease",
        "&:hover": {
          transform: "translateY(-2px)",
          boxShadow:
            theme.palette.mode === "dark"
              ? "0 14px 36px rgba(2,6,23,0.55)"
              : "0 14px 36px rgba(37,99,235,0.18)",
        },
      })}
    >
      <Box sx={{ display: "flex", alignItems: "center", gap: 2 }}>
        <Box
          component="img"
          src={logo}
          alt={title}
          sx={{
            width: 56,
            height: 56,
            borderRadius: 2,
            objectFit: "contain",
            backgroundColor: "rgba(255,255,255,0.6)",
            p: 0.5,
            boxShadow: "0 4px 12px rgba(15,23,42,0.12)",
          }}
        />
        <Typography
          variant="h6"
          sx={(theme) => ({
            fontWeight: 700,
            color: theme.palette.mode === "dark" ? "#bfdbfe" : "#1d4ed8",
          })}
        >
          {title}
        </Typography>
      </Box>
      <Typography
        variant="body2"
        color="text.secondary"
        sx={{ lineHeight: 1.6 }}
      >
        {intro}
      </Typography>
      <Box sx={{ flex: 1 }} />
      <Box>
        <Button
          size="small"
          startIcon={<GitHubIcon />}
          onClick={() => openUrl(githubUrl)}
          sx={(theme) => ({
            textTransform: "none",
            color: theme.palette.mode === "dark" ? "#93c5fd" : "#1d4ed8",
            "&:hover": {
              backgroundColor:
                theme.palette.mode === "dark"
                  ? "rgba(59,130,246,0.16)"
                  : "rgba(37,99,235,0.08)",
            },
          })}
        >
          {t("welcome.viewOnGithub")}
        </Button>
      </Box>
    </Box>
  );
}

export default function Welcome() {
  const { t } = useTranslation();
  const addFiles = useMkvStore((s) => s.addFiles);

  const handleAddFiles = async () => {
    const selection = await openDialog({
      multiple: true,
      filters: [{ name: "Matroska", extensions: ["mkv"] }],
    });
    if (!selection) {
      return;
    }
    const paths = Array.isArray(selection) ? selection : [selection];
    if (paths.length === 0) {
      return;
    }
    try {
      const mkvFiles = await getMkvFiles(paths);
      if (mkvFiles.length > 0) {
        addFiles(mkvFiles);
      }
    } catch (err) {
      console.error("Failed to resolve selected files", err);
    }
  };

  const handleAddFolder = async () => {
    const directory = await openDialog({ directory: true });
    if (typeof directory !== "string" || directory.length === 0) {
      return;
    }
    try {
      const mkvFiles = await getMkvFiles([directory]);
      if (mkvFiles.length > 0) {
        addFiles(mkvFiles);
      }
    } catch (err) {
      console.error("Failed to resolve selected folder", err);
    }
  };

  return (
    <Box
      sx={(theme) => ({
        flex: 1,
        minHeight: 0,
        display: "flex",
        justifyContent: "center",
        alignItems: "flex-start",
        py: 4,
        px: 2,
        background:
          theme.palette.mode === "dark"
            ? "radial-gradient(circle at 20% 0%, rgba(30,64,175,0.20), transparent 60%), radial-gradient(circle at 80% 100%, rgba(14,165,233,0.16), transparent 55%)"
            : "radial-gradient(circle at 20% 0%, rgba(191,219,254,0.55), transparent 60%), radial-gradient(circle at 80% 100%, rgba(186,230,253,0.45), transparent 55%)",
        borderRadius: 2,
        overflow: "auto",
      })}
    >
      <Stack spacing={3} sx={{ width: "100%", maxWidth: 880 }}>
        <Box sx={{ textAlign: "center" }}>
          <Typography
            variant="h4"
            sx={(theme) => ({
              fontWeight: 800,
              letterSpacing: "-0.02em",
              background:
                theme.palette.mode === "dark"
                  ? "linear-gradient(90deg, #60a5fa 0%, #38bdf8 100%)"
                  : "linear-gradient(90deg, #1d4ed8 0%, #0284c7 100%)",
              WebkitBackgroundClip: "text",
              WebkitTextFillColor: "transparent",
              backgroundClip: "text",
              color: "transparent",
            })}
          >
            {t("welcome.title")}
          </Typography>
          <Typography variant="body2" color="text.secondary" sx={{ mt: 1 }}>
            {t("welcome.subtitle")}
          </Typography>
        </Box>

        <Box sx={{ display: "flex", flexWrap: "wrap", gap: 2 }}>
          <AppCard
            logo={appIconUrl}
            title="BatchMkvExtract"
            intro={t("welcome.intro")}
            githubUrl={GITHUB_URL}
            isPrimary
          />
          <AppCard
            logo={batchMkvMergeIcon}
            title="BatchMkvMerge"
            intro={t("welcome.introBatchMkvMerge")}
            githubUrl={BATCH_MKV_MERGE_URL}
          />
          <AppCard
            logo={betterMediaInfoIcon}
            title="BetterMediaInfo"
            intro={t("welcome.introBetterMediaInfo")}
            githubUrl={BETTER_MEDIA_INFO_URL}
          />
          <AppCard
            logo={bdMasterIcon}
            title="BDMaster"
            intro={t("welcome.introBDMaster")}
            githubUrl={BD_MASTER_URL}
          />
        </Box>

        <Box
          sx={{
            display: "flex",
            justifyContent: "center",
            gap: 1.5,
            flexWrap: "wrap",
          }}
        >
          <Button
            variant="contained"
            startIcon={<ArticleIcon />}
            onClick={handleAddFiles}
            sx={{
              textTransform: "none",
              fontWeight: 600,
              borderRadius: 2,
              backgroundColor: "#2563eb",
              boxShadow: "0 6px 16px rgba(37,99,235,0.32)",
              "&:hover": { backgroundColor: "#1d4ed8" },
            }}
          >
            {t("welcome.addFiles")}
          </Button>
          <Button
            variant="outlined"
            startIcon={<FolderIcon />}
            onClick={handleAddFolder}
            sx={(theme) => ({
              textTransform: "none",
              fontWeight: 600,
              borderRadius: 2,
              borderColor: theme.palette.mode === "dark" ? "#60a5fa" : "#2563eb",
              color: theme.palette.mode === "dark" ? "#93c5fd" : "#1d4ed8",
              "&:hover": {
                borderColor:
                  theme.palette.mode === "dark" ? "#93c5fd" : "#1d4ed8",
                backgroundColor:
                  theme.palette.mode === "dark"
                    ? "rgba(59,130,246,0.12)"
                    : "rgba(37,99,235,0.06)",
              },
            })}
          >
            {t("welcome.addFolder")}
          </Button>
        </Box>

        <Typography
          variant="caption"
          color="text.secondary"
          sx={{ textAlign: "center", display: "block" }}
        >
          {t("welcome.emptyHint")}
        </Typography>

        <Box
          sx={(theme) => ({
            display: "flex",
            justifyContent: "center",
            alignItems: "center",
            gap: 2,
            flexWrap: "wrap",
            pt: 1,
            borderTop: "1px solid",
            borderColor:
              theme.palette.mode === "dark"
                ? "rgba(148,163,184,0.20)"
                : "rgba(148,163,184,0.30)",
          })}
        >
          <Box sx={{ display: "flex", alignItems: "center", gap: 0.75 }}>
            <PersonIcon fontSize="small" sx={{ color: "text.secondary" }} />
            <Typography variant="caption" color="text.secondary">
              {t("about.author")}:
            </Typography>
            <Link
              component="button"
              onClick={() => openUrl(AUTHOR_URL)}
              underline="hover"
              sx={(theme) => ({
                fontSize: "0.75rem",
                fontWeight: 600,
                color: theme.palette.mode === "dark" ? "#93c5fd" : "#1d4ed8",
              })}
            >
              {AUTHOR_NAME}
            </Link>
          </Box>
          <Box sx={{ display: "flex", alignItems: "center", gap: 0.75 }}>
            <GitHubIcon fontSize="small" sx={{ color: "text.secondary" }} />
            <Link
              component="button"
              onClick={() => openUrl(GITHUB_URL)}
              underline="hover"
              sx={(theme) => ({
                fontSize: "0.75rem",
                fontWeight: 600,
                color: theme.palette.mode === "dark" ? "#93c5fd" : "#1d4ed8",
              })}
            >
              {GITHUB_URL.replace("https://", "")}
            </Link>
          </Box>
        </Box>
      </Stack>
    </Box>
  );
}
