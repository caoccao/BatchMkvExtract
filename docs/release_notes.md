# Release Notes

## 0.2.0

### Getting started

* The empty file list now opens to a Welcome screen with clear Add Files and Add Folder actions
* The Welcome screen keeps the drag-and-drop hint visible and links to the project on GitHub

### External tools

* Settings now has a dedicated Integration tab for MKVToolNix and BetterMediaInfo
* MKVToolNix and BetterMediaInfo paths can both be browsed for or detected from the same settings flow
* Tool detection now shows localized found and not-found messages
* Opening files in BetterMediaInfo behaves more naturally on macOS and no longer opens an extra console window on Windows
* The BetterMediaInfo icon is easier to see

### Appearance and language

* Italian has been added, bringing the UI to 9 languages: English (US), Deutsch, Español, Français, Italiano, 日本語, 简体中文, 繁體中文 (香港), 繁體中文 (台灣)
* Theme names are now translated in every supported language
* On first launch, the app can start in your system language when it is supported

### Installation

* Windows installers now include localized installer languages
* Windows installers can download the required WebView2 runtime when needed

## 0.1.0

### Platforms

* Windows (x86_64)
* Linux (x86_64)
* macOS (x86_64 and arm64)

### Add files

* Drag and drop `.mkv` files or folders onto the window
* Launch the app with file or folder paths as arguments to pre-populate the list
* Integrate with [BetterMediaInfo](https://github.com/caoccao/BetterMediaInfo)

### Extract

* Extract one file from its card, or Extract All selected files at once (F3)
* Copy Command puts the equivalent `mkvextract` command line on the clipboard
* On Windows, files on different drives extract in parallel; files on the same drive extract one after another
* On Linux and macOS, one file at a time
* Cancel any extraction from its card or from the Queue tab

### Queue tab

* Appears automatically when any file is queued
* Shows File Path, Status, Start, End, Elapsed, ETA for each item
* Statuses: Waiting, Extracting, Completed, Cancelled, Failed
* Completed, Cancelled, and Failed rows stay visible; clear them per drive with the Clear Completed button

### Profiles

* Switch profiles from the toolbar (F9)
* Each profile has its own filename templates for video, audio, and subtitle tracks
* Placeholders: `{file_name}`, `{track_id}`, `{track_number}`, `{language}`, `{codec_name}`, `{track_name}`
* Each profile chooses which track types get selected by default when a file is added
* The built-in `Default` profile selects subtitles
* Add, reset, and delete profiles from Settings

### Appearance and language

* Auto / Light / Dark display mode, 20 color themes
* 8 UI languages: English (US), Deutsch, Español, Français, 日本語, 简体中文, 繁體中文 (香港), 繁體中文 (台灣)
* Dark mode is applied throughout, including queued-card tint and progress bar

### Settings

* MKVToolNix path with live detection of `mkvextract`; on macOS, the latest versioned install is auto-detected
* Your selected profile, templates, theme, language, and MKVToolNix path are remembered between runs
