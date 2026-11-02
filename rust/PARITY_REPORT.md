# Parity report: 1.10.0 (Python) vs 2.0.0 (Rust)

Plan step 8.5. One row per item of `PORT_SPEC.md`. ✅ the Rust app does what 1.x does (or what 1.x
was meant to do, where PORT_SPEC §I names a 1.x bug); ⚠️ a difference recorded in `DECISIONS.md`
and waiting for the owner's approval, or behaviour that could not be checked here; ❌ missing.

"Golden" means the item is held to the 1.10.0 app's own output by `cargo xtask golden`
(`rust/fixtures/golden/`: 3 documents for the parser, 72 simulations for the scheduler). As of
2026-10-28 there are no ❌ rows.

## A. `.docx` parsing

| Item | Status | Note |
|---|---|---|
| A.1 Reading the package | ✅ | `docx.rs`. Golden: merges, `gridSpan`, `vMerge`, nested tables, hyperlinks, `w:tab`/`w:cr`. |
| A.2 Text normalisation | ✅ | `text.rs`: NFC, apostrophe folding, whitespace, case folding. Golden (`edge_cases`: NFD text). |
| A.3 Day-name recognition | ✅ | `days.rs`, the same alias table and longest-prefix rule. Golden (`multilingual`). |
| A.4 Times and pairs | ⚠️ | Same rules; digits are ASCII only (DECISIONS "ASCII digits in time and pair cells"). No document seen uses others. |
| A.5 Table → course | ✅ | `parser.rs::parse_table`, column ranges per group. Golden. |
| A.6 Cell content | ✅ | Subject, teacher, links from relationships and text. Golden. |
| A.7 Shared classes and duplicates | ✅ | `gridSpan` first, duplicated text as fallback. Golden. |
| A.8 Consecutive slots | ✅ | `merge_consecutive_slots`, adjacency kept. Golden. |
| A.9 Courses | ✅ | Ordinal order, `course_label`. Golden (empty course included). |
| A.10 Identity and repeat rules | ✅ | `source_key`, re-import keeps manual lessons. `db.rs` tests. |

## B. Data model and storage

| Item | Status | Note |
|---|---|---|
| B.1 Schema | ✅ | The 1.10.0 schema, plus Lecture Assist tables; versioned migrations instead of `_migrate`. |
| B.2 Operations | ✅ | `db.rs`: import, add/update/move/delete, claim, mark, clear. Re-import still never deletes vanished courses (§I.11, kept). |
| B.3 Data locations | ⚠️ | Same folder; new file names `autopara2.db`, `timetables/`, `logs/` (DECISIONS "Data folder"). The 1.x files are imported once and only read; checked on a copy of real data (docs/UPGRADING.md). |

## C. Scheduler

| Item | Status | Note |
|---|---|---|
| C.1 Loop | ⚠️ | Wakes at the next due moment, at most 15 s (DECISIONS "The engine wakes at the next due moment"). Golden: every event of the 72 simulations matches except the two catch-up cases below. |
| C.2 Invariants | ✅ | Claim-before-open on `UNIQUE(lesson_id, occur_date)`; `tests/scheduler.rs` (restart, clock jumps, shared class, two racing instances). |
| C.3 Manual paths | ✅ | `open_now`, `mark`, `clear_mark`; a future date is opened without claiming. |
| C.4 Sleep, restart, clock changes | ⚠️ | A running class is offered, never opened, after start, wake or a clock jump, also in "open" mode (DECISIONS "Catch-up only from continuous watching"; fixes §I.12, §I.20). DST pinned in Europe/Kyiv. |
| C.5 Link opening | ⚠️ | `https://` only (DECISIONS "Only https links are opened"; §I.13). System browser through the opener plugin. |
| C.6 Notifications | ✅ | Reminder, open, catch-up, update found, update ready, with the same texts from `uk.json`; gated on `notifications_enabled` as in 1.x. Not yet seen on a clean VM. |
| C.7 "Now / next" pill | ✅ | `lib/lessons.ts`, vitest. |
| C.8 Single instance | ✅ | `tauri-plugin-single-instance`; a second launch surfaces the window. 1.x and 2.x do not see each other (§I.21, below). |

