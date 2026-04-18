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

import { Box, ButtonGroup, IconButton, Tooltip } from "@mui/material";
import InfoIcon from "@mui/icons-material/Info";
import SettingsIcon from "@mui/icons-material/Settings";
import { useTranslation } from "react-i18next";
import { useMkvStore } from "../store";

export default function Toolbar() {
  const { t } = useTranslation();
  const activeTab = useMkvStore((s) => s.activeTab);
  const openSettings = useMkvStore((s) => s.openSettings);
  const openAbout = useMkvStore((s) => s.openAbout);

  const buttonSx = {
    width: 28,
    height: 28,
    margin: "2px",
    borderRadius: 1,
  };
  const activeButtonSx = {
    ...buttonSx,
    color: "primary.main",
  };

  return (
    <Box sx={{ mx: 1, my: 0, display: "flex", gap: 1 }}>
      <ButtonGroup variant="outlined" size="small">
        <Tooltip title={t("toolbar.settings")}>
          <IconButton
            sx={activeTab === "settings" ? activeButtonSx : buttonSx}
            onClick={openSettings}
          >
            <SettingsIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <Tooltip title={t("toolbar.about")}>
          <IconButton
            sx={activeTab === "about" ? activeButtonSx : buttonSx}
            onClick={openAbout}
          >
            <InfoIcon fontSize="small" />
          </IconButton>
        </Tooltip>
      </ButtonGroup>
    </Box>
  );
}
