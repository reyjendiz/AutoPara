# Contributing

Guidance for working on this repository: the commands, the build environment and the invariants
that must not be broken. The desktop app is the Rust + Tauri 2 workspace in `rust/`; the Python
1.x app (`autopara/`, `tests/`, `installer/`) stays in the tree for reference and its tests, and
`android/` is the Android app.

## Living documentation — read before coding

`docs/ARCHITECTURE.md`, `docs/BACKEND.md` and `docs/FRONTEND.md` are the canonical design record and
must be **read before starting any coding task** and **updated in the same commit** as any change to
architecture, data model, UI structure or behavior. A choice with alternatives, and every difference
from the 1.x behaviour (`rust/PORT_SPEC.md`), goes into `rust/DECISIONS.md`. Stale docs are treated
as a bug in this project. Keep them concise and current rather than appending changelogs.

## Commands

From `rust/`:

```bash
cargo xtask check          # the whole gate: fmt, clippy -D warnings, tests, typecheck, lint, vitest, deny
cargo test --workspace     # Rust tests only
cargo test -p autopara-core --test scheduler   # one test target
cargo xtask golden         # parity with the 1.10.0 parser and scheduler (rust/fixtures/golden)
cargo xtask demo-data      # regenerate ui/src/demo/sample.json from the core
cargo xtask screenshots    # photograph the demo scenes into target/screenshots (rust/PARITY_UI.md)
cargo xtask bench-stt <wav> [model.bin]   # speech benchmark (docs/ASSIST_BENCH.md)
```

From `rust/crates/autopara-app`:

```bash
npm ci                                  # once
cargo tauri dev                         # run the app with the Vite dev server
cargo tauri build --debug --no-bundle   # a debug executable, no installer
cargo tauri build                       # the release installer (needs the updater key, see below)
npm run dev                             # the interface alone; open http://localhost:5173/?demo
```

**Set `AUTOPARA_DATA_DIR` to a scratch folder whenever you run a checkout.** The default data folder
is the real one (`%APPDATA%\AutoPara`), which an installed AutoPara — 1.x or 2.x — is using.

### Build environment (Windows)

- Rust stable with the MSVC toolchain, Node.js LTS, the Visual Studio Build Tools (C++), NSIS for
  installers.
- whisper.cpp is compiled by `whisper-rs`: set `LIBCLANG_PATH` to a folder with `libclang.dll` and
  put CMake on `PATH`. CMake from the Build Tools works; add it by its 8.3 short path
  (`C:\PROGRA~2\MICROS~2\2022\BUILDT~1\Common7\IDE\COMMON~1\MICROS~1\CMake\CMake\bin`), because the
  long path with spaces breaks `npm.cmd` inside `cargo xtask check`.
- `rust/.cargo/config.toml` forces `-O2` for whisper.cpp's C/C++ code: CMake's Visual Studio
  generator otherwise builds it unoptimised and speech recognition falls behind the audio.
