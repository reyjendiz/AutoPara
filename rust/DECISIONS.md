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

## Catch-up only from continuous watching

- **Decision:** a class that is already running is opened without asking (catch-up mode "open")
  only when the scheduler was running, awake and on an unbroken clock when the class's trigger
  passed: its previous tick was before the trigger, at most 45 s earlier, and the wall clock moved
  in step with the monotonic clock (within 30 s). After an app start, a wake from sleep or a clock
  change, a running class is always offered (the missed-class banner) and never opened.
- **Python:** the first tick after start asks; the second tick, 15 s later, opens the class in
  "open" mode, and a wake from sleep opens it on the first tick (PORT_SPEC §I.12, §I.20; golden
  scenarios `late_start_open` and `sleep_across_start_open`).
- **Why:** plan step 3.4 — nothing opens until the user picks, even in "open immediately" mode,
  which applies only to a class that starts while the app is running. The golden comparison checks
  that these are the only differences: every other event of all 72 Python simulations matches.

## Only https links are opened

- **Decision:** `scheduler::is_openable` accepts `https://` only (trimmed, case-insensitive).
- **Python:** `http://` too (PORT_SPEC §C.5, §I.13).
- **Why:** plan step 3.5. A lesson with an `http://` link is treated like one with no link: never
  opened automatically, missed when its time passes.

## The engine wakes at the next due moment

- **Decision:** after each tick the engine sleeps until the earliest reminder, trigger, start or end
  still ahead today (or midnight), never longer than 15 s, and wakes early when told something
  changed.
- **Python:** a plain 15 s poll.
- **Why:** an open at exactly `start - lead` instead of up to 15 s later; the 15 s cap keeps the
  Python app's resilience to settings edits, sleep and clock changes.

## Local wall time, and daylight saving

- **Decision:** the timetable stays in naive local time, as in Python. Across a DST change:
  an hour that happens twice opens its class once (the open log is keyed by date); a class in the
  hour that does not exist is offered, not opened, because the wall clock jumps past its trigger.
- **Why:** classes are written as local wall times; the Europe/Kyiv tests in `tests/scheduler.rs`
  pin both cases.

## Autostart without a plugin

- **Decision:** `src-tauri/src/autostart.rs` writes the same entries as the Python app: the
  `AutoPara` value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` holding
  `"<exe>" --hidden`, and `~/Library/LaunchAgents/com.autopara.app.plist` on macOS. It defaults to
  on and is synced on every launch, as before. A debug build never writes either.
- **Alternatives:** `tauri-plugin-autostart`, whose value name and command format are its own.
- **Why:** the installer (phase 6) and the upgrade from 1.x must agree with the app byte for byte,
  or the app silently rewrites the installer's entry (PORT_SPEC §D). The debug-build rule keeps a
  run from a checkout from replacing the user's real entry with a path into `target/`.

## Data folder override

- **Decision:** `AUTOPARA_DATA_DIR`, when set, replaces the data folder.
- **Why:** the default folder is the Python app's; development and smoke runs must not create
  files there.

## Window size and position

- **Decision:** `tauri-plugin-window-state`, restoring size, position and maximised state but
  never visibility (a `--hidden` start stays in the tray). It keeps its file in Tauri's config
  folder.
- **Why:** it handles monitors that went away; a hand-written version would have to re-learn that.

## Tray glyph variants

- **Decision:** the tray picks `tray-light.png` or `tray-dark.png` from the system theme and follows
  theme changes. Both are the app mark today.
- **Why:** the mark is a full-colour tile that reads on both backgrounds, as in the Python app. The
  mechanism is there for a monochrome glyph, and for the "recording" glyph Lecture Assist needs.

## Command errors are codes

- **Decision:** a command that rejects its input answers with a short code (`subject_required`,
  `invalid_url`, `invalid_time`, `invalid_date`, `scope_required`, `not_found`, `no_group`,
  `out_of_range`); the interface owns the Ukrainian sentence for each.
- **Why:** every user-visible word stays in the one string table (R2), and the Rust side stays
  testable without it.

## The editor accepts https links only

- **Decision:** `save_lesson` rejects a link that is not `https://`.
- **Python:** accepted `http://` too.
- **Why:** the scheduler would never open it (see "Only https links are opened"); saying so when
  the link is typed is better than a class that silently never opens.

