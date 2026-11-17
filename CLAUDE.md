# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build rules

- **Always build and run the release profile. Never build a debug build.** Run a release build to validate changes before handing them off. Every cargo invocation in this repo, in docs and in CI carries `--release` (or `-r`): `cargo build --release`, `cargo run --release`, `cargo test --release`, `cargo check --release`, `cargo llvm-cov --release`. GPUI debug builds are slow to compile, slow to start and render at a fraction of release frame rates. Do not add `[profile.dev]` tuning as a workaround; use release.
- **Installers are built with the `dist` profile**, which inherits `release` and adds LTO, `codegen-units = 1` and symbol stripping: `cargo packager --profile dist`, `cargo generate-rpm --profile dist`. `dist` is a release build with slower, tighter optimisation; it is used for packaging only, never for the edit-test loop.

## Migration in progress

The Tauri v2 + React/MUI shell is being replaced by a native [gpui-kit](https://crates.io/crates/gpui-kit) application on the `migrate-gpui-kit` branch, built by copying and adapting the solution of [BatchMkvMerge](https://github.com/caoccao/BatchMkvMerge). The maintainer keeps the migration plan (phases 0–8) outside the repository. Each phase lands as one Conventional Commit on that branch, so `git log` shows which phases are done. The work ends with a pull request into `main`; the branch creates no tags and no releases.

The Tauri app is no longer built, run or tested. `src/` and `src-tauri/` stay in the tree as unbuilt porting reference (excluded from the Cargo workspace from Phase 1) and are deleted in Phase 8. Until then the sections below describe that Tauri app, not the target architecture.

| Phase | Content |
| --- | --- |
| 0 | Ground rules (the Build rules above), branch |
| 1 | Cargo workspace, `crates/bme-core`, `crates/bme-platform`, packaged gpui-kit skeleton on CI |
| 2 | Extraction domain logic ported to Rust, with tests and a golden test |
| 3 | Application shell |
| 4 | State stores, track loading, extract events, actions |
| 5 | File cards, track table, file list, output path dialog |
| 6 | Queue, completion notifications, taskbar progress |
| 7 | Settings dialog, profile menu and Group Mode toggle, update banner, numeric word navigation |
| 8 | Parity audit, Tauri removal, version 1.0.0, docs, QA |

### The new workspace (from Phase 1)

The gpui-kit application is a Cargo workspace at the repository root (root `Cargo.toml`, one `Cargo.lock`, one `target/`; `exclude = ["src-tauri"]`):

- `crates/bme-core` — UI-free: the config file (0.3.0's schema plus `window.display`, on a loader that keeps an unreadable file as `*.json.bad` and saves atomically), the per-drive extract queue (`extract.rs`: `ExtractQueue` with an injectable launcher, `ExtractEvent::{Started, Progress, Finished}` into a sink, `mkvextract --gui-mode`, exit 1 completes with warnings), `mkvtoolnix.rs` (tool resolution and detection without config writes, `list_tracks` = `mkvmerge -J` + the pure `parse_identify_json`, `spawn_mkvextract` with stdout and stderr in one pipe), `services.rs` (`.mkv` intake with skipped paths, output directory checks and `ensure_output_path`, BetterMediaInfo), `update.rs` (GitHub `/releases/latest`, the startup policy) and `protocol.rs` (`MkvTrack`, `TrackListError`, the tool statuses). No GUI toolkit and no async runtime. Tested through fake launchers, fake tool scripts (Unix) and a local HTTP stub; the whole crate is held to ≥ 90 % line coverage.
- `crates/bme-platform` — BatchMkvMerge's per-OS integrations: taskbar / Dock / launcher progress, the display of a window.
- `crates/bme-app` — package `BatchMkvExtract`, library `batch_mkvextract` plus a three-line binary (`#![windows_subsystem = "windows"]` unconditionally). gpui-kit is pinned at `=0.10.0` (its `test-support` feature only as a dev-dependency). Until Phase 3 it is a skeleton window titled `BatchMkvExtract v<version>`. Packaging metadata is in its `Cargo.toml` (`[package.metadata.packager]`, `[package.metadata.generate-rpm]`) and `packaging/` (icons, desktop file, `Info.plist`, the WiX template pinning 0.3.0's UpgradeCode).

The toolchain is pinned in the root `rust-toolchain.toml` (1.100.0); `.rustfmt.toml` sits at the root too. The CI workflows build only the workspace: `actions-rust-lang/setup-rust-toolchain` without a `toolchain:` input (so the pin is used) and with `build-warnings: warn`, tests, the `bme-core` coverage gate on Linux, `--profile dist` builds and the seven installers; every action is pinned to a commit SHA.

```bash
cargo run --release -p BatchMkvExtract                   # run the app
cargo check --release --workspace                        # fast Rust type-check without linking
cargo test --release --workspace                         # run the whole test suite
cargo llvm-cov --release -p bme-core --summary-only --fail-under-lines 90   # the CI coverage gate
cargo build --profile dist -p BatchMkvExtract            # installer build: release + thin LTO
cargo packager --profile dist -p BatchMkvExtract -f app,dmg   # or nsis,wix / deb,appimage (cargo-packager 0.11.8)
cargo generate-rpm --profile dist -p crates/bme-app      # .rpm on Linux (cargo-generate-rpm 0.21.1)
```

## Common commands

The Tauri app's commands, kept for reference; the app is no longer built:

```bash
pnpm install                             # install JS dependencies
pnpm tauri dev --release                 # run the desktop app with frontend + Rust hot-reload (release Rust build)
pnpm tauri build                         # build installers for the current platform
pnpm build                               # tsc + vite build (type-check + frontend bundle only)
```

Version bumps are done with a Deno script — edit the two arguments at the bottom of `scripts/ts/change-version.ts` and run:

```bash
cd scripts/ts && deno task change-version
```

That rewrites `crates/bme-app/Cargo.toml`, `crates/bme-core/Cargo.toml`, `crates/bme-platform/Cargo.toml`, all three `.github/workflows/*.yml` files and the `CFBundleVersion` of `crates/bme-app/packaging/macos/Info.plist`. It then runs `cargo update -w` (so `Cargo.lock` records the new workspace versions) and warns when `docs/release_notes.md` has no `## <version>` section yet.

## Platform-specific notes

- Development here happens on Windows with Git Bash. Use forward-slash paths (`C:/…` not `C:\\…`), `/dev/null` not `NUL`, and avoid `cd <project>` before `git` commands — git already runs against the working tree and prepending `cd` triggers an extra permission prompt.
- MKVToolNix (`mkvmerge`, `mkvextract`) must be installed separately — the app shells out to them.

## Architecture

### Frontend ↔ Backend split

React app under `src/` talks to a Rust Tauri v2 backend under `src-tauri/`. All communication goes through Tauri's `invoke` (commands) and the event system — no other IPC. Everything crossing the boundary is JSON; `src/protocol.ts` mirrors `src-tauri/src/protocol.rs` and must stay in sync (including the serde `#[serde(rename = "camelCase")]` attributes vs. the TS interface fields).

### Single source of truth: Zustand store

`src/store.ts` owns:
- the dropped file list,
- the extraction queue (grouped per-drive on Windows — single bucket on Linux/macOS), with per-item status, progress, timestamps, cancel flag, and error,
- the persisted config (theme, language, profiles, mkvtoolnix path, window geometry),
- a registry of per-card extract handlers and selection flags that the Toolbar consumes for Extract All.

**Important:** `updateConfig` applies the patch optimistically and discards the backend's response. Don't re-add a post-await `set({ config: saved })` — it re-introduces a race condition where rapid typing reverts edits (older async responses land on top of newer optimistic state). The backend also doesn't transform the config inside `set_config`, so the response would be identical anyway.

### Status state machine

`QueueItemStatus` is a string enum in `src/protocol.ts`: `Waiting | Extracting | Completed | Cancelled | Failed`. Backend snapshots only ever carry `Waiting` / `Extracting`; terminal states are set on the frontend:

1. The backend emits an `extraction-finished` event when the worker exits, carrying the authoritative outcome (`Completed`, `Cancelled`, or `Failed`) — `FileList.tsx` listens and calls `recordFinishedOutcome` which sets the status directly.
2. As a fallback, `applyExtractSnapshot` transitions any item that's disappeared from the backend snapshot to `Completed` (or `Cancelled` if `cancelRequested` was flagged). This handles races where the event arrives after the next poll.
3. The terminal-status check in `applyExtractSnapshot` skips items already in a terminal state, so the event-driven update is never overwritten.

Polling runs every **200 ms** from `FileList.tsx` via `get_extract_status`.

### Per-drive extraction queue (backend)

`src-tauri/src/extract.rs` owns all live extraction state via a module-level `OnceLock<Mutex<ExtractState>>`:

- `tasks: HashMap<file, TaskState>` — metadata (status, progress, args, cancel flag).
- `drives: HashMap<drive_key, DriveState>` — per-drive `extracting + queued`.
- `children: HashMap<file, Arc<Mutex<Child>>>` — kept separately so `cancel()` can `kill()` from a different call site.

`get_drive_key` returns the Windows path `Prefix` component (`C:`, `\\server\share`) uppercased, else `"default"`. When a file is enqueued and the drive's `extracting` slot is free, it's promoted immediately and a worker is `std::thread::spawn`'d. When a worker finishes, `on_worker_finished` emits the event, drops the task, and pops the next file for the same drive.

The tokio runtime is intentionally capped at 4 workers (see `lib.rs` → `tauri::async_runtime::set(runtime.handle().clone())` with `worker_threads(4)`). The actual extraction work runs on dedicated `std::thread::spawn` threads, not on those 4 workers, so the polling endpoint stays responsive under load.

### Profiles and filename templates

Profiles live in the persisted config (one `ConfigProfile` per entry, with a shared `activeProfile` pointer). Each profile carries three templates (video / audio / subtitle) and three auto-select flags. The `Default` profile auto-selects subtitle tracks.

Templates are expanded by `extract-utils.ts::renderTemplate`, a **single-pass character scanner** (not regex-based — don't regress this). It supports `{file_name}`, `{track_id}`, `{track_number}`, `{language}`, `{codec_name}`, `{track_name}`. `{{` / `}}` escape to literal braces; unknown placeholders are emitted verbatim so typos are visible. `{codec_name}` and `{track_name}` are sanitized for filesystem-unsafe characters before substitution.

The active profile is consumed both on track auto-select (once per card, guarded by `autoSelectedRef`) and at extract time (passed into `buildExtractArgs` / `buildCommandString`).

### Window flicker

`tauri.conf.json` creates the main window with `visible: false`. The setup hook in `src-tauri/src/lib.rs` applies the stored position and size, then calls `window.show()` — this is why users don't see a default-geometry flash on startup. Don't flip `visible` back on.

### Config file location

Config is stored per-OS:
- Windows — `%APPDATA%\BatchMkvExtract\` in installed mode (exe under `%LOCALAPPDATA%` / `%ProgramFiles%` / `%ProgramFiles(x86)%`), next to the `.exe` in portable mode.
- Linux — `$XDG_CONFIG_HOME/BatchMkvExtract/`, else `$HOME/.config/BatchMkvExtract/`.
- macOS — `~/Library/Application Support/BatchMkvExtract/`.

Detection lives in `src-tauri/src/config.rs::get_config_dir`. Old config files from earlier schema versions still load because every new field has `#[serde(default = "...")]`.

### i18n

Nine locales (`de`, `en-US`, `es`, `fr`, `it`, `ja`, `zh-CN`, `zh-HK`, `zh-TW`). When adding a user-facing string, add the key to **all nine** files under `src/i18n/locales/`. Missing keys fall back to `en-US` via i18next, but that's not a design pattern to lean on.

### External tools

Two optional external binaries are integrated, both opt-in via Settings:

- **MKVToolNix** (`mkvmerge`, `mkvextract`) — required for the core extraction flow; the app shells out to them.
- **BetterMediaInfo** — optional; when its path is configured, the "Open in BetterMediaInfo" entry becomes available on each card.

Both paths live under `config.externalTools` (`mkvToolNixPath`, `betterMediaInfoPath`). Backend probe commands return `MkvToolNixStatus` / `BetterMediaInfoStatus` (`src-tauri/src/protocol.rs`) with auto-detected directories — see `controller.rs` for the per-OS search paths (Windows registry / `Program Files`, macOS `/Applications`, Linux `$PATH`). The shared **`src/components/settings/ExternalToolPathRow.tsx`** + **`useToolPathDetection.ts`** drive the Browse/Detect UI for both tools — change them once and both settings rows update.

## Project conventions

- **Copyright headers** use `2026` only (e.g. `Copyright (c) 2026. caoccao.com Sam Cao`, `© Copyright 2026`). Don't reintroduce ranges like `2024-2026` — this project started in 2026; the range is a template-copy artifact from other caoccao projects.
- Status labels in the UI use **PascalCase enum values** (`Waiting`, `Extracting`, …). i18n keys that back them are lowercase (`queue.status.waiting`, …); the `statusLabel` helper in `Queue.tsx` bridges with `.toLowerCase()`.
- macOS `Stack` alignment must use `sx={{ alignItems: "center" }}`, not the `alignItems` prop — MUI v9 dropped the prop.
- When cards register their `handleExtract` into `fileExtractHandlers`, the signature is `() => Promise<void>`. The toolbar's Extract All awaits each handler sequentially so backend per-drive queue order matches the on-screen file order.
- All `if-else` must have braces.
