# Development

## Build rules

- **Always build and run the release profile. Never build a debug build.** Run a release build to validate changes before handing them off. Every cargo invocation in this repo, in docs and in CI carries `--release` (or `-r`): `cargo build --release`, `cargo run --release`, `cargo test --release`, `cargo check --release`, `cargo llvm-cov --release`. GPUI debug builds are slow to compile, slow to start and render at a fraction of release frame rates. Do not add `[profile.dev]` tuning as a workaround; use release.
- **Installers are built with the `dist` profile**, which inherits `release` and adds LTO, `codegen-units = 1` and symbol stripping: `cargo packager --profile dist`, `cargo generate-rpm --profile dist`. `dist` is a release build with slower, tighter optimisation; it is used for packaging only, never for the edit-test loop.

## Tech Stack

- Rust (edition 2024), a Cargo workspace: the UI-free `crates/bme-core` library, `crates/bme-platform` and the application `crates/bme-app` (package `BatchMkvExtract`)
- [gpui-kit](https://crates.io/crates/gpui-kit) 0.10.0 (exact pin; GPUI with the `gpui-pre` backend) for the user interface
- [rust-i18n](https://crates.io/crates/rust-i18n) 4.2 for localization (9 locales under `crates/bme-app/locales/`)
- `crates/bme-platform`: per-OS integrations GPUI lacks (taskbar / Dock / launcher progress through `ITaskbarList3`, `NSDockTile`, the `com.canonical.Unity.LauncherEntry` D-Bus signal)
- Dedicated worker threads for the `mkvextract` processes, which push `ExtractEvent`s to the app
- Installers: cargo-packager 0.11.8 (`.app`, `.dmg`, NSIS `.exe`, WiX `.msi`, `.deb`, `.AppImage`) and cargo-generate-rpm 0.21.1 (`.rpm`)

### External tools

- [MKVToolNix](https://mkvtoolnix.download/) — `mkvmerge -J` for the track listing, `mkvextract` for the extraction; required
- [BetterMediaInfo](https://github.com/caoccao/BetterMediaInfo) — optional; Open in BetterMediaInfo appears on the cards when it is found

## Tooling

- [Rust](https://www.rust-lang.org/) 1.100.0 — pinned in `rust-toolchain.toml` at the repository root
- [cargo-packager](https://crates.io/crates/cargo-packager) 0.11.8 and [cargo-generate-rpm](https://crates.io/crates/cargo-generate-rpm) 0.21.1 — installers (`cargo install --locked --version …`)
- [cargo-llvm-cov](https://crates.io/crates/cargo-llvm-cov) — coverage gate
- [Deno](https://deno.com/) — scripts under `scripts/ts/` (e.g. `change-version.ts`)

## Repository layout

- `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.rustfmt.toml`: Cargo workspace root (members `crates/bme-core`, `crates/bme-platform`, `crates/bme-app`; one `target/`)
- `crates/bme-app/`: the application (`src/`; `src/domain/` holds the UI logic without GPUI: queue states, Group Mode's grouping, time and count labels, the localised extraction and track-listing errors, numeric word navigation; `src/state/` the config, file, queue and UI stores, with the track listing four files at a time, the profile's auto-selection, group cards' shared selection and the profile edits (add, delete, reset, switch); `src/extract_events.rs` the bridge from the core's extract queue; `src/notifications.rs` the toast of every finished extraction and the notice sent once the queue drains; `src/actions.rs` enqueue, copy commands, cancel, resume, delete, clear and Clear Completed across stores, and BetterMediaInfo detection; `src/tools.rs` the MKVToolNix and BetterMediaInfo detection behind the Integration settings; `src/updates.rs` the startup update check and Skip this version; `src/ui/` the screens and shortcuts, with the file and group cards in `src/ui/cards/`, the read-only track table in `src/ui/track_table.rs`, the output path dialog in `src/ui/output_dialog.rs`, the Queue tab in `src/ui/queue/`, the read-only path text of the cards and queue rows, which can be selected and copied, in `src/ui/selectable_path.rs`, the topmost completion window in `src/ui/topmost.rs`, the Settings dialog (gpui-kit's `Settings` component: pages, the profile editor, the tool path rows) in `src/ui/settings/`, the update banner in `src/ui/update_banner.rs`, the toolbar with its overflow and Profile menus in `src/ui/toolbar.rs` and the numeric word stops of every text input in `src/ui/word_navigation.rs`; `src/platform/` window geometry and title, close guard, macOS menus, file dialogs and the taskbar progress; `src/i18n.rs` and `src/theme.rs`), its locale files (`locales/`), theme sets (`themes/`), embedded images and toolbar artwork (`assets/`) and installer inputs (`packaging/`: icons, desktop entry, the macOS `Info.plist` overlay, WiX template)
- `crates/bme-platform/src/`: taskbar / Dock / launcher progress per OS, the display of a window
- `crates/bme-core/src/`: UI-free core (config, extract queue and `ExtractEvent`s, services, MKVToolNix, update check, the result types of `protocol.rs`; the `mkvextract` arguments, output file names and copyable command in `extract_args.rs`)
- `crates/bme-core/tests/`: `extract_args_golden.rs` with its cases in `tests/golden/*.json`
- `scripts/ts/`: maintenance scripts (including multi-file version bumping)

## Commands

```bash
cargo run --release -p BatchMkvExtract                         # run the app
cargo check --release --workspace                              # fast Rust type-check
cargo test --release --workspace                               # run all Rust tests
cargo llvm-cov --release -p bme-core                           # line coverage of the core crate (CI gate: 90 %)
cargo build --profile dist -p BatchMkvExtract                  # installer build (release + thin LTO, stripped)
cargo packager --profile dist -p BatchMkvExtract -f app,dmg    # macOS; nsis,wix on Windows; deb,appimage on Linux
cargo generate-rpm --profile dist -p crates/bme-app            # .rpm on Linux
```

Installers land in `target/packager/` (`target/generate-rpm/` for the `.rpm`).

The Linux dependencies in `crates/bme-app/Cargo.toml` come from two sources. `dpkg-shlibdeps` on the `ubuntu-22.04` `dist` binary gives the libraries it links (`libc6 (>= 2.35)`, `libgcc-s1`, `libxcb1`, `libxkbcommon0`, `libxkbcommon-x11-0`; rpm adds the same ones by soname). The rest are what GPUI loads at run time, so no tool sees them: the Vulkan loader, the Wayland client, EGL and wayland-egl (wgpu's GL fallback), fontconfig's `/etc/fonts/fonts.conf` (read by a pure-Rust parser) and Noto Color Emoji. The `.rpm` names them as capabilities (`libvulkan.so.1()(64bit)`, …), so they resolve whichever package provides them. Re-derive the list after a gpui-kit bump: list the binary's `NEEDED` entries and the `lib*.so*` names in it, and run `dpkg-shlibdeps -O` in an `ubuntu:22.04` container.

The twenty colour themes in `crates/bme-app/themes/` are derived from their main colours in `crates/bme-app/src/theme.rs` (`THEMES`); the test `every_theme_file_is_derived_from_its_main_colour` fails when a file differs. After changing a colour, rewrite the files with:

```bash
BME_WRITE_THEMES=1 cargo test --release -p BatchMkvExtract --lib theme::
```

gpui-kit's own strings that the app shows (the settings search and Reset All, the select placeholder and search box, the input menu, the dialog buttons) are translated in the `gpui_component` part of every locale file.

In every text field, Ctrl+Left/Right (Option on macOS), with Shift, and the word deletions treat each part of a number as a word: `Movie.2024.1080p` stops at 5, 6, 10 and 16 going right. In a path field (the output path dialog, the Integration tool paths, the profile's file name templates) every `.` between two letters or digits separates words too: `Movie.2024.1080p.mkv` stops at 5, 10, 16 and 20. Other text without digits moves as gpui-kit's inputs do. The stops live in `crates/bme-app/src/domain/word_navigation.rs`.

To try the app by hand on macOS or Linux without touching your settings, start it with a scratch home, because the config directory follows `$HOME` (on Linux only while `XDG_CONFIG_HOME` is unset): `HOME=/tmp/bme-home cargo run --release -p BatchMkvExtract`.

To bump the version across the three `crates/*/Cargo.toml`, the three workflow files and the macOS `CFBundleVersion` (`crates/bme-app/packaging/macos/Info.plist`), edit both arguments at the bottom of `scripts/ts/change-version.ts` and run:

```bash
cd scripts/ts
deno task change-version
```

The script then runs `cargo update -w`, so `Cargo.lock` records the new workspace versions, and warns when `docs/release_notes.md` has no section for the new version yet.

## Configuration storage

The persisted config file is named `BatchMkvExtract.json` and lives at:

- **Windows** — `%APPDATA%\BatchMkvExtract\` when installed under `%LOCALAPPDATA%` / `%ProgramFiles%` / `%ProgramFiles(x86)%` / `%ProgramW6432%`; next to the `.exe` in portable mode.
- **Linux** — `$XDG_CONFIG_HOME/BatchMkvExtract/`, else `$HOME/.config/BatchMkvExtract/`.
- **macOS** — `~/Library/Application Support/BatchMkvExtract/`.

Each save writes a new file of its own, `BatchMkvExtract.json.<process id>-<counter>.tmp`, and renames it over the old one, so a crash never leaves a truncated config and two running instances never write into the same file. A config file that cannot be read, or that holds a value this version does not accept, never stops the app: every value that can still be read is kept, the rest goes back to its default, the original file is kept as `BatchMkvExtract.json.bad` (or `BatchMkvExtract.json.2.bad` and so on when that name is taken, so no backup is ever replaced, also not by a second instance repairing the same file), and a toast says so at startup. Before it saves a repaired or completed config at startup, and before it moves a file it could not read, the app reads the file again: when another instance has replaced it meanwhile, it loads that newer file instead of saving over it or moving it.

The same directory holds the log, `BatchMkvExtract.log` (the previous run's is `BatchMkvExtract.previous.log`). The app's own crates log from `info` up, other crates from `warn` up, and panics are logged as well; the log is also written to stderr when the app has a console.

## Concurrency

- Extraction scheduling is per drive:
	- each drive has at most one active `mkvextract` worker,
	- additional items for that drive are queued FIFO.
- Different drives (drive letters or shares on Windows) can extract in parallel; Linux and macOS have one queue.
- The workers push `ExtractEvent`s (`Started`, `Progress`, exactly one `Finished` per file) through a channel to the app's queue store; nothing polls.
- `mkvmerge -J` runs on the background executor for at most four files at a time.

## Template engine

`crates/bme-core/src/extract_args.rs::render_template` is a single-pass character scanner:

- `{{` / `}}` escape to literal braces
- `{placeholder}` replaces with the corresponding value
- Unknown placeholders are emitted verbatim (e.g. `{typo}` stays `{typo}`)

Supported placeholders: `{file_name}`, `{track_id}`, `{track_number}`, `{language}`, `{codec_name}`, `{track_name}`. `{codec_name}` and `{track_name}` are sanitized for filesystem-unsafe characters before substitution. The extension comes from the row's codec, and the name is joined to the output directory without touching the disk, so a template's leading `/` stays inside it.

## CI

Three GitHub Actions workflows build the app on push/PR and upload its installers as seven artifacts named `BatchMkvExtract-<os>-<arch>-<version>`, with `-<format>` on Linux and Windows:

- `linux_build.yml` (`ubuntu-22.04`, for a low glibc floor) — `.deb`, `.AppImage` (cargo-packager), `.rpm` (cargo-generate-rpm)
- `macos_build.yml` — `.dmg` (x86_64 on `macos-15-intel` and arm64 on `macos-15`)
- `windows_build.yml` — `.msi` (one per language) and NSIS `.exe`

Each installs the Rust toolchain pinned in `rust-toolchain.toml` (1.100.0; the setup action gets no `toolchain:` input, so nothing overrides the pin, and `build-warnings: warn`, so warnings do not fail the build), runs `cargo test --release --workspace --locked`, builds `BatchMkvExtract` with `--profile dist` and packages it. Linux also installs GPUI's build dependencies and Mesa's Vulkan drivers.

Linux additionally enforces ≥ 90 % line coverage for the `bme-core` crate (`cargo llvm-cov --release -p bme-core --locked --summary-only --fail-under-lines 90`); that step runs the core's tests, so the Linux test step leaves `bme-core` out (`--exclude bme-core`). A separate Linux job, `lint`, runs `cargo fmt --all --check` and `cargo clippy --release -p bme-core --all-targets`; it reports and never fails the workflow (`continue-on-error`).

Every workflow runs with `permissions: contents: read`, and every action is pinned to a commit SHA with its release tag in a comment; `.github/dependabot.yml` proposes updates monthly.

## Release checklist

Run it before tagging a release, on each OS, with the `dist` installers that CI built from the release commit (the workflow artifacts), not with a local `cargo run`. The app checks can use a scratch home (`HOME=/tmp/bme-home`, see above) or a test account, so they do not touch your own settings; the install and upgrade checks use the real install locations.

Before:

- [ ] The three workflows are green on the release commit and upload seven artifacts whose names carry the new version.
- [ ] The version is bumped with `deno task change-version`, and `docs/release_notes.md` has its `## <version>` section.
- [ ] After a gpui-kit bump since the last release: the Linux dependencies are re-derived (see "Commands").

Install, upgrade and uninstall:

- [ ] Windows: the NSIS `-setup.exe` installs for the current user and starts from the Start Menu; run over the previous release, it replaces it (one entry in Apps); uninstalling removes it. The same for an `.msi` (all users; a major upgrade removes the previous MSI install). The app also starts on a machine without WebView2.
- [ ] macOS, on Intel and on Apple silicon: the `.dmg` opens without a prompt; the app dragged over the previous release in Applications replaces it and starts.
- [ ] Linux: `sudo apt install ./<file>.deb` on Ubuntu 22.04 and `sudo dnf install ./<file>.rpm` on Fedora install, and upgrade the previous release (one package, the new binary); the `.AppImage` starts; the app runs in an X11 and in a Wayland session.
- [ ] A config written by the previous release loads unchanged: theme, language, profiles (templates, auto-selection, language filters, default group mode), tool paths and window geometry.

In the app, on each OS:

- [ ] Files and folders dropped onto the window, chosen with Add Files or Add Folder, or given on the command line become cards; a missing path is named in a snackbar; the same file added again is listed once.
- [ ] Track table: a click on a row toggles it, select-all shows the mixed state, the chapters and attachment rows, the profile's auto-selection when the tracks arrive.
- [ ] Group Mode (toolbar, F8): files of a folder with the same track counts form a group card; its table selects for every file; the splitter resizes the file list.
- [ ] Set Output Path on a card and a group card, and the toolbar's global output path: the warning for a missing folder, an unwritable folder refused; Extract creates the folder.
- [ ] Copy Command and Copy All Commands: the copied command runs in the platform's shell and writes what Extract writes; Open in BetterMediaInfo when it is installed.
- [ ] Extract, Extract All (F3), Cancel on the card and in the queue, Cancel All (F4), Resume, Resume All, Clear Completed, Delete, Clear (Ctrl+Q); on Windows, files on two drives extract in parallel, one at a time per drive.
- [ ] While extracting: the card's progress row with elapsed time and ETA, the title's `- done/total`, and the taskbar, Dock or launcher progress, which clears afterwards; a toast for each finished file.
- [ ] With both queue toggles on, a finished queue sends one system notification (the first one may ask for permission) and opens the topmost window once, above other applications; Enter or Escape closes it. With Quit when complete on, the app quits after the last job, and the topmost window offers Quit and `OK, but do not quit`.
- [ ] Closing the window (or Cmd+Q on macOS) while an extraction runs asks first; Enter and Escape answer No; Yes stops the extractions and quits, with no `mkvextract` left.
- [ ] Settings (the toolbar, F10, Ctrl+Comma, Cmd+Comma or the app menu on macOS): every page; Browse and Detect for MKVToolNix and BetterMediaInfo, also while each is running; adding, switching, deleting and resetting a profile; the templates, the auto-selection and the language filters; Default group mode.
- [ ] The Profile menu (F9), Ctrl+Tab / Ctrl+Shift+Tab, Ctrl+1…9 and Ctrl+W on the tabs.
- [ ] Every theme in light, dark and auto (auto follows the OS appearance), every language, each switched live in Settings; the emoji track icons and Japanese and Chinese text render; the Extract All and Group Mode toolbar artwork reads at its size and tints (primary colour while Group Mode is on).
- [ ] In a text field, the word keys (Ctrl+Arrow, Option+Arrow on macOS, with Shift, and the word deletions) stop at each part of a number; in a path field and a template field also at a file's extension; also when pressed quickly, after which the window still takes keys.
- [ ] The window reopens at its saved size and position; a maximized window keeps its restore size. With a second display at another scale, it reopens on the display it was closed on.
- [ ] The update banner appears for a newer GitHub release, and Skip this version hides it for that version.
