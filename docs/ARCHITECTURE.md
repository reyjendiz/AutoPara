# ARCHITECTURE — Class Auto-Launcher ("AutoPara")

> Read this file, plus `BACKEND.md` and `FRONTEND.md`, before starting any coding task in this
> project. Update the relevant file(s) as part of any change that alters architecture, data model,
> UI structure, or behavior. **Stale docs are a bug.** Choices and every difference from the 1.x
> Python app are recorded in `rust/DECISIONS.md`; the 1.x behaviour itself is `rust/PORT_SPEC.md`.

## Purpose

A Windows and macOS tray application for a university student. It imports a weekly class schedule
from a `.docx` file, renders it as a day, week or month calendar, and opens each class's Zoom /
Google Meet link in the default browser a configurable lead time (default **1 minute**) before the
class starts. It launches with the system, runs quietly in the tray, and never opens the same class
occurrence twice. An opt-in module, Lecture Assist (`docs/ASSIST.md`), transcribes the class audio
locally and keeps notes on the user's own screen.

**The interface is entirely Ukrainian.** The source document may be in any language the importer
recognises (see `BACKEND.md` R10); everything the app generates from it -- day names, course
labels, every string on screen -- is Ukrainian, and every such string is a key in
`rust/crates/autopara-core/i18n/uk.json`. A literal string in code is a bug, not a detail.

## Tech stack and why

| Layer | Choice | Why |
|---|---|---|
| Logic | Rust (stable, edition 2024), no `unsafe` | One language for parsing, storage, scheduling, audio, speech and updates; a small native binary instead of a 126 MB Python bundle. |
| Shell | Tauri 2 | The system webview (WebView2 on Windows, WKWebView on macOS) instead of shipping a browser; tray, single instance, updater, notifications and dialogs as plugins. |
| UI | TypeScript, no framework, Vite | A handful of screens whose state lives in the database; one `h()` helper renders each view (DECISIONS "Frontend without a UI framework"). |
| Storage | SQLite through `rusqlite` (bundled) | The 1.x schema, so the old data imports with its ids. |
| IPC types | `ts-rs` | Rust types export to `ui/src/bindings/`; the check fails if they drift. |
| Speech | whisper.cpp through `whisper-rs` | Local, CPU or CUDA, no audio leaves the machine. |

## Workspace

```
rust/
  Cargo.toml                 workspace; [workspace.package] version is THE version
  crates/autopara-core/      model, .docx reader and parser, SQLite, scheduler, engine, strings
  crates/lecture-assist/     audio capture, VAD, speech recognition, LLM client, notes
  crates/autopara-app/
    src-tauri/               the Tauri binary: commands, tray, autostart, updates, assist wiring
    ui/src/                  the interface (main window and the assist overlay)
  xtask/                     cargo xtask check | golden | demo-data | screenshots | bench-stt
  fixtures/golden/           the 1.10.0 parser's and scheduler's output, for parity tests
```

`autopara-core` has no Tauri dependency and never reads the clock, so all of it is tested with plain
`cargo test` and a fake clock.

## Process model

One process. The Tauri main thread owns the windows and the tray; a tokio task (`engine.rs`) runs
the scheduler; Lecture Assist, when switched on, runs its capture and recognition on threads of its
own and shares nothing with the scheduler but the database, so an assistant failure never touches
joining. Two windows: `main` (the calendar) and `overlay` (the assist notes, hidden until used).

Closing the main window hides it to the tray; only the tray's quit item exits. A second launch
surfaces the first (`tauri-plugin-single-instance`).

## Data flow

```
.docx -> core/docx.rs       zip + XML -> cell grid with merges and link targets
      -> core/parser.rs     grid -> courses, groups, lessons (the rules in BACKEND.md §2)
      -> core/db.rs         SQLite in <data>/autopara2.db
      -> app commands.rs    typed IPC -> ui/src (renders the selected group's period)
      -> core/engine.rs     wakes at the next due moment (<= 15 s) -> scheduler.rs
      -> app opener.rs      the system browser
```

**The database is the single source of truth.** The UI caches no lesson state; every command that
changes something is followed by a reload, and the engine emits a `scheduler` event when it opens,
offers or records a class.

## The "fire exactly once" guarantee

The core correctness property of the app, enforced by the database, not by in-memory bookkeeping:

- `occurrences` has `UNIQUE(lesson_id, occur_date)`.
- The scheduler **inserts the row first, then opens the browser** (`Storage::claim_occurrence`). If
  the insert conflicts, the occurrence already fired and the browser call is skipped.
- Because the key is `(lesson, date)` rather than a subject name, the same weekly class recurring
  every week is a distinct occurrence each week, while an app restart, a sleep/wake cycle, or a
  clock adjustment on the same day cannot cause a second open.

## Scheduling and catch-up