## Interface: morphs, dragging and native fields

- **Morphs** use the View Transitions API: a class keeps a `view-transition-name` (its place within
  one view, its date across views), so it grows from its old box to the new one; everything else
  cross-fades. `prefers-reduced-motion` turns all motion off. The Python overlay's 14 px drift of
  the empty calendar is not reproduced.
- **Dragging** a class uses pointer events, not HTML drag and drop: the webview's own file-drop
  handling, which delivers a dropped `.docx` with its real path, disables HTML drag and drop on
  Windows.
- **Date and time fields** in the editor are the webview's own (`<input type="date|time">`), with
  its calendar and clock pickers, in place of Qt's spin boxes.
- **Text size on macOS** is the same CSS points as on Windows; the Python app's 1.15 Mac scale is
  not carried over.
- **Demo backend** (`?demo`): the interface can run in an ordinary browser on the sample timetable
  with a pinned clock; `cargo xtask demo-data` regenerates it from the core, and `cargo xtask
  screenshots` photographs it for `PARITY_UI.md`.

## Interface: morphs, dragging and native fields

- **Morphs** use the View Transitions API: a class keeps a `view-transition-name` (its place within
  one view, its date across views), so it grows from its old box to the new one; everything else
  cross-fades. `prefers-reduced-motion` turns all motion off. The Python overlay's 14 px drift of
  the empty calendar is not reproduced.
- **Dragging** a class uses pointer events, not HTML drag and drop: the webview's own file-drop
  handling, which delivers a dropped `.docx` with its real path, disables HTML drag and drop on
  Windows.
- **Date and time fields** in the editor are the webview's own (`<input type="date|time">`), with
  its calendar and clock pickers, in place of Qt's spin boxes.
- **Text size on macOS** is the same CSS points as on Windows; the Python app's 1.15 Mac scale is
  not carried over.
- **Demo backend** (`?demo`): the interface can run in an ordinary browser on the sample timetable
  with a pinned clock; `cargo xtask demo-data` regenerates it from the core, and `cargo xtask
  screenshots` photographs it for `PARITY_UI.md`.

## Updates

- **Decision:** `tauri-plugin-updater` reads `latest.json` from the newest GitHub release of
  `reyjendiz/AutoPara`; every file is checked against the minisign public key in `tauri.conf.json`.
  Windows runs the downloaded `Setup.exe` silently (`/S /UPDATE /R`) and the installer restarts the
  app. **macOS replaces `AutoPara.app` through the updater** (a signed `.app.tar.gz`) and restarts,
  instead of opening the `.dmg` as the Python app did. The plugin is built without its `zip`
  feature: Tauri 2's own update archives are `.tar.gz` on macOS and the bare installer on Windows.
- **Python:** asked the GitHub API for the latest release and picked the asset by name (PORT_SPEC
  §G); on macOS it only opened the disk image.
- **Why:** signature checking and the install step come with the plugin; a `.dmg` would leave the
  user to drag the app by hand. Releases with a pre-release version are GitHub pre-releases, which
  `releases/latest` (and so the updater) skips. `latest.json` carries no notes yet, so the update
  strip shows no detail line.

## Windows installer

- **Decision:** the Tauri bundler's NSIS target with a **custom template**,
  `src-tauri/installer/installer.nsi`: Tauri 2.10's own template changed to do what
  `installer/AutoPara.nsi` does. Per-user into `%LOCALAPPDATA%\Programs\AutoPara` with no UAC
  prompt; a components page with the app (read-only), `Ярлик на робочому столі` and `Запускати
  разом із Windows` (both on, so `/S` installs both); the HKCU `Run` value `AutoPara` = `"<exe>"
  --hidden`, which a test compares with `autostart::startup_command`; Start Menu folder `AutoPara`
  with the app and `Видалити AutoPara`; Add/Remove entry `AutoPara — автозапуск пар` under HKCU;
  Ukrainian only. Uninstall removes the files, shortcuts, `Run` value and registry keys, then asks
  whether to delete `%APPDATA%\AutoPara`; No, `/S` and `/P` keep it.
