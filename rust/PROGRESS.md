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
- [ ] 2.1 Domain types
- [ ] 2.2 Day-name recogniser
- [ ] 2.3 `.docx` reading
- [ ] 2.4 Timetable extraction
- [ ] 2.5 SQLite layer
- [ ] 2.6 Import service
- [ ] 2.7 Legacy migration
- [ ] 2.8 Parity tests

## Phase 3 — Scheduler and joining
- [ ] 3.1 `Clock` trait
- [ ] 3.2 Occurrence generation
- [ ] 3.3 Open-once engine
- [ ] 3.4 Missed-class handling
- [ ] 3.5 Link opening
- [ ] 3.6 Reminders
- [ ] 3.7 Fake-clock tests

## Phase 4 — Application shell
- [ ] 4.1 Command and event API
- [ ] 4.2 Single instance, tray
- [ ] 4.3 Autostart, settings, theme, window state
- [ ] 4.4 Logging and diagnostics
- [ ] 4.5 Security baseline

## Phase 5 — UI parity
- [ ] 5.1 Design tokens
- [ ] 5.2 Layout
- [ ] 5.3 Week grid
- [ ] 5.4 Day and month views
- [ ] 5.5 Interactions
- [ ] 5.6 Search and filters
- [ ] 5.7 Import screen
- [ ] 5.8 Missed-class banner
- [ ] 5.9 Settings screen
- [ ] 5.10 Accessibility and keyboard
- [ ] 5.11 Visual parity check
- [ ] 5.12 Frontend tests

## Phase 6 — Updates and installers
- [ ] 6.1 Re-read release docs
- [ ] 6.2 Updater
- [ ] 6.3 Windows installer
- [ ] 6.4 macOS images
- [ ] 6.5 Release workflow
- [ ] 6.6 Updater signing
- [ ] 6.7 Release checklist

## Phase 7 — Lecture Assist
- [ ] 7.1 Engine API
- [ ] 7.2 Audio capture — Windows
- [ ] 7.3 Audio capture — macOS
- [ ] 7.4 VAD and segmentation
- [ ] 7.5 Speech recognition
- [ ] 7.6 LLM provider layer
- [ ] 7.7 Assist features
- [ ] 7.8 Overlay window
- [ ] 7.9 Integration
- [ ] 7.10 Storage
- [ ] 7.11 Settings UI
- [ ] 7.12 Performance budgets
- [ ] 7.13 Tests
- [ ] 7.14 Docs

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
