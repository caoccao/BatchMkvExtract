# BatchMkvExtract

[![Linux Build](https://github.com/caoccao/BatchMkvExtract/actions/workflows/linux_build.yml/badge.svg)](https://github.com/caoccao/BatchMkvExtract/actions/workflows/linux_build.yml) [![MacOS Build](https://github.com/caoccao/BatchMkvExtract/actions/workflows/macos_build.yml/badge.svg)](https://github.com/caoccao/BatchMkvExtract/actions/workflows/macos_build.yml) [![Windows Build](https://github.com/caoccao/BatchMkvExtract/actions/workflows/windows_build.yml/badge.svg)](https://github.com/caoccao/BatchMkvExtract/actions/workflows/windows_build.yml)

![BatchMkvExtract](src-tauri/icons/128x128@2x.png)

BatchMkvExtract is a desktop GUI that drives [mkvextract](https://mkvtoolnix.download/doc/mkvextract.html) from [MKVToolNix](https://mkvtoolnix.download/) across many `.mkv` files at once, so you can pull tracks, chapters, and attachments out of a whole batch without hand-writing commands.

![BatchMkvExtract screenshot](docs/screenshots/group_view_extracting.png)

## Features

* Cross-platform: Windows, Linux, macOS (intel + arm64)
* Drag and drop `.mkv` files or folders
* Per-drive parallel extraction with queue
* Profile-based track filename templates
* Live progress bar with elapsed time and ETA
* Cancel extractions from the card or queue
* 8 UI languages with light and dark modes
* Integrate with [BetterMediaInfo](https://github.com/caoccao/BetterMediaInfo)

## Installation

* Make sure you have [MKVToolNix](https://mkvtoolnix.download/) installed.
* Download the latest installer from [Releases](https://github.com/caoccao/BatchMkvExtract/releases) or [Actions](https://github.com/caoccao/BatchMkvExtract/actions).
* Run the installer, then launch the app from the Start Menu (Windows) or Applications folder (macOS).

## Documentation

* [Development](docs/development.md)
* [Release Notes](docs/release_notes.md)

## License

[APACHE LICENSE, VERSION 2.0](LICENSE)
