# Upgrading from 1.x (Python) to 2.0

What happens when someone who has the Python AutoPara (1.10.0 or older) installs 2.0.0, piece by
piece. The short version: **download `AutoPara-2.0.0-Setup.exe` and run it; everything comes
across.** On a Mac, drag the new `AutoPara.app` over the old one.

## How a 1.x user gets 2.0

**Not through the 1.x updater.** 1.10.0 asks the GitHub API under the repository's old name
(`sevcenkoa864-oss/AutoPara`). GitHub redirects that, so 1.x does see 2.0.0 and shows "Доступна нова
версія AutoPara 2.0.0", but the file links come back under `reyjendiz/AutoPara` and fail 1.x's
prefix check (PORT_SPEC §I.7). The strip therefore offers no download and no page, only
"Пропустити версію" and "Пізніше". Checked on 2026-10-28 against the live API: the answer for
`v1.10.0` carries `https://github.com/reyjendiz/AutoPara/releases/download/...` URLs.

The strip's detail line is the first non-empty line of the release notes, so the release notes
start with a plain sentence telling 1.x users where to download 2.0 (`rust-release.yml`).

A 1.10.1 Python release with the repository name fixed would let 1.x download and run the 2.0
installer itself. That means changing the Python app, which this rewrite does not do; it is the
owner's call (docs/HANDOFF.md).

## Windows: the installer

The 2.0 installer (`src-tauri/installer/installer.nsi`) installs over 1.x in place:

| What | 1.x | 2.0 | Result of installing 2.0 over 1.x |
|---|---|---|---|
| Install folder | `%LOCALAPPDATA%\Programs\AutoPara`, recorded in `HKCU\Software\AutoPara\InstallDir` | the same folder | The installer reads 1.x's `InstallDir` to find it, removes 1.x's `_internal` bundle, and writes `AutoPara.exe` over the old one. |
| Executable | `AutoPara.exe` | `AutoPara.exe` | Same path, so the "AutoPara is running" check (`CheckIfAppIsRunning` on `<folder>\AutoPara.exe`) finds a running 1.x copy and closes it after an OK. |
| Autostart | `HKCU\…\Run\AutoPara` = `"<folder>\AutoPara.exe" --hidden` | identical | The same value, same path: the entry now starts 2.0. |
| Start Menu, desktop shortcut | `AutoPara\AutoPara.lnk`, `Видалити AutoPara.lnk`, desktop `AutoPara.lnk` | same paths | Replaced. |
| Add/Remove Programs | `HKCU\…\Uninstall\AutoPara` | same key | Replaced; shows 2.0.0. |
| Uninstaller | `Uninstall.exe` | `uninstall.exe` | The same file (Windows names ignore case): replaced, and the Add/Remove entry points at it. |

So there is never a second copy to remove: the old program is replaced, not kept beside the new one.
That also settles PORT_SPEC §I.21 (both apps running and each opening every class) for the
installed case: the old executable and its autostart entry are gone.

**Not covered:** a 1.x copy run from source (`python -m autopara`) or installed into another
folder by hand. The 2.0 installer does not know about it, and its own autostart entry
(`pythonw.exe … autopara_launch.pyw --hidden`) is replaced by 2.0's only because both use the value
name `AutoPara`. If such a copy is started by hand it schedules from its own database and would open
classes too.

## macOS

`AutoPara.app` keeps its name and bundle identifier (`com.autopara.app`), and autostart keeps the
same LaunchAgent (`~/Library/LaunchAgents/com.autopara.app.plist`, label `com.autopara.app`). Drag
the new app over the old one in Applications and choose Replace; quit 1.x from its tray icon first.
The first start of the unsigned app needs the context menu → Відкрити, as with 1.x. Untested on a
Mac.

## Data

The data folder is the same: `%APPDATA%\AutoPara` (`~/Library/Application Support/AutoPara`). 2.0
keeps its own files there under new names and leaves 1.x's alone:

| | 1.x | 2.0 |
|---|---|---|
| Database | `autopara.db` | `autopara2.db` |
| The imported document | `schedules/` | `timetables/` |
| Log | `autopara.log` | `logs/autopara.<date>.log` |
| Update downloads | `updates/` | none in the data folder (the updater plugin handles its own download) |

On the first start with an empty `autopara2.db`, 2.0 imports 1.x's data once
(`autopara_core::legacy::import_legacy`):

- `autopara.db` is opened **read-only**. Courses, groups, lessons, the lesson–group links, the open
  log (`occurrences`) and moved dates (`lesson_skips`) are copied row for row with their ids, so the
  chosen course and group and every "opened / skipped" mark keep pointing at the same classes.
  Databases from before dated lessons existed (no `on_date`, no `lesson_skips`) import too.
- **Settings**: every key is copied as it is (lead time, class length, theme, catch-up mode,
  notifications, update mode, view, autostart, the selected course and group, the import time),
  except `schedule_copy_path`, which is pointed at the new copy.
- The archived `.docx` is copied from `schedules/` into `timetables/`.
- `legacy_imported_at` is stamped, so the import never runs again, also when there was nothing to
  import or the new database already had a timetable.

Nothing of 1.x's is modified or deleted, so going back stays possible: reinstalling 1.10.0 finds its
own `autopara.db` exactly as it was (without anything changed in 2.0 since). The old files are kept
rather than offered for removal because they are small and they are the way back; the uninstaller's
"delete my data too" removes the whole folder, old files included.

If the import fails, 2.0 logs the error and starts with an empty timetable; the user can import the
`.docx` again, and 1.x's files are still untouched.

### Checked on a copy of real data

On 2026-10-28 a copy of a real 1.10.0 data folder (6 courses, 11 groups, 86 lessons, 3 open-log rows,
16 settings) was put in a scratch folder and the 2.0 app started on it with `AUTOPARA_DATA_DIR`:

- the log says `imported the previous version's data` with those counts;
- every table compares equal row for row with the old database (ids included);
- every setting is equal, plus `legacy_imported_at`; `schedule_copy_path` points into `timetables/`;
- the copied document is byte-identical, and `autopara.db` is byte-identical to the original.

The installer over a real 1.x install has **not** been run: doing that on the development PC would
replace the owner's working 1.x. It belongs on a clean VM with 1.10.0 installed first
(docs/RELEASE_CHECKLIST.md, "Upgrade from 1.x").

## What changes for the user

Behaviour stays the same except where 1.x was wrong; `rust/PARITY_REPORT.md` lists every item.
The ones a user may notice:

- A class that is already running when the app starts, wakes or the clock jumps is always offered,
  never opened, even in "open immediately" mode (1.x opened it 15 s later).
- `http://` links are no longer opened or accepted; only `https://`.
- The class-length setting now applies to imports.
- On macOS, updates install themselves and restart (1.x opened the disk image).
- From 2.0 on, updates are downloaded from this repository and signature-checked.
- New: Lecture Assist, off until the user turns it on (docs/ASSIST.md).
