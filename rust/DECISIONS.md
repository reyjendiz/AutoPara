# Decisions

Each entry: what was decided, the alternatives, and why. Differences from the Python app's behaviour
(`PORT_SPEC.md`) are recorded here too.

## Frontend without a UI framework

- **Decision:** vanilla TypeScript with one helper, `h(tag, attributes, ...children)` (`ui/src/dom.ts`),
  built by Vite.
- **Alternatives:** Svelte, Solid, React, Lit.
- **Why:** the interface is a handful of screens whose state lives in the database, as in the Python
  app ("the database is the single source of truth"; every change re-renders from it). A framework
  would add a runtime and a build plugin for what a small render function per view does.

## Typed IPC with ts-rs

- **Decision:** types that cross the IPC boundary derive `ts_rs::TS` and export into
  `ui/src/bindings/` when `cargo test` runs; the bindings are committed and `cargo xtask check`
  fails if a test run changes them. Commands are called through small typed wrappers in
  `ui/src/ipc.ts`.
- **Alternatives:** `specta` with `tauri-specta`, which also generates the command wrappers.
- **Why:** ts-rs is stable and has no dependency on Tauri's version; `tauri-specta` tracks Tauri
  releases with release candidates. Writing one wrapper line per command is cheap.

## One string table for both sides

- **Decision:** `crates/autopara-core/i18n/uk.json` is the only place user-visible text lives. Rust
  embeds it (`i18n::t`), the frontend imports the same file (`ui/src/i18n.ts`, keys type-checked).
- **Why:** rule R2 asks for one table; a JSON file both toolchains read needs no generator.

## The version lives in the workspace manifest

- **Decision:** `[workspace.package] version` in `rust/Cargo.toml`. `tauri.conf.json` has no
  `version` field, so Tauri takes the app crate's (inherited) version.
- **Alternatives:** write the number into `tauri.conf.json` as well and keep the two in step.
- **Why:** one number, nothing to mirror (plan 6.5 asks for a single place).

## Cargo-installed Tauri CLI

- **Decision:** builds use `cargo tauri` (the `tauri-cli` crate). `@tauri-apps/cli` stays in
  `package.json` for `npm run tauri` on machines where it works.
- **Why:** on the development machine the npm CLI's native binary does not find the Rust toolchain
  (`tauri info` reports rustc and cargo as missing while both are on `PATH`), so it cannot build.
  The plan names `cargo tauri` anyway.

## Intel macOS CI runner

- **Decision:** `macos-15-intel` instead of the plan's `macos-13`.
- **Why:** GitHub retired the `macos-13` image; `macos-15-intel` is the Intel image the existing
  installer workflow already uses.

## Smoke build without bundling in CI

- **Decision:** CI runs `cargo tauri build --debug --no-bundle`.
- **Why:** it proves the app compiles and links with the frontend embedded on every runner. Bundling
  (NSIS, `.dmg`) is phase 6's job and would make every CI run download the bundlers.

## MPL-2.0 crates under Tauri

- **Decision:** `deny.toml` allows MPL-2.0 for exactly five crates: `cssparser`, `cssparser-macros`,
  `selectors`, `dtoa-short` (all via `tauri-utils` → `dom_query`) and `option-ext` (via `dirs`).
- **Alternatives:** none that keep Tauri 2; the plan's licence list (R11) does not name MPL.
- **Why:** MPL-2.0 is file-level copyleft, not GPL: using the crates unmodified only obliges us to
  point at their source, which is on crates.io. The exception is per crate, so any new MPL
  dependency still fails `cargo deny`. The frontend toolchain has one MPL package, `lightningcss`
  (Vite's CSS processor), which runs at build time and is not shipped.

## Data folder

- **Decision:** the Rust app keeps its data in the Python app's folder (`%APPDATA%\AutoPara`,
  `~/Library/Application Support/AutoPara`) under names of its own: `autopara2.db` and
  `timetables/`. The Python app's `autopara.db` and `schedules/` are only ever read (step 2.7).
- **Alternatives:** Tauri's identifier folder (`%APPDATA%\com.autopara.app`); reusing
  `autopara.db` in place.
- **Why:** the installer's promise ("uninstall keeps `%APPDATA%\AutoPara` unless you ask") keeps
  covering everything, and the legacy files are never modified, so a user can still go back.

## Import uses the class-length setting

- **Decision:** the import passes `class_duration_minutes` to the parser.
- **Python:** `SetupDialog._load` calls `parse_file(path)` with the 80-minute default although
  Settings says the length applies to future imports (PORT_SPEC §I.8).
- **Why:** the setting's own wording; with the default (80) the result is identical.

## ASCII digits in time and pair cells

- **Decision:** times and pair numbers are read from ASCII digits only.
- **Python:** `\d` and `str.isdigit` also accept other Unicode digits (and `int()` then fails on
  some of them, such as superscripts).
- **Why:** no timetable seen uses them; the golden documents parse identically.

## Timestamps come from the caller

- **Decision:** `autopara-core` never reads the clock; `import_courses`, `claim_occurrence` and
  friends take `now`. Phase 3's `Clock` supplies it.