- **Alternatives:** installer hooks on Tauri's default template. Hooks run inside the install
  section, so they can neither change the default folder (`%LOCALAPPDATA%\AutoPara`) shown on the
  directory page nor add a components page; the default template also offers to delete
  `%APPDATA%\com.autopara.app`, not the folder AutoPara's data is in.
- **Strings:** `build.rs` writes `gen/installer-strings.nsh` (`!define STR_*`) from the `installer_*`
  keys and `app_title` of `i18n/uk.json`; Tauri includes it as the "installer hooks" file, the only
  file path the bundler passes into a template. The template's other messages (the reinstall page,
  WebView2, "the app is running") are Tauri's own Ukrainian language file.
- **Differences from `AutoPara.nsi`:** a running copy is found by its executable through the
  Restart Manager and closed after an OK (Tauri's `CheckIfAppIsRunning`) instead of by the window
  title; a previous install is offered for removal on Tauri's reinstall page; an update (`/UPDATE`)
  leaves the shortcuts and the autostart choice alone; Yes on uninstall also deletes the webview's
  and window state's folders (`%APPDATA%\com.autopara.app`, `%LOCALAPPDATA%\com.autopara.app`); the
  install folder is removed only when empty. Installing over the Python app removes its
  `_internal` folder, finds its folder through `Software\AutoPara\InstallDir`, and replaces its
  shortcuts and Add/Remove entry (same paths and key).
- **WebView2:** the Evergreen bootstrapper is embedded and runs silently when WebView2 is missing.
  It still downloads the runtime, so a PC without WebView2 needs the network once; Windows 11 and an
  updated Windows 10 already have it. The fixed runtime would add about 180 MB to every download.
- **No MSI:** bundle targets are `nsis`, `app` and `dmg`; WiX would need an admin install and does
  not accept a pre-release version.

## Release workflow

- **Decision:** `.github/workflows/rust-release.yml` builds Windows, Apple Silicon and Intel in
  parallel and creates the release only after all three succeed, with `latest.json`; the version is
  read with `cargo metadata` from the workspace.
- **Python:** `installer.yml` created the release from the Windows job and the Mac jobs attached
  their images afterwards.
- **Why:** `latest.json` names every platform's file, so it can only be written once all exist;
  a release missing a file would point the updater at nothing. The Android APK is still built and
  attached by `installer.yml` under the Python app's version; it moves over in phase 8.

## Lecture Assist: voice activity detection

- **Decision:** Silero VAD runs through whisper.cpp's own implementation (`whisper_rs::WhisperVadContext`,
  the ggml conversion of Silero v5, MIT), with a loudness detector against an adapting noise floor
  as the fallback.
- **Plan:** Silero through ONNX Runtime (`ort`), `webrtc-vad` as fallback.
- **Why:** whisper.cpp is already shipped for recognition; ONNX Runtime would add a second
  inference runtime and a native library to every installer for a model of 2 MB. The loudness
  fallback needs no native code and is enough for meeting audio, where the far side is near silent
  between phrases.

## Lecture Assist: building whisper.cpp

- **Decision:** `whisper-rs-sys` generates its bindings at build time, which needs libclang and
  cmake. CI runners have both. On a Windows machine without LLVM, set `LIBCLANG_PATH` to any
  libclang 18 (the `libclang` wheel from PyPI works) and put Visual Studio's bundled cmake on
  `PATH`.
- **Alternative:** the bindings bundled with the crate (`WHISPER_DONT_GENERATE_BINDINGS`).
- **Why:** they are generated on Linux, where `long` is 8 bytes, and fail the layout checks on
  Windows.

## Lecture Assist: engine timing

- **Decision:** the notes timer and the partial-text interval count audio time (samples heard),
  not wall time; partials re-read the open utterance every `partial_every_ms` (1 s by default,
  0 turns them off).
- **Why:** the pipeline is then deterministic under test, and a slow recogniser delays notes
  instead of piling up requests.

## Lecture Assist: language detection

- **Decision:** with "auto", the language is detected once, on the first utterance of at least
  three seconds, choosing only among Ukrainian, Russian and English, and kept for the session.
