# HANDOFF — state of the project for whoever picks it up next

Written at the end of a long working session. It says what exists, how to run and verify it, what is
deliberately unfinished, and the traps already found. The design record proper is
`ARCHITECTURE.md`, `BACKEND.md` and `FRONTEND.md`; `CONTRIBUTING.md` lists the invariants. Read those
first — this file does not repeat them.

## Where things stand

- **Repository:** https://github.com/sevcenkoa864-oss/AutoPara (public, `main`). It was deleted and
  recreated once, so history starts at the 1.6.0 commit. No tooling-specific files are in it.
- **Latest release is v1.7.x**, published with both installers: `AutoPara-<version>-Setup.exe`
  (Windows) and `AutoPara-<version>.dmg` (macOS, Apple Silicon only). CI (`.github/workflows/installer.yml`) builds both
  on every push to `main`; a release is cut only when `APP_VERSION` names a version without a tag.
- **What 1.6.0 added** over 1.5.0: dated classes (one-off and "weekly until a date"), day / week /
  month views with navigation, the "Education Hub" redesign (Inter, black-pill accent, pastel
  subjects), course/group switcher, animations, in-app updates from GitHub, a new app logo.
- **What 1.7.x added** over 1.6.0: a system clock under the rail pill at 50 % opacity
  (`ui/clock_bar.py`); an eased hover fill on the round "+" buttons (`transitions.FadeButton`); the
  interface font is Inter bundled in `ui/fonts`, except on Windows where it is Segoe UI (Inter is its
  fallback, see `theme.font_stack`); and text is drawn unhinted (`app._smooth_text`) because Qt's
  default hinting on Windows reads as a jagged pixel font.

## Run and verify

```bash
python -m venv .venv && . .venv/bin/activate && pip install -r requirements.txt pytest
python -m pytest                       # whole suite, offscreen Qt
python -m autopara --no-rebuild        # run from source (skips the installed-copy refresh)
python tools/bump_version.py --show    # version + release state; --set X.Y.Z to bump
```

- **Test baseline.** The whole suite passes with no timetable file at all: it runs on a synthetic
  `.docx` built from code (`tests/sample_timetable.py`) that has the real one's structure -- merged
  cells, a spanning header, an empty course, links -- and none of its content. Tests that say exactly
  what the importer does pin numbers on that fixed document. The user's real timetable (not in git;
  it holds live meeting links) is optional: point `AUTOPARA_TEST_DOCX` at it and the `document_*`
  checks also run against it, asserting only what holds for any revision and reading the expected
  numbers from the file, so a new revision cannot break them. Without it those variants skip, and
  pytest's header prints which file it used or `NOT FOUND`.
- **Isolated manual run** (does not touch the real profile or the login item):
  `HOME=<tmp> APPDATA=<tmp> python -m autopara --no-rebuild`. A source run with the default `HOME`
  rewrites `~/Library/LaunchAgents/com.autopara.app.plist`.
- **Screenshots without a display:** `QT_QPA_PLATFORM=offscreen`, build a `MainWindow`, `grab()` it.
  This is how every visual change was checked.

## Things that cost time — do not rediscover

1. **Qt draws a corner square when `border-radius` exceeds half the height.** Every round-ended
   control has an exact height (`min-height` and `max-height` together) and a radius of half.
   `TestRoundEnds` in `tests/test_visual.py` looks at the corner pixel. A QSS `min-height` also
   *overrides* `setFixedSize`.
2. **A scroll area sizes its widget on resize, not when the layout grows.** `WeekGrid._pin_canvas_size`
   states the size outright. A `QGridLayout` also remembers column stretch after its widgets are gone
   (week → day left five phantom columns); `render_days` resets them.
3. **Qt counts a point as 1 px on macOS, 4/3 px on Windows.** The stylesheet's `pt` sizes are scaled
   by `theme.MAC_POINT_SCALE` (1.15) on a Mac.
4. **Hiding a widget only schedules a layout** — `ClassCard._fit_chips` re-lays out at once.
5. **Popups are square unless the window is translucent.** `ui/popups.py` swaps in a rounded tooltip
   and makes menus translucent. Written and unit-tested, but **only seen on a real Mac by the owner**;
   Windows behaviour is unverified.
6. **Tests that open a message box hang the suite.** Patch `QMessageBox`; see
   `tests/test_update_service.py`. The suite also forbids the network (`conftest._no_network`) and
   turns animations off (`conftest._no_motion`).
7. **The release switch is the version.** Bump with `tools/bump_version.py` (it writes the `.nsi` and
   `autopara/__init__.py`; a test fails if they differ). **Publishing a release ships an update** to
   every older install through the in-app check.

## Known gaps / candidate next work

- **Updater is untested end to end.** There are now several releases, so install an older one (1.6.0)
  and let it find the newest to exercise download → verify → install. The Windows
  apply step (`updater.windows_command`) has never run on Windows hardware.
- **macOS:** the image is arm64 only, unsigned and un-notarised (first launch: right-click → Open).
  The update path opens the `.dmg`; it does not replace the app itself.
- **Intel Mac** has no image. Add a second matrix entry to `build-mac` if it is wanted.
- **Dragging one date of a series** shifts the whole series; dragging a weekly template changes the
  template. There is no "move only this date".
- **Drag and drop** exists only in the week/day grid, not in the month.
- **Windows:** the Windows installer's UI strings and the redesigned look have not been seen on a
  Windows machine.

## Map of what was added in this line of work

| Area | Files |
|---|---|
| Dated lessons | `core/models.py` (`Lesson.occurs_on`), `core/storage.py` (`_migrate`, `on_date`, `repeat_until`), `core/scheduler.py` |
| Views | `ui/week_grid.py` (`render_days`), `ui/month_view.py`, `ui/nav_bar.py`, `ui/main_window.py` (`anchor`, `_animated`) |
| Dialogs | `ui/edit_dialog.py` (repeat modes), `ui/group_dialog.py`, `ui/settings_dialog.py` (updates), `ui/metrics.py` |
| Look | `ui/styles.qss`, `core/theme.py` (`PALETTES`, `scaled_points`), `ui/fonts/` (Inter), `ui/assets/logo.png` |
| Motion / popups | `ui/transitions.py`, `ui/popups.py` |
| Updates | `core/updater.py` (Qt-free), `core/update_service.py`, `ui/update_banner.py` |
| Release tooling | `tools/bump_version.py`, `docs/RELEASING.md`, `.github/workflows/installer.yml`, `build_mac.py` |
| New test files | `test_dated_lessons`, `test_navigation`, `test_views`, `test_edit_dialog`, `test_group_dialog`, `test_visual`, `test_motion`, `test_popups`, `test_updater`, `test_update_service`, `test_logo` |

## Conventions worth keeping

- Every visible string is Ukrainian (a test walks the widget tree); comments and docs are English.
- Docs are updated in the same commit as the code they describe; stale docs are a bug.
- A regression test is only trusted after it has been seen to fail without the fix (several bugs above
  were first "covered" by tests that passed either way).
