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

import { useCallback, useState } from "react";
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
  Switch,
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
import UpdateIcon from "@mui/icons-material/Update";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import * as Protocol from "../protocol";
import { detectBetterMediaInfo, isMkvtoolnixFound } from "../service";
import { useMkvStore } from "../store";
import { ExternalToolPathRow } from "./settings/ExternalToolPathRow";
import {
  DetectToolPath,
  useToolPathDetection,
} from "./settings/useToolPathDetection";

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
  const setBetterMediaInfoAvailable = useMkvStore(
    (s) => s.setBetterMediaInfoAvailable,
  );
  const [newProfileName, setNewProfileName] = useState("");

  const updateExternalTools = useCallback(
    (patch: Partial<Protocol.ConfigExternalTools>) => {
      if (!config) {
        return;
      }
      updateConfig({
        externalTools: {
          mkvToolNixPath:
            patch.mkvToolNixPath ?? config.externalTools?.mkvToolNixPath ?? "",
          betterMediaInfoPath:
            patch.betterMediaInfoPath ??
            config.externalTools?.betterMediaInfoPath ??
            "",
        },
      });
    },
    [config, updateConfig],
  );

  const persistMkvToolNixPath = useCallback(
    (value: string) => {
      if (!config) {
        return;
      }
      if (value === (config.externalTools?.mkvToolNixPath ?? "")) {
        return;
      }
      updateExternalTools({ mkvToolNixPath: value });
    },
    [config, updateExternalTools],
  );

  const persistBetterMediaInfoPath = useCallback(
    (value: string) => {
      if (!config) {
        return;
      }
      if (value === (config.externalTools?.betterMediaInfoPath ?? "")) {
        return;
      }
      updateExternalTools({ betterMediaInfoPath: value });
    },
    [config, updateExternalTools],
  );

  const detectMkvToolNixPath = useCallback<DetectToolPath>(
    async (path, checkRunning = false) => {
      const status = await isMkvtoolnixFound(path, checkRunning);
      return {
        found: status.found,
        path: status.mkvToolNixPath,
      };
    },
    [],
  );

  const detectBetterMediaInfoPath = useCallback<DetectToolPath>(
    async (path, checkRunning = false) => {
      const status = await detectBetterMediaInfo(path, checkRunning);
      return {
        found: status.found,
        path: status.path,
      };
    },
    [],
  );

  const {
    path: mkvToolNixPath,
    setPath: setMkvToolNixPath,
    detection: mkvtoolnixFound,
    detectNow: handleDetectMkvToolNix,
    handleBlur: handlePathBlur,
  } = useToolPathDetection({
    ready: config !== null,
    initialPath: config?.externalTools?.mkvToolNixPath ?? "",
    detectPath: detectMkvToolNixPath,
    persistPath: persistMkvToolNixPath,
  });

  const {
    path: betterMediaInfoPath,
    setPath: setBetterMediaInfoPath,
    detection: betterMediaInfoDetection,
    detectNow: handleDetectBetterMediaInfo,
    handleBlur: handleBetterMediaInfoPathBlur,
  } = useToolPathDetection({
    ready: config !== null,
    initialPath: config?.externalTools?.betterMediaInfoPath ?? "",
    detectPath: detectBetterMediaInfoPath,
    persistPath: persistBetterMediaInfoPath,
    onFoundChange: setBetterMediaInfoAvailable,
  });

  const handleBrowseMkvToolNixPath = async () => {
    const directory = await open({
      directory: true,
      defaultPath: mkvToolNixPath.trim() || undefined,
    });
    if (typeof directory === "string" && directory.length > 0) {
      setMkvToolNixPath(directory);
      persistMkvToolNixPath(directory);
    }
  };

  const handleBrowseBetterMediaInfoPath = async () => {
    const directory = await open({
      directory: true,
      defaultPath: betterMediaInfoPath.trim() || undefined,
    });
    if (typeof directory === "string" && directory.length > 0) {
      setBetterMediaInfoPath(directory);
      persistBetterMediaInfoPath(directory);
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
          <SettingRow label={t("settings.mode")}>
            <ToggleButtonGroup
              value={config.displayMode}
              exclusive
              size="small"
              onChange={(_e, value) => {
                if (value !== null) {
                  updateConfig({ displayMode: value as Protocol.DisplayMode });
                }
              }}
              sx={{ "& .MuiToggleButton-root": { textTransform: "none" } }}
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
                    {t(`settings.theme${theme}`)}
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
                  sx={{
                    height: 36,
                    textTransform: "none",
                    whiteSpace: "nowrap",
                  }}
                >
                  {t("settings.addProfile")}
                </Button>
                <Button
                  variant="outlined"
                  size="small"
                  color="error"
                  disabled={!canDelete}
                  onClick={() => deleteActiveProfile()}
                  sx={{
                    height: 36,
                    textTransform: "none",
                    whiteSpace: "nowrap",
                  }}
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
                      languagesKey: "videoLanguages" as const,
                      label: t("settings.video"),
                    },
                    {
                      typeKey: "audio" as const,
                      templateKey: "audioTemplate" as const,
                      selectKey: "selectAudio" as const,
                      languagesKey: "audioLanguages" as const,
                      label: t("settings.audio"),
                    },
                    {
                      typeKey: "subtitles" as const,
                      templateKey: "subtitleTemplate" as const,
                      selectKey: "selectSubtitle" as const,
                      languagesKey: "subtitleLanguages" as const,
                      label: t("settings.subtitles"),
                    },
                    {
                      typeKey: "chapters" as const,
                      templateKey: "chaptersTemplate" as const,
                      selectKey: "selectChapters" as const,
                      languagesKey: null,
                      label: t("settings.chapters"),
                    },
                    {
                      typeKey: "attachments" as const,
                      templateKey: "attachmentsTemplate" as const,
                      selectKey: "selectAttachments" as const,
                      languagesKey: null,
                      label: t("settings.attachments"),
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
                    <Box
                      sx={{
                        display: "flex",
                        alignItems: "center",
                        gap: 1,
                        mt: 0.5,
                      }}
                    >
                      <FormControlLabel
                        sx={{ mr: 0 }}
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
                            {row.languagesKey
                              ? t("settings.autoSelectOnDropForLanguages")
                              : t("settings.autoSelectOnDrop")}
                          </Typography>
                        }
                      />
                      {row.languagesKey && (
                        <TextField
                          size="small"
                          value={activeProfile[row.languagesKey] ?? ""}
                          onChange={(e) =>
                            updateActiveProfile({
                              [row.languagesKey]: e.target.value,
                            })
                          }
                          sx={{ flex: 1 }}
                        />
                      )}
                    </Box>
                  </Box>
                ))}
              </Stack>
              <SettingRow label={t("settings.defaultGroupMode")}>
                <Switch
                  size="small"
                  checked={activeProfile.defaultGroupMode}
                  onChange={(e) =>
                    updateActiveProfile({
                      defaultGroupMode: e.target.checked,
                    })
                  }
                />
              </SettingRow>
              <Typography
                variant="caption"
                color="text.secondary"
                sx={{ display: "block", mt: 1 }}
              >
                {t("settings.templatePlaceholdersHint")}
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
            title={t("settings.externalTools")}
          />
          <Box sx={{ py: 1 }}>
            <ExternalToolPathRow
              label={t("settings.mkvToolNixPath")}
              value={mkvToolNixPath}
              status={mkvtoolnixFound ?? false}
              foundLabel={t("settings.mkvToolNixFound")}
              notFoundLabel={t("settings.mkvToolNixNotFound")}
              browseLabel={t("settings.browse")}
              detectLabel={t("settings.detect")}
              onChange={setMkvToolNixPath}
              onBlur={handlePathBlur}
              onBrowse={handleBrowseMkvToolNixPath}
              onDetect={handleDetectMkvToolNix}
            />
          </Box>
          <Box sx={{ py: 1, borderTop: 1, borderColor: "divider" }}>
            <ExternalToolPathRow
              label={t("settings.betterMediaInfoPath")}
              value={betterMediaInfoPath}
              status={betterMediaInfoDetection}
              foundLabel={t("settings.betterMediaInfoFound")}
              notFoundLabel={t("settings.betterMediaInfoNotFound")}
              browseLabel={t("settings.browse")}
              detectLabel={t("settings.detect")}
              onChange={setBetterMediaInfoPath}
              onBlur={handleBetterMediaInfoPathBlur}
              onBrowse={handleBrowseBetterMediaInfoPath}
              onDetect={handleDetectBetterMediaInfo}
            />
          </Box>
        </Paper>

        <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
          <SectionHeader
            icon={<UpdateIcon fontSize="small" />}
            title={t("settings.update")}
          />
          <Box sx={{ display: "flex", alignItems: "center", gap: 1, py: 1 }}>
            <Typography variant="body2" color="text.secondary">
              {t("settings.checkNewVersion")}
            </Typography>
            <FormControl size="small" sx={{ minWidth: 150 }}>
              <Select
                value={
                  config.update?.checkInterval ??
                  Protocol.UpdateCheckInterval.Weekly
                }
                onChange={(e) => {
                  const next = e.target.value as Protocol.UpdateCheckInterval;
                  updateConfig({
                    update: {
                      checkInterval: next,
                      lastChecked: config.update?.lastChecked ?? 0,
                      lastVersion: config.update?.lastVersion ?? "",
                      ignoreVersion: config.update?.ignoreVersion ?? "",
                    },
                  });
                }}
              >
                <MenuItem value={Protocol.UpdateCheckInterval.Daily}>
                  {t("settings.daily")}
                </MenuItem>
                <MenuItem value={Protocol.UpdateCheckInterval.Weekly}>
                  {t("settings.weekly")}
                </MenuItem>
                <MenuItem value={Protocol.UpdateCheckInterval.Monthly}>
                  {t("settings.monthly")}
                </MenuItem>
              </Select>
            </FormControl>
          </Box>
        </Paper>
      </Stack>
    </Box>
  );
}