- **Alternative:** whisper's own detection on every window, among all languages.
- **Why:** a class keeps its language; per-window detection doubles the encoder work and flips
  between close languages on short phrases.

## Lecture Assist: GPU builds on Windows (open)

- **Status:** the `cuda` feature builds whisper.cpp against the CUDA toolkit for local use and
  `bench-stt`. Shipping it would mean redistributing `cudart`/`cublas`, which are under NVIDIA's
  licence, not a permissive one (R11). Until the owner decides, release builds use the CPU
  backend on Windows and Metal on macOS. A Vulkan build is the permissive alternative to test.

## Lecture Assist: the shortcut

- **Decision:** `Ctrl+Alt+L` through `tauri-plugin-global-shortcut`, registered from Rust at
  start; it toggles the same start/stop as the tray item. A new combination is saved only if it
  registers, otherwise the old one is put back and the settings say it is taken.
- **Why:** a combination another program holds fails silently on some systems; checking at
  registration is the only reliable conflict test.

## Lecture Assist: overlay window

- **Decision:** a second window `overlay`, declared in `tauri.conf.json`, created hidden at
  start and shown when listening starts. Transparent and frameless, always on top, not in the
  taskbar; size and position come from the window-state plugin, like the main window. Its
  look (opacity, font size, collapsed) is per-viewer and kept in the webview's `localStorage`.
  macOS transparency needs `macOSPrivateApi`.
- **Click-through:** the window ignores the mouse entirely, so it cannot turn itself back;
  opening AutoPara from the tray (or a second launch) turns click-through off.
- **Why:** created at start, the overlay sees every event from the first `starting`, and needs
  no state replay command.

## Lecture Assist: macOS audio

- **Decision:** ScreenCaptureKit audio only (16 kHz mono straight from the system, picture
  2×2 at 1 fps because a stream must have one) through the `screencapturekit` crate, used when
  no device is chosen. Without the Screen Recording permission, or when the user picks a
  device (BlackHole), the `cpal` input path is used.
- **Status:** untested on hardware; compiled only by the macOS CI jobs.
- **Why:** it needs no extra driver, and the system's own recording indicator stays on.

## Lecture Assist: benchmark

- **Decision:** `bench-stt` replays a wav through the energy detector, the engine's segmenter
  and whisper as fast as possible (partials every second of audio, a final per utterance) and
  reports timings only. It is the `bench_stt` example of `lecture-assist`, which xtask runs:
  linking whisper into xtask made `cargo run` add its native search paths to PATH, and
  `npm.cmd` in `cargo xtask check` then failed to find `tsc` on Windows.
- **Status:** results are in `docs/ASSIST_BENCH.md`.


## Upgrade from 1.x