- `cargo tauri build` signs the updater files and stops without `TAURI_SIGNING_PRIVATE_KEY` (the key
  file's path or contents); `--no-bundle` skips that.

### Test environment

- Tests never touch the network, never open a browser and never wait on real time: the scheduler
  takes a `Clock`, the opener is a recorder, and `autopara-core` never reads the clock itself.
- Nothing from the real timetable is committed (it holds live meeting links). The golden fixtures
  are built from invented timetables.
- Lecture Assist tests use synthetic audio and need no speech model; real speech is measured by
  `cargo xtask bench-stt` (docs/ASSIST_BENCH.md), not by the test suite.
- The Python suite (`python -m pytest` from the repository root) still runs against the 1.x code;
  keep it until the parity report (`rust/PARITY_REPORT.md`) is accepted.

## Code rules

`RULES.md` is short and binding: no `unsafe`, no `unwrap`/`expect` outside tests, `tracing` instead
of `println!`, no new dependency without a reason, and never log API keys, audio or transcript text.
Every dependency's licence is listed in `rust/THIRD_PARTY.md` and checked by `cargo deny`.

## Architecture

```
.docx -> autopara-core/docx.rs      zip + XML -> cell grid with merges and link targets
      -> autopara-core/parser.rs    grid -> courses, groups, lessons (the domain rules)
      -> autopara-core/db.rs        SQLite in %APPDATA%\AutoPara\autopara2.db
      -> autopara-app (commands)    typed IPC -> ui/src renders the selected group's period
      -> autopara-core/engine.rs    wakes at the next due moment (<= 15 s) -> scheduler.rs -> browser
```

**The database is the single source of truth.** The UI caches no lesson state; every mutation is
followed by a reload from storage.

### Invariant: the interface is Ukrainian

Every user-visible string — the interface, the tray, notifications and the installer — is a key in
`rust/crates/autopara-core/i18n/uk.json`. Rust reads it through `i18n::t`, the interface through
`ui/src/i18n.ts` (keys type-checked), the installer through strings `build.rs` generates. Code,
comments and docs stay English.

**Nothing on the left-hand rail is written.** Every rail button is an icon with a Ukrainian tooltip;
the app mark stands in for the word "AutoPara". A label would put a word back on screen and widen a
rail whose width is one button plus air.

The importer reads documents in other languages and converts what it finds (days, course labels)
into Ukrainian. Subject and teacher text is copied verbatim — it is data, not chrome.

### Invariant: open each class exactly once

Enforced by the database, not by in-memory bookkeeping. `occurrences` has
`UNIQUE(lesson_id, occur_date)`, and the scheduler **claims the row before opening the browser**
(`Storage::claim_occurrence`); a conflict means it already fired, so the browser call is skipped.
Keying on `(lesson, date)` is what lets the same weekly class recur while a restart, sleep/wake or
clock change on the same day cannot double-open it. `tests/scheduler.rs` pins restarts, clock jumps,
shared classes, DST and two instances racing (`two_instances_racing_open_a_class_once`).

`scheduler::evaluate` is a pure function of the lesson, `now`, the lead time and what already fired,
so trigger boundaries are testable without a clock. `reminder_due` is a *second* pure function and
must stay separate: a reminder is a message, and folding it into `evaluate` puts it one keystroke
away from opening a browser.

### Invariant: a class that is already running is never opened unasked

After an app start, a wake from sleep or a clock change, a running class is offered by the
missed-class banner and opened only when the user presses `Підключитися зараз` — **whatever
`catchup_mode` says**. Mode `open` applies only to a class whose trigger passed while the scheduler
was watching on an unbroken clock (`BACKEND.md` §4). This fixes a real, reported bug — launching the
app after a missed morning opened a browser tab per still-running class — and 1.x's one-tick guard
still let it through 15 s later. `a_start_during_a_class_only_offers_it` and
`after_a_sleep_a_running_class_is_offered_not_opened_even_in_open_mode` keep it.

### Invariant: only https links are opened

Link text comes from a document, so `file:`, `javascript:` and plain `http:` values never reach the
system browser (`scheduler::is_openable`); the editor refuses them when typed.

### Invariant: installing replaces, never merges

The installer removes the previous program (including 1.x's `_internal` bundle) before copying.
`%APPDATA%\AutoPara` is the one thing a reinstall spares; uninstalling asks whether to delete it, and
No or `/S` keeps it.

### Invariant: an update comes from this repository, is signed, and is installed only on request

The updater reads `latest.json` from this repository's newest release and checks every file against
the minisign public key in `tauri.conf.json`. Downloading may be automatic (`update_mode = auto`);
**installing never is**. **Publishing a release is shipping an update** to every 2.x copy, and the
version lives in one place: `[workspace.package] version` in `rust/Cargo.toml`.

### Invariant: a timetable only applies from the moment it is imported

`import_courses` stamps `schedule_active_from`; `predates_schedule` skips anything that had already
started by then — not opened, not offered, not marked missed. Importing at the weekend to set up the
coming week must not paint that weekend red.

`Storage::mark_occurrence` is the one sanctioned way to write an occurrence row without claiming it
first: it records a decision the user made in the interface, and the row it writes is what stops the
scheduler acting on that class.

### Invariant: Lecture Assist only ever shows text, on this screen

No text-to-speech, no virtual microphone or camera, nothing that speaks or acts in the meeting,
nothing that hides the app or its recording indicator, and audio is never written to disk. See
`docs/ASSIST.md`.

### The .docx parser — rules that look wrong but are not

`docs/BACKEND.md` section 2 has the full list with evidence. The ones most likely to be "simplified"
into bugs by someone who hasn't seen the source document:

- **Shared classes are real `w:gridSpan` merges, not repeated text.** Word stores a session taught to
  several groups as *one* `<w:tc>` spanning columns. Comparing adjacent cell text is only a fallback
  (`dedupe_duplicates`) for documents from other tooling. Down the **rows** the opposite holds: a
  class retyped in the next time slot is the same session, and comparing that text is the primary
  strategy (`merge_consecutive_slots`, R12) — a double class must be one lesson, or the browser
  opens again mid-meeting. Its adjacency test is load-bearing: the same subject can legitimately run
  twice in one day with a gap.
- **Never index cells positionally.** Rows contain different numbers of `<w:tc>` (4, 5, 7) because of
  merges. A cell's column is the running sum of preceding `gridSpan` values.
- **A group owns a column *range*.** A header cell can itself span (course IV: 7 grid columns, 2
  groups). A lesson belongs to every group whose range it overlaps.
- **Links appear twice** — as a `w:hyperlink` relationship *and* as visible text. Collect both, dedupe.
- **The day column is usually empty**, carried down from a `w:vMerge` restart row.
- Text needs NFC normalization and apostrophe folding: the document writes `П` + **backtick** (U+0060)
  for Friday, and course headings mix Cyrillic `І` (U+0406) with Latin `V`. Match courses by ordinal
  position, never by heading string.

Any change to the parser must keep `cargo xtask golden` green, or record the difference in
`rust/DECISIONS.md` with a reason.

### Windows integration

- **Autostart** is the `HKCU` `Run` value `AutoPara` = `"<exe>" --hidden`, written by the app
  (`autostart.rs`) and by the installer in the identical format; `the_installer_writes_the_same_entry`
  compares them. If they diverge, the app silently rewrites the installer's entry on next launch.
  `autostart_enabled` defaults to `1` for the same reason: the default and the installer must agree.
- A debug build never writes the autostart entry, so a run from a checkout cannot point it into
  `target/`.
- Closing the window hides to tray; only the tray's quit exits. A second launch surfaces the
  running copy.
- The installer is per-user (`%LOCALAPPDATA%\Programs\AutoPara`, no admin/UAC) and Ukrainian only; its
  template is `src-tauri/installer/installer.nsi` and its strings come from `uk.json`.
- WebView2 is present on Windows 11 and updated Windows 10; when it is missing, the installer runs the
  embedded Evergreen bootstrapper, which needs the network once.
