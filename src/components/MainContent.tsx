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

import { useEffect } from "react";
import { Box, IconButton, Tab, Tabs, Tooltip } from "@mui/material";
import CloseIcon from "@mui/icons-material/Close";
import { useTranslation } from "react-i18next";
import { TabType, useMkvStore } from "../store";
import About from "./About";
import FileList from "./FileList";
import Queue from "./Queue";
import Settings from "./Settings";

export default function MainContent() {
  const { t } = useTranslation();
  const activeTab = useMkvStore((s) => s.activeTab);
  const showSettings = useMkvStore((s) => s.showSettings);
  const showAbout = useMkvStore((s) => s.showAbout);
  const queueOrder = useMkvStore((s) => s.queueOrder);
  const setActiveTab = useMkvStore((s) => s.setActiveTab);
  const closeSettings = useMkvStore((s) => s.closeSettings);
  const closeAbout = useMkvStore((s) => s.closeAbout);

  const hasQueue = queueOrder.length > 0;

  const tabs: TabType[] = ["fileList"];
  if (hasQueue) {
    tabs.push("queue");
  }
  if (showSettings) {
    tabs.push("settings");
  }
  if (showAbout) {
    tabs.push("about");
  }

  useEffect(() => {
    if (activeTab === "queue" && !hasQueue) {
      setActiveTab("fileList");
    }
  }, [activeTab, hasQueue, setActiveTab]);

  const activeIndex = Math.max(0, tabs.indexOf(activeTab));

  const labelOf = (type: TabType) => {
    switch (type) {
      case "fileList":
        return t("tabs.fileList");
      case "queue":
        return t("tabs.queue");
      case "settings":
        return t("tabs.settings");
      case "about":
        return t("tabs.about");
    }
  };

  const closeHandlerOf = (type: TabType) => {
    if (type === "settings") {
      return closeSettings;
    }
    if (type === "about") {
      return closeAbout;
    }
    return null;
  };

  const closeTab = (type: TabType) => {
    const handler = closeHandlerOf(type);
    if (!handler) {
      return;
    }
    if (type === activeTab) {
      const idx = tabs.indexOf(type);
      const leftIdx = Math.max(0, idx - 1);
      const leftTab = tabs[leftIdx];
      if (leftTab && leftTab !== type) {
        setActiveTab(leftTab);
      }
    }
    handler();
  };

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey) {
        return;
      }
      if (event.key === "Tab") {
        event.preventDefault();
        event.stopPropagation();
        if (tabs.length <= 1) {
          return;
        }
        const delta = event.shiftKey ? -1 : 1;
        const next = (activeIndex + delta + tabs.length) % tabs.length;
        setActiveTab(tabs[next]);
        return;
      }
      if (event.shiftKey) {
        return;
      }
      if (event.key >= "1" && event.key <= "9") {
        const idx = parseInt(event.key, 10) - 1;
        if (idx >= 0 && idx < tabs.length) {
          event.preventDefault();
          event.stopPropagation();
          setActiveTab(tabs[idx]);
        }
        return;
      }
      if (event.key === "w" || event.key === "W") {
        event.preventDefault();
        event.stopPropagation();
        const current = tabs[activeIndex];
        if (!current) {
          return;
        }
        closeTab(current);
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
    };
  }, [tabs, activeIndex, activeTab, setActiveTab, closeSettings, closeAbout]);

  return (
    <Box
      sx={{
        width: "100%",
        height: "100%",
        overflow: "hidden",
        display: "flex",
        flexDirection: "column",
      }}
    >
      <Box sx={{ borderBottom: 1, borderColor: "divider", flexShrink: 0 }}>
        <Tabs
          value={activeIndex}
          onChange={(_, index) => setActiveTab(tabs[index])}
          variant="scrollable"
          scrollButtons="auto"
          sx={{
            mt: 0,
            minHeight: "24px",
            "& .MuiTab-root": { textTransform: "none" },
          }}
        >
          {tabs.map((type) => {
            const closable = closeHandlerOf(type) !== null;
            return (
              <Tab
                key={type}
                style={{ minHeight: "24px" }}
                sx={{ py: 0, my: 0 }}
                label={
                  <Box
                    sx={{ display: "flex", alignItems: "center", gap: 0.5 }}
                  >
                    <span>{labelOf(type)}</span>
                    {closable && (
                      <Tooltip title={t("tabs.close")}>
                        <IconButton
                          size="small"
                          component="span"
                          sx={{ ml: 0.5, p: 0.25 }}
                          onClick={(e) => {
                            e.stopPropagation();
                            closeTab(type);
                          }}
                        >
                          <CloseIcon sx={{ fontSize: 14 }} />
                        </IconButton>
                      </Tooltip>
                    )}
                  </Box>
                }
              />
            );
          })}
        </Tabs>
      </Box>

      <Box
        sx={{
          p: 1,
          border: 1,
          borderColor: "divider",
          borderTop: 0,
          borderRadius: "0 0 4px 4px",
          width: "100%",
          flex: 1,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
        }}
      >
        {tabs.map((type) => (
          <Box
            key={type}
            sx={{
              display: type === activeTab ? "flex" : "none",
              flexDirection: "column",
              flex: 1,
              minHeight: 0,
              overflow: "auto",
            }}
          >
            {type === "fileList" && <FileList />}
            {type === "queue" && <Queue />}
            {type === "settings" && <Settings />}
            {type === "about" && <About />}
          </Box>
        ))}
      </Box>
    </Box>
  );
}
