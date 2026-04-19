# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Common commands

```bash
pnpm install                     # install JS dependencies
pnpm tauri dev                   # run the desktop app with frontend + Rust hot-reload
pnpm tauri build                 # build installers for the current platform
pnpm build                       # tsc + vite build (type-check + frontend bundle only)
cd src-tauri && cargo check      # fast Rust type-check without linking
```

Version bumps are done with a Deno script — edit the two arguments at the bottom of `scripts/ts/change-version.ts` and run:

```bash
cd scripts/ts && deno task change-version
```

That rewrites `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, and all three `.github/workflows/*.yml` files.

There is no test runner wired up; the CI workflows run `cargo test -r` but the crate has no tests yet. `pnpm build` is the primary smoke test while iterating.

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

Templates are expanded by `extract-utils.ts::renderTemplate`, a **single-pass character scanner** (not regex-based — don't regress this). It supports `{file_name}`, `{track_id}`, `{track_number}`, `{language}`, `{codec_name}`, `{track_name}`. `{{` / `}}` escape to literal braces; unknown tokens are emitted verbatim so typos are visible. `{codec_name}` and `{track_name}` are sanitized for filesystem-unsafe characters before substitution.

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

Eight locales (`de`, `en-US`, `es`, `fr`, `ja`, `zh-CN`, `zh-HK`, `zh-TW`). When adding a user-facing string, add the key to **all eight** files under `src/i18n/locales/`. Missing keys fall back to `en-US` via i18next, but that's not a design pattern to lean on.

## Project conventions

- **Copyright headers** use `2026` only (e.g. `Copyright (c) 2026. caoccao.com Sam Cao`, `© Copyright 2026`). Don't reintroduce ranges like `2024-2026` — this project started in 2026; the range is a template-copy artifact from other caoccao projects.
- Status labels in the UI use **PascalCase enum values** (`Waiting`, `Extracting`, …). i18n keys that back them are lowercase (`queue.status.waiting`, …); the `statusLabel` helper in `Queue.tsx` bridges with `.toLowerCase()`.
- macOS `Stack` alignment must use `sx={{ alignItems: "center" }}`, not the `alignItems` prop — MUI v9 dropped the prop.
- When cards register their `handleExtract` into `fileExtractHandlers`, the signature is `() => Promise<void>`. The toolbar's Extract All awaits each handler sequentially so backend per-drive queue order matches the on-screen file order.
- All `if-else` must have braces.