`BACKEND.md` §4 has the table. The rule people try to simplify: **a class that is already running
is never opened unasked after a start, a wake from sleep or a clock change.** Catch-up mode `open`
applies only to a class whose trigger passed while the app was watching. Everything else shows the
missed-class banner and waits for a button.

## Storage location

| | Data | Webview and window state |
|---|---|---|
| Windows | `%APPDATA%\AutoPara` | `%APPDATA%\com.autopara.app`, `%LOCALAPPDATA%\com.autopara.app` |
| macOS | `~/Library/Application Support/AutoPara` | `~/Library/Application Support/com.autopara.app` |

The data folder is 1.x's (DECISIONS "Data folder"): `autopara2.db`, `timetables/`, `logs/` beside
the old `autopara.db` and `schedules/`, which are only ever read. `AUTOPARA_DATA_DIR` replaces the
folder; **always set it when running a checkout**, or the dev app uses your real data.

## Entry points

- `src-tauri/src/main.rs` → `lib.rs::run()`: plugins, data folder, logging, database, the 1.x
  import, retention purge, autostart sync, engine, tray, windows.
- `--hidden` starts in the tray with no window (what autostart passes).
- `cargo tauri dev` from `rust/crates/autopara-app` runs it with the Vite dev server.
- `?demo` in a browser (`npm run dev`, then `http://localhost:5173/?demo`) runs the interface alone
  on the sample timetable with a pinned clock, for screenshots and UI work.

## Autostart is on out of the box

`autostart_enabled` defaults to `1`, and every launch syncs the system entry to it. The installer
writes the identical `"<exe>" --hidden` string `autostart::startup_command()` produces, so the app
sees the installer's entry as already enabled instead of fighting it. 1.x once defaulted the setting
to `0` while its installer wrote the entry anyway, so the first launch deleted it; the default and
the installer have to agree. A debug build never writes the entry.

## Logging

`tracing` into `<data>/logs/autopara.<date>.log`, a week of files; a panic is written there before
the default hook runs. Settings → diagnostics copies the version, paths and the log's tail. API keys,
audio and transcript text are never logged.

## Security baseline

- Only `https://` links are opened, by the system browser (`scheduler::is_openable`; the editor
  refuses anything else).
- A strict CSP in `tauri.conf.json`; each window gets only the commands its capability file in
  `src-tauri/capabilities/` lists.
- Updates are signed (minisign) and fetched from this repository's releases only.
- API keys for Lecture Assist live in the OS keychain (`secrets.rs`), never in the database.

## Building

```
cd rust/crates/autopara-app
npm ci
cargo tauri build            # NSIS installer on Windows, .app + .dmg on macOS
```

On Windows whisper.cpp needs `LIBCLANG_PATH` and CMake on `PATH` (see `CONTRIBUTING.md`). Without
`TAURI_SIGNING_PRIVATE_KEY` the build stops at signing the updater files; `cargo tauri build
--no-bundle` builds just the executable.

### The installer that people download is built by CI

`.github/workflows/rust-release.yml` builds Windows, Apple Silicon and Intel on every push to `main`
that touches `rust/`, and publishes a release only when all three succeeded and `v<version>` has no
release yet. See `docs/RELEASING.md`.

## Installer

`src-tauri/installer/installer.nsi` is Tauri's NSIS template changed to do what 1.x's
`installer/AutoPara.nsi` did (DECISIONS "Windows installer"): per-user into
`%LOCALAPPDATA%\Programs\AutoPara`, no UAC, a components page (app, desktop shortcut, start with
Windows), Ukrainian only. Uninstall removes the program, shortcuts, autostart and registry keys and
then asks whether to delete `%APPDATA%\AutoPara`; No, `/S` and `/P` keep it. The template's strings
come from `uk.json` through `build.rs`.

**Installing replaces, never merges.** The install removes the 1.x bundle (`_internal`) before
copying, and an update (`/UPDATE`) replaces the program but leaves shortcuts and the autostart choice
alone. Upgrading from 1.x is described in `docs/UPGRADING.md`.

On macOS the bundle is `AutoPara.app` (identifier `com.autopara.app`, as 1.x) in a `.dmg`, unsigned
and not notarised.

## Updates

`BACKEND.md` §7. In short: `latest.json` from this repository's newest release, every file checked
against the public key in `tauri.conf.json`, downloading may be automatic, installing is always the
user's click.

## Testing

`cargo xtask check` from `rust/` is the whole gate: `cargo fmt --check`, `clippy -D warnings`,
`cargo test --workspace` (which also re-exports the TypeScript bindings and fails if they changed),
`npm run typecheck`, `npm run lint`, `npm test`, and `cargo deny check`. Parity with 1.x is pinned by
`tests/golden.rs` (parser) and `tests/golden_scheduler.rs` (72 scheduler simulations) against
`rust/fixtures/golden/`. CI (`rust-ci.yml`) runs the gate on Windows, Apple Silicon and Intel macOS, with `cargo deny` on
Linux.
