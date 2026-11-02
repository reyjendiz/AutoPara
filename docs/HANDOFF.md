# HANDOFF — state of the project for whoever picks it up next

What exists, how to run and verify it, what is deliberately unfinished, and the traps already found.
The design record proper is `ARCHITECTURE.md`, `BACKEND.md` and `FRONTEND.md`; `CONTRIBUTING.md`
lists the invariants. Read those first — this file does not repeat them.

## Where things stand

- **Repository:** https://github.com/reyjendiz/AutoPara (the old name `sevcenkoa864-oss/AutoPara`
  redirects). The latest published release is **v1.10.0**, the Python app.
- **2.0.0 is the Rust + Tauri 2 rewrite** on the local branch `rust-rewrite`, not pushed yet. It
  reproduces the 1.x desktop app (`rust/PARITY_REPORT.md`) and adds Lecture Assist
  (`docs/ASSIST.md`). The step-by-step status is `rust/PROGRESS.md`.
- **Merging `rust-rewrite` into `main` publishes 2.0.0** (`rust-release.yml`): the version is
  already 2.0.0, not a pre-release. Push the branch first and let `rust-ci.yml` run on it; merge
  only after the release checklist has passed.
- The Python app (`autopara/`, `tests/`, `installer/`, `build.cmd`, `installer.yml`) is unchanged
  and stays until the parity report is accepted. `android/` is untouched.

## Waiting on the owner

1. **Push `rust-rewrite`** so CI runs for the first time on all three runners (plan 1.4, 6.5, 9.1).
   The macOS code (ScreenCaptureKit capture, the `.app`/`.dmg`) has never been compiled.
2. **Repository secrets** `TAURI_SIGNING_PRIVATE_KEY` (contents of
   `%USERPROFILE%\.tauri\autopara-updater.key`) and, if set, its password; back the key up
   (docs/RELEASE_CHECKLIST.md).
3. **Tag `python-final`** on the last Python commit (`8e73aff`, 1.10.0) before the merge. Not done
   from here: a tag is meant to be pushed.