## D. Settings and defaults

| Item | Status | Note |
|---|---|---|
| D Settings and defaults | ✅ | Same keys and defaults (`db.rs` `DEFAULTS`); unknown `catchup_mode`, `theme`, `view_mode`, `update_mode` read as their defaults. New keys: `legacy_imported_at`, `assist_*`. `class_duration_minutes` now applies to imports (§I.8). |

## E. Interface

Compared side by side with the 1.x pictures (`rust/PARITY_UI.md`); no pixel diff.

| Item | Status | Note |
|---|---|---|
| E.1 Main window | ✅ | Close to 1.x. Window size and position are now remembered (§I.19). |
| E.2 Navigation bar | ✅ | Period, pill, segmented control, arrows, "Сьогодні". |
| E.3 Filter bar | ✅ | Search, subject and group filters, count, Enter / Shift+Enter jumps. |
| E.4 Week and day grid | ✅ | Minutes-to-pixels placement, now line, empty-slot menu. Small text offsets visible side by side (PARITY_UI). |
| E.5 Class card | ✅ | Chips, whole-line clipping, states, left click joins, right click menu, drag. |
| E.6 Month view | ✅ | Six weeks, three chips and "ще N", drag to another day. |
| E.7 Catch-up banner | ✅ | Queue, `ще N у черзі`, nothing opens until a button. |
| E.8 Update strip | ✅ | Same states and texts; installs through the updater plugin (see G). |
| E.9 Import landing | ✅ | Drop anywhere on the window, `.docx` only. |
| E.10 Import dialog | ✅ | Preview before writing; preselects the previous course and group. |
| E.11 Course and group dialog | ✅ | No re-parse. |
| E.12 Class editor | ⚠️ | Date and time fields are the webview's own controls, not Qt spin boxes (DECISIONS "Interface: morphs, dragging and native fields"); `https://` only (DECISIONS "The editor accepts https links only"). |
| E.13 Settings | ✅ | All 1.x settings, plus diagnostics and Lecture Assist. Taller than the window, so it scrolls. |
| E.14 Tray | ✅ | Same menu, plus `Слухати пару` for Lecture Assist; click shows the window. Glyph follows the system theme (both variants are the mark). |
| E.15 Keyboard and pointer | ✅ | Ctrl+F, Enter / Shift+Enter, Esc, Menu key, dialog Enter/Esc, drag (plan step 5.10). |
| E.16 Ukrainian forms | ✅ | `lib/calendar.ts` and `i18n.rs`, tested. |

## F. Design tokens

| Item | Status | Note |
|---|---|---|
| F.1 Colours | ✅ | `styles/tokens.css`, both themes; subject colours by the same MD5 pick (`lib/color.ts`). |
| F.2 Typography | ⚠️ | Same faces (Segoe UI on Windows, Inter elsewhere); the 1.15 point scale on macOS is dropped (PARITY_UI "Text on macOS"). |
| F.3 Shape and spacing | ✅ | Radii, spacing and shadows as tokens. |
| F.4 Motion | ⚠️ | View Transitions morph and cross-fade; the empty calendar's 14 px drift is not reproduced; `prefers-reduced-motion` turns motion off. |

## G. Update flow

| Item | Status | Note |
|---|---|---|
| G Update flow | ⚠️ | `tauri-plugin-updater` with `latest.json` and minisign signatures instead of the GitHub API and SHA-256; macOS replaces the app instead of opening the `.dmg`; the repository name is the current one (§I.7) (DECISIONS "Updates"). Schedule (15 s, 6 h), skip, auto-download and "installing is a click" as in 1.x. **Never run end to end**: needs two published releases. |

## H. Installer, uninstaller and release