- **Decision:** 2.0 installs over 1.x in place and keeps its identity: the same install folder
  (`%LOCALAPPDATA%\Programs\AutoPara`, found through 1.x's `Software\AutoPara\InstallDir`), the
  same `AutoPara.exe`, `Run` value, shortcuts and Add/Remove key; on macOS the same `AutoPara.app`,
  bundle identifier `com.autopara.app` and LaunchAgent. The old program is replaced, so there is no
  second copy to offer for removal. 1.x's data files (`autopara.db`, `schedules/`) are imported once
  and kept, not offered for deletion. Details: `docs/UPGRADING.md`.
- **Alternatives:** a new identity (`com.autopara.app` folder, a second Start Menu entry) with an
  uninstall of 1.x; deleting 1.x's data after the import.
- **Why:** one identity means one autostart entry and one app in Add/Remove, which also stops both
  apps opening the same class (PORT_SPEC §I.21). The old files are small and are the way back to
  1.10.0; the uninstaller's "delete my data" removes them with everything else.
- **1.x updater:** cannot download 2.0 (its repository name is the old one, PORT_SPEC §I.7). The
  release notes' first line, which 1.x shows in its update strip, says where to download it. A
  1.10.1 bridge release would need a change to the Python app and is left to the owner.

## The Python tree stays in place

- **Decision (plan 8.3):** `autopara/`, `tests/`, `installer/` and the build scripts stay where they
  are; nothing moves to `legacy/python/` yet. The last Python state is tagged `python-final`
  (`8e73aff`, 1.10.0). The Android app and its build stay exactly as they are, at the owner's
  request.
- **Alternatives:** move the tree to `legacy/python/` now.
- **Why:** the plan keeps the Python tests until the parity report is accepted, and
  `installer.yml`, the Android build (which reads `autopara/__init__.py`) and the docs' references
  all point at the current paths; moving them is one change for the cutover, together with retiring
  `installer.yml`.

## Xcode 26 on the Mac runners

- **Decision:** the macOS jobs of `rust-ci.yml` and `rust-release.yml` select the newest installed
  Xcode 26 before building, and the release's Apple Silicon job runs on `macos-15` instead of
  `macos-14`.
- **Why:** `screencapturekit` 11 requires `apple-metal` 0.10, whose Swift bridge names Metal tensor
  types that exist only in the macOS 26 SDK; with the runners' default Xcode 16 the build script
  fails ("cannot find 'MTLTensorDataType' in scope"). Every 0.10.x release has the same code. The
  types sit behind `@available(macOS 26, *)`, so the app still runs on older macOS; only the SDK
  used to build must be new. `macos-14` has no Xcode 26.

## Classes open in AutoPara's own tab

- **Decision:** at the owner's request, Google Meet links and Zoom `/j/<id>` links open in a
  "Відкрита пара" tab inside the main window instead of the system browser (1.x and 2.0 until now
  opened the browser). A Zoom link becomes Zoom's web client (`/wc/join/<id>`, same query), so no
  Zoom app is involved. Any other link still goes to the browser, and "Відкрити в браузері" moves a
  class there. Windows also loses its system title bar: the tab row carries the window's own
  minimise, maximise and close buttons and drags the window.
- **How:** the class page is a second webview added to the main window (Tauri's `unstable`
  feature, `Window::add_child`), placed over the area the interface reports and hidden while the
  schedule tab or a dialog is showing. One class at a time; opening another replaces it. A webview
  is always a rectangle (and `unsafe` window regions are not allowed), so an initialization script
  adds one style rule to the page: a click-through `html::after` fixed to the viewport whose
  box-shadow paints the tab's border and background colours over the four corners. Rounding the
  site's `body` instead failed on pages that scroll the whole document (Zoom). The interface sends
  its colours with every placement, so a theme change recolours the corners.
- **Joining by itself (opt-in, owner's request):** with "Входити в пару одразу" on, `join.js` runs
  after each class page loads. It turns Meet's or Zoom's camera and microphone off, types the
  saved guest name if the page asks for one, and presses "Join"/"Ask to join". It presses Join only
  once both devices are confirmed off or unavailable, clicks a toggle at most twice and gives up
  after 60 s, leaving the page to the user. Its hooks (Meet's `jsname`/`data-is-muted`, Zoom's
  `#preview-*` ids and icon class names) were read from the live join screens in November 2026
  and break silently when the sites change them. The name reaches the script as a JSON string.
  Lecture Assist's rule (never act in the meeting) is unchanged; this is the opener's job.
- **Security:** the page is a remote origin with no capability. The `default` capability names the
  interface's webview (`webviews: ["main"]`), not the window, and Tauri refuses every command from
  a remote origin that no capability names, so the page cannot reach the app. Logs name only the
  site, never the link (it carries the passcode).
- **Sign-in:** "Увійти в Google" and "Увійти в Zoom" open the services' sign-in pages in the tab;
  cookies stay in AutoPara's WebView2 profile. Google says it blocks sign-in from embedded
  webviews, so a Google account may be refused there (checked only up to the sign-in page loading:
  no credentials were entered during development). Meet's guest join and Zoom's web client work
  without an account; camera and microphone go through WebView2's own permission prompt.
- **Alternatives:** keep the browser (the owner asked for tabs); a separate window per class (not
  tabs); spoofing a browser user agent to get past Google's check (rejected: it defeats a security
  control).

## Lecture Assist may answer the teacher's questions (opt-in)

- **Decision (owner, 2026-11-02):** a setting "Підказувати відповіді на запитання викладача", off
  by default. With it on, each segment the engine flags as a question (`FlagKind::Question`) gets
  a short answer from the chosen language model (`AskKind::Answer`), shown in its own block at
  the top of the overlay. The system prompt no longer forbids answering for the student.
- **Why:** the owner attends only to keep a study deferment and asked for this directly, overriding
  the plan's earlier rule (plan 7.7: explain, never answer). Tasks and homework are still only
  listed.
- **Unchanged (R13):** text on the user's own screen only; nothing speaks, types into the meeting
  or hides the overlay from screen sharing.
- **Limits:** answers share the rolling notes' rate limit (6 requests a minute); a question that
  finds no request left, or no provider, is listed without an answer and shows no error.

## Lecture Assist: the built-in language model

- **Decision (owner, 2026-11-02: "everything must be in one app"):** the default chat provider is
  "Вбудована модель": llama.cpp's own `llama-server` release (b12052, Windows Vulkan build, 38 MB)
  plus a GGUF model, both downloaded from the settings and checked against a pinned SHA-256, never
  bundled. The app starts the server on 127.0.0.1 at a free port with a random `--api-key` on the
  first request, talks to it through the existing OpenAI-compatible client, and kills it on quit.
  `--sleep-idle-seconds 600` unloads the model when the app is quiet. An unset provider now means
  the built-in model; "Без нотаток" is stored as `none`.
- **Models:** Gemma 4 12B (Google's QAT q4_0, 7.0 GB, Apache-2.0, the default), MamayLM 12B v2.0
  (INSAIT, Ukrainian-tuned Gemma 3, 7.3 GB, Gemma licence) and Gemma 4 E4B (Google's QAT q4_0,
  5.2 GB, Apache-2.0, for weak PCs and the CPU).
- **Alternatives:** the `llama-cpp-2` crate in-process (both it and `whisper-rs-sys` link their own
  static ggml, so their symbols clash, and every build would compile llama.cpp with a GPU SDK);
  bundling the server in the installer (8.5 MB would become ~45 MB for users who never use it);
  the CUDA build (≈165 MB plus NVIDIA's runtime under a non-permissive licence, see "GPU builds on
  Windows"); Gemma 4 26B-A4B (fills a 16 GB card and got a history fact wrong where 12B did not).
- **Measured** (RTX 4080 SUPER, Vulkan, Gemma 4 12B): load about 5 s, the first answer about 1.5 s
  to its first token, then about 0.1 s, 66–71 tokens/s; a two-sentence Ukrainian answer in 0.7 s.
- **Fallback:** with fallback on, any other provider hands over to the built-in model when it is
  downloaded; the built-in model hands over to Ollama. Nothing downloaded means no chat at all, as
  before, rather than requests to a server that is not there.
- **Not yet:** macOS (the owner will do it later; `RUNTIME` is `None` there). A quit during a
  model load leaves the server running until it idles out.

## Interface: morphs everywhere

- **Decision (owner, 2026-11-02):** every screen change made by the user is a View Transitions
  morph through one helper (`dom.ts` `morph`): dialogs grow out of the control that opened them and
  shrink back into it, plus the top tabs, the theme, the banners, the assistant's model lists and
  the assistant panel folding. Reduced motion, a still demo picture or a webview without the API skip it.
- **Not morphed:** live text (transcript lines, notes) — it changes every second and would never
  sit still; the class page in its tab, which is a native webview the morph cannot capture.

## Lecture Assist: a panel in the main window, not an overlay

- **Decision (owner, 2026-11-02):** the assistant's notes are a panel on the right of the
  "Відкрита пара" tab, beside the class page, instead of the separate always-on-top `overlay`
  window ("Lecture Assist: overlay window" no longer applies). The panel exists from start,
  hidden, and shows itself (bringing the window and the tab forward) on `starting`. It folds to
  its header and closes only once listening has stopped. Opacity and click-through went with the
  window; font size and folding stay, in `localStorage`.
- **Why:** the class itself now opens inside AutoPara, so the notes belong next to it; one window
  is simpler (no overlay capability, no transparency, no click-through recovery).
- **Unchanged (R13):** text on the user's screen only; the panel shows the pulsing recording dot
  and nothing hides it.