4. **The Python tree:** stays where it is for now (`rust/DECISIONS.md`, "The Python tree stays in
   place"). Moving it to `legacy/python/` is a separate change once the parity report is accepted.
5. **`installer.yml` and Android.** That workflow still builds the Python app on every push to
   `main` and attaches the APK only to the Python version's release, so 2.x releases have no APK.
   Retire its Windows and Mac jobs, and move the Android build under the workspace version, in one
   change at the cutover.
6. **A 1.10.1 bridge release (optional).** 1.x cannot download 2.0 because its updater asks the old
   repository name (docs/UPGRADING.md). Fixing `GITHUB_REPO` in a last Python release would let 1.x
   users get 2.0 in one click; without it they download it by hand, guided by the release notes'
   first line.
7. **Hardware:** a clean Windows VM for the release checklist (install, autostart, upgrade over
   1.10.0, uninstall); a Mac of each kind; a real lecture recording for Lecture Assist accuracy.

## Run and verify

```bash
cd rust
cargo xtask check                          # the whole gate
cd crates/autopara-app
AUTOPARA_DATA_DIR=/tmp/ap cargo tauri dev  # PowerShell: $env:AUTOPARA_DATA_DIR = "..."
```

Windows needs `LIBCLANG_PATH` and CMake on `PATH` by its 8.3 short path (CONTRIBUTING.md). The
release build is `cargo tauri build` with `TAURI_SIGNING_PRIVATE_KEY` set to the key file; the
2.0.0 dry run produced a signed 8.5 MB `AutoPara_2.0.0_x64-setup.exe`.

## Things that cost time — do not rediscover

- **Never run a checkout without `AUTOPARA_DATA_DIR`.** The default data folder is the real one,
  shared with an installed 1.x or 2.x. A debug build already refuses to write the autostart entry.
- **Never start a release build (`target\release\AutoPara.exe`) on a PC with AutoPara installed.**
  It syncs the autostart entry on start and would point the real `Run` value at `target\`.
  Smoke-test release builds on a VM; on the development PC use a debug build.
- **Build whisper.cpp outside `%TEMP%` and away from very long paths.** With the target folder deep
  under `%TEMP%`, CMake's compiler check fails ("No CMAKE_C_COMPILER could be found", MSB8029).
- **whisper.cpp built unoptimised** under CMake's Visual Studio generator until
  `rust/.cargo/config.toml` forced `-O2`; speech recognition was several times slower.
- **CMake's long path with spaces breaks `npm.cmd`** inside `cargo xtask check`; put it on `PATH`
  by its 8.3 short name.
- **The npm Tauri CLI cannot find the Rust toolchain** on the development machine; use
  `cargo tauri` (`tauri-cli` from crates.io).
- **HTML drag and drop does not work in the Windows webview** once Tauri's file drop is on (which
  is how a `.docx` arrives with its real path); classes are dragged with pointer events.
- **MPL-2.0 crates under Tauri** (`cssparser`, `selectors`, …) are allowed one by one in
  `deny.toml`; any new MPL crate still fails `cargo deny`.
- **The 1.x updater can see but not fetch 2.0** (PORT_SPEC §I.7). Nothing on the 2.x side can fix
  that; see "Waiting on the owner".
- **`latest.json` is written by CI** (`jq` on Ubuntu); a local build produces the installer and its
  `.sig` but no `latest.json`.

## Known gaps

From `rust/PROGRESS.md` and `rust/PARITY_REPORT.md`:

- The updater has never run end to end (needs two published releases); the macOS self-update has
  never run at all.
- The installer has not been run on a clean VM, nor over a real 1.x install (the data import has
  been checked on a copy of real data: docs/UPGRADING.md).
- Lecture Assist: Windows loopback capture works on synthetic audio; not yet on a real meeting.
  macOS capture is written but uncompiled. GPU (CUDA) builds are not in CI.
- macOS images are unsigned and not notarised.

## Security pass (plan 9.3, 2026-11-02)

- `cargo deny check`: advisories, bans, licences, sources ok. `cargo audit`: no vulnerabilities;
  five warnings (`glib` unsound, `zbus_names`, `zvariant`, `zvariant_derive`, `proc-macro-error`
  unmaintained), all Linux-only Tauri dependencies that are not built for Windows or macOS.
- Secrets: gitleaks is not installed here; instead the whole history (and the text inside every
  committed `.docx`) was searched for key patterns and for the 27 meeting ids of the real
  timetable. No hits; every committed link is an invented one.
- CSP: `default-src 'self'`, no inline scripts, `object-src 'none'`, IPC only. Capabilities: the
  main window has `core:default` only; the overlay adds moving, hiding, resizing and click-through
  of itself. No plugin (opener, dialog, notification, updater) is reachable from the page; Rust
  calls them. App commands are callable from both windows (Tauri's default without an app
  manifest); both load only the bundled pages.
- URLs: the scheduler and the editor accept `https://` only; `open_release_page` only this
  repository's release pages; the updater one fixed HTTPS endpoint; a custom chat server must be
  `https://`, or plain `http://` only to 127.0.0.1 / localhost (fixed in this pass).
- No audio is written: the only `WavWriter` is in a test; the only file Lecture Assist creates is a
  downloaded model. The API key lives in the OS keychain, is sent only as a bearer header, and the
  interface only ever sees `has_key`.
- Logs: links are logged by host only and chat provider error bodies not at all (both fixed in
  this pass): the diagnostics button copies the log's tail, and meeting links carry passcodes.

## Map

| Where | What |
|---|---|
| `rust/crates/autopara-core` | Model, `.docx` reader and parser, SQLite, 1.x import, scheduler and engine, the string table. |
| `rust/crates/lecture-assist` | Audio capture, VAD, whisper, the LLM client, notes. |
| `rust/crates/autopara-app/src-tauri` | Commands, tray, autostart, updates, the assistant's wiring, the NSIS template. |
| `rust/crates/autopara-app/ui/src` | The interface and the overlay. |
| `rust/xtask` | The developer commands. |
| `rust/PORT_SPEC.md`, `DECISIONS.md`, `PARITY_UI.md`, `PARITY_REPORT.md`, `PROGRESS.md` | The 1.x behaviour, the choices, the comparison, the plan's checklist. |
| `.github/workflows/rust-ci.yml`, `rust-release.yml` | The gate on three runners; the release. |

## Conventions worth keeping

- One string table (`uk.json`); no literal user-visible text in code.
- `autopara-core` never reads the clock.
- Every difference from 1.x is a `DECISIONS.md` entry with a reason.
- Commits are small, one change each, written plainly.