| Item | Status | Note |
|---|---|---|
| H.1 Windows (NSIS) | ⚠️ | Custom Tauri template doing what `AutoPara.nsi` did; differences listed in DECISIONS "Windows installer". Built and signed locally for 2.0.0 (8.5 MB). Not yet run on a clean VM or over a real 1.x install. |
| H.2 macOS | ⚠️ | `.app` + `.dmg` per architecture, names as in 1.x, unsigned. Never built: needs the macOS CI runners. |
| H.3 Release workflow | ⚠️ | `rust-release.yml` publishes only after all three platforms built (DECISIONS "Release workflow"). Never run: the branch has not been pushed. The Android APK is not part of it yet. |

## I. Known gaps and 1.x bugs

| # | 1.x | 2.0 |
|---|---|---|
| 1 | Updater never exercised end to end | ⚠️ Same for 2.0 (G). |
| 2 | macOS images unsigned, update only opens the `.dmg` | ⚠️ Still unsigned; the update now replaces the app (untested). |
| 3 | Installer strings and look unseen on Windows | ⚠️ Built on Windows; the VM run is in docs/RELEASE_CHECKLIST.md. |
| 4 | Rounded popups only seen on a Mac | ✅ Popups are drawn by the page (`ui/popups.ts`), the same on every platform. |
| 5 | Qt layout traps | ✅ Not applicable; the web equivalents (card height, equal columns, whole-line clipping) are handled (E.4, E.5). |
| 6 | Publishing a release ships an update | ✅ Unchanged and documented (docs/RELEASING.md); pre-releases are skipped by the updater. |
| 7 | Updater asks the old repository name and can never download | ✅ Fixed in 2.0. 1.x copies still cannot fetch 2.0 (docs/UPGRADING.md). |
| 8 | Class length not used by import | ✅ Fixed (DECISIONS "Import uses the class-length setting"). |
| 9 | Docs disagree on uninstall | ✅ Docs now say what ships: uninstall asks, No and `/S` keep the data. |
| 10 | `open_now` leaves a `manual` row when the browser refuses | ⚠️ Kept as 1.x: the row is claimed before the open and stays `manual` if it fails. |
| 11 | Re-import never deletes vanished courses or groups | ⚠️ Kept as 1.x (BACKEND.md "Re-import behavior"). |
| 12 | Waking from sleep opens a running class in "open" mode | ✅ Fixed (C.4). |
| 13 | `http://` links accepted | ✅ `https://` only (C.5). |
| 14 | No time-zone or DST handling | ✅ Naive local time kept; DST behaviour defined and tested (DECISIONS "Local wall time, and daylight saving"). |
| 15 | Fire-once relies on one process | ✅ Single instance plus the constraint; `two_instances_racing_open_a_class_once`. |
| 16 | Test suite picks any Desktop `.docx` | ✅ Rust tests use committed invented documents only. |
| 17 | Dead code (`looks_like_schedule`, `is_course_heading`) | ✅ Not ported. |
| 18 | `theme`, `catchup_mode` not validated | ✅ Parsed; unknown values read as the defaults. |
| 19 | Window size and position not kept | ✅ `tauri-plugin-window-state`. |
| 20 | Cold-start guard lasts one tick | ✅ Fixed (C.4). |
| 21 | 1.x and 2.x would both run and both open classes | ⚠️ Solved for the installed case: 2.0 replaces 1.x's program and autostart entry (docs/UPGRADING.md). A 1.x copy run from source is not detected. Untested on a VM. |

## J–K. Baselines

| Item | Status | Note |
|---|---|---|
| J Python baseline | ✅ | 674 passed, 1 failed (§I.7), 20 skipped; the Python suite is kept until this report is accepted. |
| K Visual baseline | ✅ | The 1.x pictures in `docs/` are the reference for `rust/PARITY_UI.md`. |

## Beyond parity

Lecture Assist (plan phase 7) is new in 2.0 and has no 1.x counterpart; its status is in
`rust/PROGRESS.md` and `docs/ASSIST.md`.
