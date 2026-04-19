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
  Box,
  Button,
  Checkbox,
  FormControl,
  FormControlLabel,
  MenuItem,
  Paper,
  Select,
  Stack,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
} from "@mui/material";
import BrightnessAutoIcon from "@mui/icons-material/BrightnessAuto";
import ContentCutIcon from "@mui/icons-material/ContentCut";
import DarkModeIcon from "@mui/icons-material/DarkMode";
import LightModeIcon from "@mui/icons-material/LightMode";
import PaletteIcon from "@mui/icons-material/Palette";
import PersonIcon from "@mui/icons-material/Person";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import * as Protocol from "../protocol";
import { isMkvextractFound } from "../service";
import { useMkvStore } from "../store";

function SectionHeader({
  icon,
  title,
}: {
  icon: React.ReactNode;
  title: string;
}) {
  return (
    <Box sx={{ display: "flex", alignItems: "center", gap: 1, mb: 2 }}>
      <Box sx={{ color: "primary.main", display: "flex" }}>{icon}</Box>
      <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
        {title}
      </Typography>
    </Box>
  );
}

function SettingRow({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <Box
      sx={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        py: 1,
        "&:not(:last-child)": { borderBottom: 1, borderColor: "divider" },
      }}
    >
      <Typography variant="body2" color="text.secondary">
        {label}
      </Typography>
      <Box>{children}</Box>
    </Box>
  );
}

