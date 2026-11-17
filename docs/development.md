# Development

## Build rules

- **Always build and run the release profile. Never build a debug build.** Run a release build to validate changes before handing them off. Every cargo invocation in this repo, in docs and in CI carries `--release` (or `-r`): `cargo build --release`, `cargo run --release`, `cargo test --release`, `cargo check --release`, `cargo llvm-cov --release`. GPUI debug builds are slow to compile, slow to start and render at a fraction of release frame rates. Do not add `[profile.dev]` tuning as a workaround; use release.
- **Installers are built with the `dist` profile**, which inherits `release` and adds LTO, `codegen-units = 1` and symbol stripping: `cargo packager --profile dist`, `cargo generate-rpm --profile dist`. `dist` is a release build with slower, tighter optimisation; it is used for packaging only, never for the edit-test loop.

## Migration in progress

The Tauri app this document describes is being replaced by a native [gpui-kit](https://crates.io/crates/gpui-kit) application on the `migrate-gpui-kit` branch. It is no longer built or run, and this document is rewritten when the migration ends; [`CLAUDE.md`](../CLAUDE.md) lists the phases.

The new application is a Cargo workspace at the repository root (one `Cargo.lock`, one `target/`); `src-tauri/` is excluded from it:

- `crates/bme-core` — the UI-free core: config file, per-drive extract queue, MKVToolNix (`mkvmerge -J` track listing, `mkvextract` launch), file intake, BetterMediaInfo, update check.
- `crates/bme-platform` — per-OS integrations GPUI lacks (taskbar / Dock / launcher progress, the display of a window).
- `crates/bme-app` — the gpui-kit application, package `BatchMkvExtract` (library `batch_mkvextract` and a three-line binary), with the packaging metadata and `packaging/`. For now it opens one window with the app's name and version.

The Rust toolchain is pinned in `rust-toolchain.toml` at the repository root (1.100.0; gpui-kit needs 1.92 or newer). The workspace commands:

```bash
cargo run --release -p BatchMkvExtract                   # run the app
cargo test --release --workspace                         # run every test
cargo llvm-cov --release -p bme-core --summary-only --fail-under-lines 90   # the CI coverage gate
cargo build --profile dist -p BatchMkvExtract            # installer build: release + thin LTO
cargo packager --profile dist -p BatchMkvExtract -f app,dmg   # or nsis,wix / deb,appimage (cargo-packager 0.11.8)
cargo generate-rpm --profile dist -p crates/bme-app      # .rpm on Linux (cargo-generate-rpm 0.21.1)
```

## Tech Stack

### Frontend

- [React 19](https://react.dev/) — UI framework
- [MUI v9](https://mui.com/) with `@mui/icons-material` and `@emotion/react` — components and icons
- [Zustand](https://github.com/pmndrs/zustand) — state management
- [react-i18next](https://react.i18next.com/) + [i18next](https://www.i18next.com/) — internationalization (8 locales)
- [TypeScript](https://www.typescriptlang.org/)
- [Vite 7](https://vite.dev/) — dev server and bundler

### Backend

- [Tauri v2](https://v2.tauri.app/) — desktop shell
- [Rust](https://www.rust-lang.org/) (edition 2024, stable toolchain)
- [tokio](https://tokio.rs/) — async runtime (custom multi-thread runtime with 4 workers)
- `serde` + `serde_json` — config serialization
- `anyhow` — error handling
- `log` — diagnostic logging

### Tauri plugins

- `tauri-plugin-opener` — open external URLs
- `tauri-plugin-dialog` — folder picker for the MKVToolNix path
- `tauri-plugin-clipboard-manager` — write the copied mkvextract command

### External tools

- [MKVToolNix](https://mkvtoolnix.download/) — `mkvmerge -J` for track metadata, `mkvextract` for extraction

## Tooling

- [pnpm](https://pnpm.io/) — package manager
- [Deno](https://deno.com/) — scripts under `scripts/ts/` (e.g. `change-version.ts`)

## Repository layout

```
src/                        React app
├── App.tsx                 Theme / language / launch-args bootstrap
├── main.tsx                Entry point
├── store.ts                Zustand store (files, queue, profiles, handlers)
├── service.ts              Wrappers around Tauri invoke calls
├── protocol.ts             Shared types mirroring Rust protocol
├── extract-utils.ts        Template parser, drive-key helper, formatters
├── components/
│   ├── Layout.tsx          Nav / main / footer grid
│   ├── Toolbar.tsx         Extract All, Clear All, Profile, Settings, About
│   ├── MainContent.tsx     Tab container (File List, Queue, Settings, About)
│   ├── FileList.tsx        Drop zone, polling, card list
│   ├── MkvFileCard.tsx     Per-file card with track table and progress row
│   ├── Queue.tsx           Per-drive queue cards
│   ├── Settings.tsx        Appearance / Profiles / MKV settings
│   ├── About.tsx           Hero + author + GitHub
│   └── Footer.tsx          Donation and copyright links
└── i18n/
    ├── index.ts            i18next init
    └── locales/            de, en-US, es, fr, ja, zh-CN, zh-HK, zh-TW

src-tauri/                  Rust crate
├── src/
│   ├── lib.rs              Command registration, setup hook, runtime setup
│   ├── main.rs             Binary entry
│   ├── constants.rs        APP_NAME
│   ├── config.rs           Persisted Config (display mode, theme, language,
│   │                       MKVToolNix path, profiles, active profile, window)
│   ├── controller.rs       Thin wrappers for commands (get_about / get_config /
│   │                       set_config / get_mkv_files)
│   ├── protocol.rs         Shared JSON types (About, MkvTrack, ExtractEntry, …)
│   ├── extract.rs          Per-drive queue, worker threads, cancel, event emit
│   └── mkvtoolnix.rs       mkvmerge/mkvextract resolution, spawn, progress parse
├── icons/                  App icons (also used in the About tab)
└── tauri.conf.json         Window defaults, bundle metadata

scripts/ts/
├── change-version.ts       Bumps version across manifests + workflow YAMLs
└── deno.json               Deno tasks

.github/workflows/
├── linux_build.yml
├── macos_build.yml
└── windows_build.yml
```

## Commands

The Tauri app's commands, kept for reference; the app is no longer built (the workspace commands are under "Migration in progress"):

```bash
pnpm install               # install JS dependencies
pnpm tauri dev --release   # run the app in development (hot-reload frontend + Rust, release Rust build)
pnpm tauri build           # build installers for the current platform
pnpm dev                   # run only the Vite dev server (no Tauri window)
pnpm build                 # type-check and build the frontend
```

To bump the version across the three crate manifests, the three workflow files and the `CFBundleVersion` of `crates/bme-app/packaging/macos/Info.plist`, edit both arguments at the bottom of `scripts/ts/change-version.ts` and run the script below. It then runs `cargo update -w`, so `Cargo.lock` records the new workspace versions, and warns when `docs/release_notes.md` has no `## <version>` section yet.

```bash
cd scripts/ts
deno task change-version
```

## Configuration storage

The persisted config file is named `BatchMkvExtract.json` and lives at:

- **Windows** — `%APPDATA%\BatchMkvExtract\` when installed under `%LOCALAPPDATA%` / `%ProgramFiles%` / `%ProgramFiles(x86)%`; next to the `.exe` in portable mode.
- **Linux** — `$XDG_CONFIG_HOME/BatchMkvExtract/`, else `$HOME/.config/BatchMkvExtract/`.
- **macOS** — `~/Library/Application Support/BatchMkvExtract/`.

## Frontend ↔ Backend protocol

All communication goes through Tauri's `invoke` and event system; there's no IPC codec beyond JSON.

### Commands (`#[tauri::command]`)

| Command | Purpose |
| --- | --- |
| `get_about` | App version for the About tab |
| `get_config` / `set_config` | Load / save persisted config |
| `get_launch_args` | Paths passed on the command line |
| `get_mkv_files` | Resolve dropped paths (files or folders) to `.mkv` file paths |
| `get_mkv_tracks` | Run `mkvmerge -J` and return the track list |
| `is_mkvextract_found` | Validate the MKVToolNix path and surface auto-detected updates |
| `enqueue_extract` | Add a file to the per-drive queue |
| `cancel_extract` | Cancel a queued or running extraction |
| `get_extract_status` | Snapshot of active tasks (frontend polls every 200 ms) |

### Events

| Event | Payload | When |
| --- | --- | --- |
| `extraction-finished` | `{ file, outcome: "Completed" \| "Cancelled" \| "Failed", error? }` | Backend worker exits |

## Concurrency

The backend uses a custom `tokio` runtime with `worker_threads(4)` set in `lib.rs`. Each extraction runs on a dedicated `std::thread::spawn`'d worker, so long-running `mkvextract` child processes do not tie up the async worker pool. Per-drive serialization is enforced in `extract.rs` — see `DriveState` and `on_worker_finished`.

## Template engine

`extract-utils.ts::renderTemplate` is a single-pass character scanner:

- `{{` / `}}` escape to literal braces
- `{placeholder}` replaces with the corresponding value
- Unknown placeholders are emitted verbatim (e.g. `{typo}` stays `{typo}`)

Supported placeholders: `{file_name}`, `{track_id}`, `{track_number}`, `{language}`, `{codec_name}`, `{track_name}`. `{codec_name}` and `{track_name}` are sanitized for filesystem-unsafe characters before substitution.

## CI

Three GitHub Actions workflows test the workspace and build the installers of the gpui-kit application on push/PR, with no Node, pnpm or Tauri step:

- `linux_build.yml` (`ubuntu-22.04`, for a low glibc floor) — GPUI's build packages and Mesa's Vulkan drivers, `cargo test --release --workspace --locked --exclude bme-core`, the `bme-core` coverage gate (`cargo llvm-cov --release -p bme-core --locked --summary-only --fail-under-lines 90`), then `.deb` and `.AppImage` (cargo-packager) and `.rpm` (cargo-generate-rpm); a non-blocking `lint` job runs `cargo fmt --all --check` and clippy on `bme-core`.
- `macos_build.yml` — `.dmg` for x86_64 (`macos-15-intel`) and arm64 (`macos-15`).
- `windows_build.yml` — one `.msi` per language and the NSIS `.exe`.

Each installs the toolchain pinned in `rust-toolchain.toml` (`actions-rust-lang/setup-rust-toolchain` without a `toolchain:` input, with `build-warnings: warn` so warnings do not fail the build), runs the tests, builds with `--profile dist` and uploads seven artifacts in all under the 0.3.0 artifact names. The workflows run with `permissions: contents: read` and pin every action to a commit SHA; `.github/dependabot.yml` proposes updates.
