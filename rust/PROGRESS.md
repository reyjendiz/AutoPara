# Progress

Ticked only when the step's quality gate is green.

## Phase 0 — Recon and specification
- [x] 0.1 Environment check
- [x] 0.2 Read everything
- [x] 0.3 `PORT_SPEC.md`
- [x] 0.4 Python baseline
- [x] 0.5 Golden fixtures
- [x] 0.6 Visual baseline

## Phase 1 — Workspace scaffold and CI
- [x] 1.1 Cargo workspace
- [x] 1.2 Tauri 2 skeleton
- [x] 1.3 Typed IPC
- [ ] 1.4 CI (workflow written; not run yet: the branch has not been pushed)
- [x] 1.5 `xtask` commands

## Phase 2 — Core: parsing and storage
- [x] 2.1 Domain types
- [x] 2.2 Day-name recogniser
- [x] 2.3 `.docx` reading
- [x] 2.4 Timetable extraction
- [x] 2.5 SQLite layer
- [x] 2.6 Import service
- [x] 2.7 Legacy migration
- [x] 2.8 Parity tests

## Phase 3 — Scheduler and joining
- [x] 3.1 `Clock` trait
- [x] 3.2 Occurrence generation
- [x] 3.3 Open-once engine
- [x] 3.4 Missed-class handling
- [x] 3.5 Link opening
- [x] 3.6 Reminders
- [x] 3.7 Fake-clock tests

## Phase 4 — Application shell
- [x] 4.1 Command and event API
- [x] 4.2 Single instance, tray
- [x] 4.3 Autostart, settings, theme, window state
- [x] 4.4 Logging and diagnostics
- [x] 4.5 Security baseline

## Phase 5 — UI parity
- [x] 5.1 Design tokens
- [x] 5.2 Layout
- [x] 5.3 Week grid
- [x] 5.4 Day and month views
- [x] 5.5 Interactions
- [x] 5.6 Search and filters
- [x] 5.7 Import screen
- [x] 5.8 Missed-class banner
- [x] 5.9 Settings screen
- [x] 5.10 Accessibility and keyboard
- [x] 5.11 Visual parity check
- [x] 5.12 Frontend tests

## Phase 6 — Updates and installers
- [x] 6.1 Re-read release docs
- [x] 6.2 Updater (untested end to end: needs two published releases)
- [x] 6.3 Windows installer (built locally; install/uninstall untested on a clean VM, see docs/RELEASE_CHECKLIST.md)
- [ ] 6.4 macOS images (configured in rust-release.yml; not built yet: needs a Mac runner, untested on hardware)
- [ ] 6.5 Release workflow (written; not run yet: the branch has not been pushed)
- [x] 6.6 Updater signing
- [x] 6.7 Release checklist

## Phase 7 — Lecture Assist
- [x] 7.1 Engine API
- [ ] 7.2 Audio capture — Windows (loopback, device list, reconnect, resampling, device picker done; a run on real meeting audio pending)
- [ ] 7.3 Audio capture — macOS (ScreenCaptureKit source written and wired, BlackHole fallback; untested on hardware and not yet compiled: only the macOS CI job builds it, and the branch has not been pushed)
- [x] 7.4 VAD and segmentation (Silero through whisper.cpp, loudness fallback; see DECISIONS.md)
- [ ] 7.5 Speech recognition (runs on `small`, but far slower than real time on CPU; accuracy not yet checked on a real lecture)
- [x] 7.6 LLM provider layer (client, SSE, fallback chain, presets, rate guard, keychain, settings and connection test)
- [x] 7.7 Assist features (engine and overlay)
- [x] 7.8 Overlay window (checked in a browser with a stubbed Tauri bridge; not yet seen in the running app)
- [x] 7.9 Integration (commands, consent, tray, recording icon, saved text, end-of-class stop, `Ctrl+Alt+L`, listen button and class menu; not run in the real app with a model yet)
- [x] 7.10 Storage (tables, retention, wipe button)
- [x] 7.11 Settings UI (consent, devices, models, language, chat model, key, fallback, shortcut, retention, wipe, revoke)
- [ ] 7.12 Performance budgets (first CPU run misses badly: RTF 4.2 for finals alone, ~16 s per call with `small`; cause unknown, see docs/ASSIST_BENCH.md)
- [ ] 7.13 Tests (unit and synthetic end-to-end done; real speech fixtures pending)
- [x] 7.14 Docs (docs/ASSIST.md, docs/ASSIST_BENCH.md)

## Phase 8 — Cutover and documentation
- [ ] 8.1 Docs for the new stack
- [ ] 8.2 Upgrade path
- [ ] 8.3 `python-final` tag
- [ ] 8.4 Version 2.0.0 dry run
- [ ] 8.5 Parity report

## Phase 9 — Final QA
- [ ] 9.1 Clean checkout gate
- [ ] 9.2 Manual smoke
- [ ] 9.3 Security pass
- [ ] 9.4 Final summary