export default function Settings() {
  const { t } = useTranslation();
  const config = useMkvStore((s) => s.config);
  const updateConfig = useMkvStore((s) => s.updateConfig);
  const updateActiveProfile = useMkvStore((s) => s.updateActiveProfile);
  const addProfile = useMkvStore((s) => s.addProfile);
  const deleteActiveProfile = useMkvStore((s) => s.deleteActiveProfile);
  const setActiveProfile = useMkvStore((s) => s.setActiveProfile);
  const resetActiveProfileTemplates = useMkvStore(
    (s) => s.resetActiveProfileTemplates,
  );

  const [mkvToolNixPath, setMkvToolNixPath] = useState("");
  const [mkvextractFound, setMkvextractFound] = useState(false);
  const [newProfileName, setNewProfileName] = useState("");
  const checkDebounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(
    undefined,
  );
  const initializedRef = useRef(false);

  useEffect(() => {
    if (config && !initializedRef.current) {
      initializedRef.current = true;
      setMkvToolNixPath(config.mkv?.mkvToolNixPath ?? "");
    }
  }, [config]);

  useEffect(() => {
    if (!initializedRef.current) {
      return;
    }
    if (checkDebounceRef.current) {
      clearTimeout(checkDebounceRef.current);
    }
    let cancelled = false;
    checkDebounceRef.current = setTimeout(async () => {
      try {
        const status = await isMkvextractFound(mkvToolNixPath.trim());
        if (cancelled) {
          return;
        }
        setMkvextractFound(status.found);
        if (
          status.found &&
          status.mkvToolNixPath &&
          status.mkvToolNixPath !== mkvToolNixPath
        ) {
          setMkvToolNixPath(status.mkvToolNixPath);
          if (config && config.mkv?.mkvToolNixPath !== status.mkvToolNixPath) {
            updateConfig({
              mkv: { mkvToolNixPath: status.mkvToolNixPath },
            });
          }
        }
      } catch {
        if (!cancelled) {
          setMkvextractFound(false);
        }
      }
    }, 250);
    return () => {
      cancelled = true;
      if (checkDebounceRef.current) {
        clearTimeout(checkDebounceRef.current);
      }
    };
  }, [mkvToolNixPath, config, updateConfig]);

  const handleBrowseMkvToolNixPath = async () => {
    const directory = await open({
      directory: true,
      defaultPath: mkvToolNixPath.trim() || undefined,
    });
    if (typeof directory === "string" && directory.length > 0) {
      setMkvToolNixPath(directory);
      updateConfig({ mkv: { mkvToolNixPath: directory } });
    }
  };

  const handlePathBlur = () => {
    const trimmed = mkvToolNixPath.trim();
    if (config && trimmed !== (config.mkv?.mkvToolNixPath ?? "")) {
      updateConfig({ mkv: { mkvToolNixPath: trimmed } });
    }
  };

  if (!config) {
    return null;
  }

  return (
    <Box sx={{ width: "100%", maxWidth: 640, mx: "auto", py: 2, px: 1 }}>
      <Stack spacing={2}>
        <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
          <SectionHeader
            icon={<PaletteIcon fontSize="small" />}
            title={t("settings.appearance")}
          />
          <SettingRow label={t("settings.displayMode")}>
            <ToggleButtonGroup
              value={config.displayMode}
              exclusive
              size="small"
              onChange={(_e, value) => {
                if (value !== null) {
                  updateConfig({ displayMode: value as Protocol.DisplayMode });
                }
              }}
            >
              <ToggleButton
                value={Protocol.DisplayMode.Auto}
                sx={{ px: 1.5, gap: 0.5 }}
              >
                <BrightnessAutoIcon sx={{ fontSize: 16 }} />
                <Typography variant="caption">
                  {t("settings.autoMode")}
                </Typography>
              </ToggleButton>
              <ToggleButton
                value={Protocol.DisplayMode.Light}
                sx={{ px: 1.5, gap: 0.5 }}
              >
                <LightModeIcon sx={{ fontSize: 16 }} />
                <Typography variant="caption">
                  {t("settings.lightMode")}
                </Typography>
              </ToggleButton>
              <ToggleButton
                value={Protocol.DisplayMode.Dark}
                sx={{ px: 1.5, gap: 0.5 }}
              >
                <DarkModeIcon sx={{ fontSize: 16 }} />
                <Typography variant="caption">
                  {t("settings.darkMode")}
                </Typography>
              </ToggleButton>
            </ToggleButtonGroup>
          </SettingRow>
          <SettingRow label={t("settings.theme")}>
            <FormControl size="small" sx={{ minWidth: 150 }}>
              <Select
                value={config.theme}
                onChange={(e) =>
                  updateConfig({ theme: e.target.value as Protocol.Theme })
                }
              >
                {Protocol.getThemes().map((theme) => (
                  <MenuItem key={theme} value={theme}>
                    {theme}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </SettingRow>
          <SettingRow label={t("settings.language")}>
            <FormControl size="small" sx={{ minWidth: 180 }}>
              <Select
                value={config.language}
                onChange={(e) =>
                  updateConfig({
                    language: e.target.value as Protocol.Language,
                  })
                }
              >
                {Protocol.getLanguages().map((lang) => (
                  <MenuItem key={lang} value={lang}>
                    {Protocol.getLanguageLabel(lang)}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </SettingRow>
        </Paper>

        {(() => {
          const activeProfile =
            config.profiles.find((p) => p.name === config.activeProfile) ??
            config.profiles[0];
          if (!activeProfile) {
            return null;
          }
          const trimmed = newProfileName.trim();
          const canAdd =
            trimmed.length > 0 &&
            !config.profiles.some((p) => p.name === trimmed);
          const canDelete =
            config.activeProfile !== Protocol.DEFAULT_PROFILE_NAME;
          return (
            <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
              <SectionHeader
                icon={<PersonIcon fontSize="small" />}
                title={t("settings.profiles")}
              />
              <SettingRow label={t("settings.activeProfile")}>
                <FormControl size="small" sx={{ minWidth: 180 }}>
                  <Select
                    value={config.activeProfile}
                    onChange={(e) => setActiveProfile(e.target.value)}
                  >
                    {config.profiles.map((p) => (
                      <MenuItem key={p.name} value={p.name}>
                        {p.name}
                      </MenuItem>
                    ))}
                  </Select>
                </FormControl>
              </SettingRow>
              <Box
                sx={{
                  display: "flex",
                  alignItems: "center",
                  gap: 1,
                  py: 1,
                  borderBottom: 1,
                  borderColor: "divider",
                }}
              >
                <TextField
                  size="small"
                  placeholder={t("settings.newProfileName")}
                  value={newProfileName}
                  onChange={(e) => setNewProfileName(e.target.value)}
                  sx={{ flex: 1 }}
                />
                <Button
                  variant="outlined"
                  size="small"
                  disabled={!canAdd}
                  onClick={async () => {
                    await addProfile(trimmed);
                    setNewProfileName("");
                  }}
                  sx={{ textTransform: "none", whiteSpace: "nowrap" }}
                >
                  {t("settings.addProfile")}
                </Button>
                <Button
                  variant="outlined"
                  size="small"
                  color="error"
                  disabled={!canDelete}
                  onClick={() => deleteActiveProfile()}
                  sx={{ textTransform: "none", whiteSpace: "nowrap" }}
                >
                  {t("settings.deleteProfile")}
                </Button>
              </Box>
              <Stack spacing={1.5} sx={{ py: 1 }}>
                {(
                  [
                    {
                      typeKey: "video" as const,
                      templateKey: "videoTemplate" as const,
                      selectKey: "selectVideo" as const,
                      label: t("settings.video"),
                    },
                    {
                      typeKey: "audio" as const,
                      templateKey: "audioTemplate" as const,
                      selectKey: "selectAudio" as const,
                      label: t("settings.audio"),
                    },
                    {
                      typeKey: "subtitles" as const,
                      templateKey: "subtitleTemplate" as const,
                      selectKey: "selectSubtitle" as const,
                      label: t("settings.subtitles"),
                    },
                  ] as const
                ).map((row) => (
                  <Box key={row.typeKey}>
                    <Typography
                      variant="caption"
                      color="text.secondary"
                      sx={{ fontWeight: 600 }}
                    >
                      {row.label}
                    </Typography>
                    <TextField
                      size="small"
                      fullWidth
                      value={activeProfile[row.templateKey]}
                      onChange={(e) =>
                        updateActiveProfile({
                          [row.templateKey]: e.target.value,
                        })
                      }
                      sx={{ mt: 0.5 }}
                    />
                    <FormControlLabel
                      sx={{ mt: 0.5 }}
                      control={
                        <Checkbox
                          size="small"
                          checked={activeProfile[row.selectKey]}
                          onChange={(e) =>
                            updateActiveProfile({
                              [row.selectKey]: e.target.checked,
                            })
                          }
                        />
                      }
                      label={
                        <Typography variant="caption">
                          {t("settings.autoSelectOnDrop")}
                        </Typography>
                      }
                    />
                  </Box>
                ))}
              </Stack>
              <Typography
                variant="caption"
                color="text.secondary"
                sx={{ display: "block", mt: 1 }}
              >
                {t("settings.templateTokensHint")}
              </Typography>
              <Box sx={{ display: "flex", justifyContent: "flex-end", mt: 1 }}>
                <Button
                  variant="outlined"
                  size="small"
                  onClick={() => resetActiveProfileTemplates()}
                  sx={{ textTransform: "none" }}
                >
                  {t("settings.resetTemplates")}
                </Button>
              </Box>
            </Paper>
          );
        })()}

        <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
          <SectionHeader
            icon={<ContentCutIcon fontSize="small" />}
            title={t("settings.mkv")}
          />
          <Box sx={{ py: 1 }}>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
              {t("settings.mkvToolNixPath")}
            </Typography>
            <Box sx={{ display: "flex", alignItems: "center", gap: 1 }}>
              <TextField
                value={mkvToolNixPath}
                onChange={(e) => setMkvToolNixPath(e.target.value)}
                onBlur={handlePathBlur}
                size="small"
                fullWidth
              />
              <Button
                variant="outlined"
                size="small"
                onClick={handleBrowseMkvToolNixPath}
                sx={{ minWidth: 90, height: 36, textTransform: "none" }}
              >
                {t("settings.browse")}
              </Button>
            </Box>
            <Typography
              variant="caption"
              sx={{
                mt: 0.75,
                display: "block",
                color: mkvextractFound ? "success.main" : "error.main",
              }}
            >
              {mkvextractFound
                ? t("settings.mkvextractFound")
                : t("settings.mkvextractNotFound")}
            </Typography>
          </Box>
        </Paper>
      </Stack>
    </Box>
  );
}
