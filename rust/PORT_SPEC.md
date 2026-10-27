# PORT_SPEC — behaviour of the Python app that the Rust rewrite must keep

Source of truth for the port. Every claim names the file and function it comes from, so it can be
checked. Python wins over the Rust code unless `rust/DECISIONS.md` records the difference and why.

Reference state: branch `main` at `8e73aff`, version **1.10.0** (`installer/AutoPara.nsi`
`APP_VERSION`, `autopara/__init__.py` `__version__`). Paths below are relative to the repository root.

Where the code and the docs disagree, the code is described and the disagreement is listed in §I.

---

## A. `.docx` parsing

Two layers: `autopara/importer/docx_reader.py` (zip/XML → cell grid) and
`autopara/importer/schedule_parser.py` (cell grid → courses, groups, lessons), with text helpers in
`autopara/importer/normalize.py`. No third-party docx library.

### A.1 Reading the package (`docx_reader.py`)

- Parts read: `word/document.xml` (required) and `word/_rels/document.xml.rels` (optional; missing →
  no links). `_read_relationships` keeps a relationship only when its `Type` attribute contains the
  substring `hyperlink` and it has both `Id` and `Target`. The map is `rId → Target`, unmodified.
- `read_document`: walks only the **direct children** of `w:body`:
  - `w:p` → its lines (see below) joined with one space and stripped; kept as a paragraph block
    only when non-empty.
  - `w:tbl` → a table block. Any other element (e.g. `w:sectPr`, `w:sdt`) is ignored.
  - Block order is document order; courses are matched to tables by this order.
- `_paragraph_segments(p)`: iterates **all descendants** of the paragraph in document order:
  `w:t` → its text (empty if none); `w:br` and `w:cr` → `"\n"`; `w:tab` → `" "`;
  `w:hyperlink` with an `r:id` present in the rels map → the target is appended to the links list.
  The joined text is split on `"\n"`, each piece is `strip()`ped, empty pieces are dropped.
- `_parse_table`: `grid_columns` = number of `w:tblGrid/w:gridCol` (0 if no grid). Rows are the
  direct `w:tr` children; cells are the direct `w:tc` children of each row.
- `_parse_cell`: `w:tcPr/w:gridSpan@w:val` → `span = max(1, int(val))`, non-integer → 1;
  `w:tcPr/w:vMerge` present → `vmerge = @w:val or "continue"` (an omitted value means continue, per
  OOXML). Lines/links come from the **direct** `w:p` children of the cell only, so a table nested
  inside a cell is ignored.
- **R1 logical columns**: `_parse_row` gives each cell `column = sum(span of preceding cells)`.
  `Cell.last_column = column + span - 1`. `Row.at(c)` returns the cell whose range covers `c`.
- `Cell.overlaps(lo, hi)` = ranges intersect. `Cell.is_empty` = no lines.

### A.2 Text normalisation (`normalize.py`)

- `normalize_text(s)`: Unicode **NFC**; each of `` ` `` `’` `ʼ` `ʹ` `‘` `´` replaced by `'`; runs of
  whitespace (`\s+`) collapsed to one space; stripped. Used before every comparison (R8).
- Comparison keys use `normalize_text(...).casefold()` (Python full case folding).

### A.3 Day-name recognition (`normalize.day_index`, R4/R8/R10)

- Input is the **first line** of the day cell (`schedule_parser._parse_table`).
- Key: `normalize_text(value).casefold()` then strip the characters ` .,:;-–—()[]` from both ends.
  Empty key → no day.
- Exact alias match first; otherwise the longest alias (length ≥ 3, after normalisation) that the
  key **starts with**. Aliases sorted by length descending, ties in insertion order.
- Alias table (`_register`, all normalised + casefolded; index 0 = Monday):

| Day | Aliases |
|---|---|
| 0 | понеділок, понеділка, пн, понедельник, monday, mon, poniedzialek, poniedziałek, montag, lunes, lundi, lunedi, lunedì, luni |
| 1 | вівторок, вт, вторник, tuesday, tue, tues, wtorek, dienstag, martes, mardi, martedi, martedì, marti |
| 2 | середа, середи, ср, среда, wednesday, wed, sroda, środa, mittwoch, miercoles, miércoles, mercredi, mercoledi, mercoledì |
| 3 | четвер, чт, четверг, thursday, thu, thur, thurs, czwartek, donnerstag, jueves, jeudi, giovedi, giovedì, joi |
| 4 | п'ятниця, п'ятниці, пт, пятница, friday, fri, piatek, piątek, freitag, viernes, vendredi, venerdi, venerdì, vineri |
| 5 | субота, сб, суббота, saturday, sat, sobota, samstag, sonnabend, sabado, sábado, samedi, sabato, sambata, sâmbătă |
| 6 | неділя, нд, вс, воскресенье, sunday, sun, niedziela, sonntag, domingo, dimanche, domenica, duminica, duminică |

  Languages: Ukrainian, Russian, English, Polish, German, Spanish, French, Italian, Romanian.
  Deliberately absent: two-letter abbreviations shared across languages (German `so`, Polish `sob`…).
  Two-letter aliases (`пн`, `вт`, …) match only exactly, never as a prefix.
- A day cell whose first line is not recognised leaves the carried-forward day unchanged (R4).
- Output names are always Ukrainian: `DAY_NAMES` = Понеділок, Вівторок, Середа, Четвер, П'ятниця,
  Субота, Неділя; `DAY_SHORT` = Пн Вт Ср Чт Пт Сб Нд; `DAY_ACCUSATIVE` = понеділок, вівторок, середу,
  четвер, п'ятницю, суботу, неділю (lower case).

### A.4 Times and pairs (`normalize.py`, R6/R11)

- `parse_time_range(s)`: on `normalize_text(s)`, find every `(\d{1,2})\s*[.:]\s*(\d{2})`; keep those
  with hour < 24 and minute < 60 as `HH:MM`. Start = first kept; end = second kept **only if it is
  lexicographically greater** than the start (zero-padded, so this is chronological). Otherwise no end.
- `parse_time(s)`: whole string must match `^(\d{1,2})[.:](\d{2})$` → `HH:MM`, else none.
- `TIME_TO_PAIR`: `8.00→1, 9.30→2, 11.20→3, 13.00→4, 14.40→5, 16.10→6, 17.40→7, 19.10→8, 20.40→9,
  22.10→10`. `ACADEMIC_PAIRS = 6`. `DEFAULT_CLASS_MINUTES = 80`.
- `pair_from_time(s)`: exact slot lookup (`.`/`:` both accepted, hour without leading zero), else none.
- `pair_slot(hhmm)`: exact slot if any, else the highest pair whose start `<=` hhmm (string
  compare), else 1.
- `last_pair_covered(start, end)`: max of `pair_slot(start)` and every pair whose start lies in
  `[start, end)`.
- `add_minutes(hhmm, m)`: clamps at `23:59`. `minutes_between(a, b)` = `max(0, b - a)` in minutes.

### A.5 Table → course (`schedule_parser._parse_table`)

- Constants: `FIRST_GROUP_COLUMN = 3` (logical columns 0, 1, 2 are День, Пара, Час),
  `HEADER_ROWS = 2`.
- **Groups (R2)** — `_header_groups`: fewer than 2 rows → no groups. For each cell of row 0 with
  `column >= 3` and non-empty: name = `normalize_text(" ".join(lines))`, range =
  `(column, last_column)`. Specialty = row 1's cell at `col_lo`, same normalisation, or `""`.
  A table without groups yields a course with no lessons.
- For each row after the header rows:
  1. Day cell = `row.at(0)`; if non-empty and its first line resolves, it becomes the current day
     (R4: carried forward otherwise).
  2. Time cell = `row.at(2)`; if non-empty, `parse_time_range(first line)`. No start time, or no
     current day yet → skip the row entirely (vMerge continuations in it are not applied).
  3. Pair = `int(first line of row.at(1))` if that normalised text `isdigit()`, else
     `pair_from_time(start) or pair_slot(start)`.
  4. `row_end = explicit end or add_minutes(start, class_minutes)`.
  5. For every cell with `column >= 3`, in row order:
     - `vmerge == "continue"` → if a block was opened at **this cell's column**, set its
       `end_time = row_end` (R5); nothing else.
     - empty cell → close any block at this column.
     - otherwise split the content (A.6); no subject → close the block and skip.
     - Create a lesson: day, pair, start, `row_end`, subject, teacher, url, provider,
       `group_names = every group whose range overlaps the cell` (in group order),
       `linked_cells = 1 if url else 0`.
     - `vmerge == "restart"` → this lesson is the open block for the column; else close it.
- After all rows: `lessons = _merge_consecutive_slots(_dedupe_adjacent_duplicates(lessons))`.
- A `restart` cell with no content is merged empty space (R5) and is skipped by the empty-cell rule.

### A.6 Cell content (`_cell_content`, R3/R9)

- URLs = the cell's hyperlink targets (relationship order), then URLs found in each line by
  `https?://[^\s<>"']+`, each with trailing characters from `.,;:)]}` stripped (`clean_url`).
- Display lines: a line containing URLs has each URL replaced by a space, then is normalised; kept
  if non-empty. Other lines are normalised and kept.
- Teacher = the first line that, normalised, starts with `(` and ends with `)`; **stored with its
  parentheses** (the UI strips them for display). Subject = the first non-teacher line; if none,
  the first line.
- URL = first element of the order-preserving dedupe of all URLs (empties dropped), or none.
- `detect_provider(url)`: lower-cased URL containing `meet.google.com` → `google_meet`; containing
  `zoom.us` or `zoom.com` → `zoom`; else `unknown`. No URL → `unknown`.
- `needs_link` = no URL (R7). Such lessons are never auto-opened.

### A.7 Shared classes and duplicates (R2)

- Primary rule: one cell spanning several groups' ranges is one lesson tagged with all of them
  (A.5). It opens once because it is one row (one `occurrences` key).
- Fallback `_dedupe_adjacent_duplicates`: lessons with an equal key
  `(day, pair, subject fold, teacher fold, url or "")` are merged into the first one: group names
  are appended (no repeats, order kept) and `linked_cells` summed. Applies to any two lessons in the
  same table with that key, not only adjacent columns, and runs **before** R12.

### A.8 Consecutive slots (`_merge_consecutive_slots`, R12)

- Lessons are visited in creation order (row by row, left to right). Key:
  `(day, tuple(sorted(group_names)), subject fold)`; one open block per key.
- `_continues(block, next)`: same sorted group list; teachers compatible (equal after
  normalise+casefold, or either blank); URLs not both present and different; `next.start >=
  block.end`; and either `next.pair == last_pair_covered(block.start, block.end) + 1` or
  `next.start == block.end`.
- On continue: `block.end = max(block.end, next.end)`; blank teacher/url on the block are filled
  from `next` (provider recomputed); `linked_cells` summed; `next` is dropped.
  Otherwise `next` is kept and becomes the open block for its key.
- Reference-document facts this must reproduce (BACKEND.md §1): 77 rows → 61 lessons; per course
  24/10/8/10/0/9. The synthetic document's numbers are pinned by `tests/test_parser.py`.

### A.9 Courses (`parse_document`, R8/R10)

- One course per table in document order; `ordinal` starts at 1; paragraphs are ignored.
- Name = `course_label(ordinal)` = Roman numeral (`I`…`VIII`, else the decimal number) + ` курс`.
  The heading text is never used.
- `looks_like_schedule` / `is_course_heading` (course words: курс, kurs, course, year, rok, jahrgang,
  año, ano, anno) exist but nothing in the app calls them.
- `parse_file(path, class_minutes=80)`. The import dialog calls it **without** `class_minutes`
  (see §I).

### A.10 Identity across re-imports and repeat rules

- `ParsedLesson.source_key(ordinal)` = `f"{ordinal}|{day}|{pair}|{normalize_text(subject).casefold()}"`.
- Imported lessons are weekly templates (`on_date` NULL) and are never manual.
- Manual lessons (edit dialog) are one of three kinds — *Один раз* (`on_date`, no `repeat_until`),
  *Щотижня до дати* (`on_date` + `repeat_until`), *Щотижня, без кінця* (weekly template,
  `is_manual = 1`).
- `Lesson.occurs_on(day)` (`core/models.py`) is the single rule: weekday must equal `day_index`;
  `day` not in `skip_dates`; a template occurs every week; a dated lesson occurs on
  `on_date <= day <= max(repeat_until or on_date, on_date)`.
- `Lesson.repeats` = template, or `repeat_until > on_date`.

---

## B. Data model and storage

`autopara/core/storage.py`, dataclasses in `autopara/core/models.py`.

### B.1 Schema (`SCHEMA`, executed on every open with `PRAGMA foreign_keys = ON`)

```sql
settings(key TEXT PRIMARY KEY, value TEXT)
courses(id INTEGER PRIMARY KEY AUTOINCREMENT, ordinal INTEGER NOT NULL UNIQUE, name TEXT NOT NULL)
groups(id INTEGER PRIMARY KEY AUTOINCREMENT,
       course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
       name TEXT NOT NULL, specialty TEXT NOT NULL DEFAULT '',
       col_lo INTEGER NOT NULL DEFAULT 0, col_hi INTEGER NOT NULL DEFAULT 0,
       UNIQUE(course_id, name))
lessons(id INTEGER PRIMARY KEY AUTOINCREMENT,
        course_id INTEGER NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
        day_index INTEGER NOT NULL, pair INTEGER NOT NULL,
        start_time TEXT NOT NULL, end_time TEXT NOT NULL,          -- 'HH:MM'
        subject TEXT NOT NULL, teacher TEXT NOT NULL DEFAULT '',
        url TEXT, provider TEXT NOT NULL DEFAULT 'unknown',
        needs_link INTEGER NOT NULL DEFAULT 0, is_manual INTEGER NOT NULL DEFAULT 0,
        source_key TEXT NOT NULL DEFAULT '',
        on_date TEXT, repeat_until TEXT)                            -- 'YYYY-MM-DD' or NULL
lesson_groups(lesson_id → lessons(id) ON DELETE CASCADE, group_id → groups(id) ON DELETE CASCADE,
              PRIMARY KEY (lesson_id, group_id))
occurrences(id INTEGER PRIMARY KEY AUTOINCREMENT,
            lesson_id INTEGER NOT NULL REFERENCES lessons(id) ON DELETE CASCADE,
            occur_date TEXT NOT NULL, status TEXT NOT NULL, fired_at TEXT NOT NULL,
            UNIQUE(lesson_id, occur_date))
lesson_skips(lesson_id → lessons(id) ON DELETE CASCADE, skip_date TEXT NOT NULL,
             PRIMARY KEY (lesson_id, skip_date))
INDEX idx_lessons_day ON lessons(day_index)
INDEX idx_occurrences_date ON occurrences(occur_date)
INDEX idx_lessons_on_date ON lessons(on_date)      -- created by _migrate
```

- Statuses (`models.py`): `opened` (auto-opened), `manual` (user opened, card click or catch-up
  connect), `missed`, `skipped`.
- Providers: `zoom`, `google_meet`, `unknown`.
- Journal mode is SQLite's default (not WAL). No `user_version`; there is no migration table.
- **Migrations** (`Storage._migrate`, runs on every open): adds `lessons.on_date` and
  `lessons.repeat_until` with `ALTER TABLE` when `PRAGMA table_info(lessons)` lacks them; creates
  `idx_lessons_on_date`. `lesson_skips` arrives through `CREATE TABLE IF NOT EXISTS`.
- Stored dates that do not parse are read as NULL with a warning (`_date_or_none`).
- `fired_at` = local time, ISO, seconds precision.

### B.2 Operations and their semantics

- `import_courses(parsed, source_path)` — one transaction:
  - courses upserted by `ordinal` (name updated); **courses absent from the new document are kept**;
  - groups upserted by `(course_id, name)` (specialty and range updated); **vanished groups are kept**;
  - imported lessons (`is_manual = 0`) whose `source_key` is not in the new document are deleted
    (cascade removes their groups, occurrences and skips); manual lessons are untouched;
  - each incoming lesson updates the row with the same `(course_id, source_key, is_manual = 0)` or
    is inserted; its `lesson_groups` rows are rewritten (unknown group names ignored).
  - After commit: **every** occurrence row is deleted (`clear_occurrences`);
    `schedule_active_from` = now (local, ISO seconds); `last_import_path` = source path;
    `schedule_copy_path` = archive result when non-empty. Failure inside the transaction rolls back
    and re-raises.
- `archive_schedule(src)`: if `src` is a file, delete every other file in `schedules/` (never the
  source itself), copy `src` there under its own file name (skipped when it already is that file),
  return the destination; on OS error log and return `""`.
- `lessons_for_group(group_id)`: lessons joined through `lesson_groups`, ordered by
  `day_index, pair`; each hydrated with group names (ordered by `col_lo`) and skip dates.
- `add_lesson(lesson, group_ids)`: `is_manual = 1`, `source_key = ''`, `day_index =
  on_date.weekday()` when dated, `needs_link = not url`.
- `update_lesson(lesson, group_ids=None)`: same derivations; groups replaced only when given.
- `move_lesson(id, day_index, pair, start, end, on_date=None)`: dated lesson + `on_date` given →
  move to that date and shift `repeat_until` by the same number of days; otherwise update
  `day_index` and times only.
- `move_occurrence(id, from_day, to_day, pair, start, end)`: insert `(id, from_day)` into
  `lesson_skips`, then add a manual one-off copy on `to_day` with the same course, subject, teacher,
  url, provider and groups. Returns the copy's id.
- `delete_lesson(id)`: deletes the row (cascades).
- `claim_occurrence(id, day, status)`: plain `INSERT`; `IntegrityError` → rollback, `False`.
- `mark_occurrence(id, day, status)`: upsert (status and `fired_at` overwritten).
- `clear_occurrence(id, day)`, `clear_occurrences()`, `set_occurrence_status(id, day, status)`.
- `occurrence`, `occurrences_on(day)` (by lesson id), `occurrences_between(a, b)` (by
  `(lesson_id, iso_date)`, inclusive).

### B.3 Data locations

- `default_db_path()`: `%APPDATA%\AutoPara\autopara.db` when `APPDATA` is set (Windows); else on
  macOS `~/Library/Application Support/AutoPara/autopara.db`; else `~/AutoPara/autopara.db`. The
  directory is created on demand.
- Beside the database: `autopara.log` (INFO, file handler only, `%(asctime)s %(levelname)s
  %(name)s: %(message)s`, `app._configure_logging`); `schedules/` (the one archived `.docx`);
  `updates/` (downloaded installers, `updater.updates_dir`); `assets/` (`check-<theme>.png`,
  `chevron-<up|down>-<theme>.png` painted by `theme.checkmark_icon` / `chevron_icon`).
- Registry (Windows, all HKCU): `Software\AutoPara\InstallDir` (written by the installer, read by
  `core/refresh.py`); `Software\Microsoft\Windows\CurrentVersion\Run` value `AutoPara`;
  `Software\Microsoft\Windows\CurrentVersion\Uninstall\AutoPara`.
- macOS autostart: `~/Library/LaunchAgents/com.autopara.app.plist` (`Label` `com.autopara.app`,
  `ProgramArguments` = `[<executable>, --hidden]` frozen or `[<python>, <abs autopara_launch.pyw>,
  --hidden]` from source, `RunAtLoad` true).

---

## C. Scheduler

`autopara/core/scheduler.py`, `autopara/core/launcher.py`, wiring in `autopara/app.py`.

### C.1 Loop

- A timer fires every **15 s** (`TICK_SECONDS`); `start()` also ticks immediately. All times are
  naive local wall-clock (`datetime.now()`); there is no time-zone or monotonic-clock handling.
- `tick(now)`: `today = now.date()`; settings re-read every tick; `fired = occurrences_on(today)`;
  lesson set = `lessons_for_group(selected_group_id)` filtered by `occurs_on(today)` (empty when no
  group is selected). `cold = self._cold_start`, then `_cold_start = False`.
- For each lesson:
  1. `predates_schedule(lesson, today, active_from)`: `starts_on(today) < schedule_active_from` →
     skip entirely (not opened, not offered, not marked).
  2. Reminder (only when `notifications_enabled`): `reminder_due(lesson, now, notify_minutes,
     already_fired)` = not fired, `notify_minutes > 0`, occurs today, `start - notify <= now <
     start`. Emitted once per `(lesson, date)` per process (in-memory set `_reminded`). Never
     written to the database.
  3. `evaluate(lesson, now, lead, already_fired)` (pure):

| Condition | Action |
|---|---|
| already has an occurrence row today | none |
| not `occurs_on(today)` | none |
| `now < start - max(0, lead)` | none |
| `start - lead <= now < start` | **open** |
| `start <= now < end` | **catch-up** |
| `now >= end` | **miss** |

  4. `_perform`:
     - open → `_open(status = opened)`.
     - catch-up → link not openable → claim `missed`; `catchup_mode == open` **and not the cold
       tick** → `_open(opened)`; `catchup_mode == missed` → claim `missed`; otherwise (notify, the
       default, and the cold tick always) → `_offer` (once per `(lesson, date)` per process,
       in-memory `_notified`) which emits `catchup_available`.
     - miss → claim `missed` (emits `lesson_missed` when the claim succeeded).
- `_open`: unopenable link → claim `missed`. Otherwise **claim first** (`claim_occurrence`); claim
  lost → do nothing. Then open; success → `lesson_opened`; failure → the row stays, its status is
  set to `missed`, `lesson_missed` emitted (no retry storm).

### C.2 Invariants

- **Open once**: guaranteed by `UNIQUE(lesson_id, occur_date)` plus claim-before-open.
- **Cold start never ambushes**: the first tick after `start()` treats catch-up as notify whatever
  the setting (`TestColdStartNeverAmbushes`, `TestLaunchingIntoAMissedDay`).
- **Schedule starts at import** (`predates_schedule`, `TestScheduleStartsWhenImported`).
- Reminder and trigger are separate pure functions; a reminder can never open anything.
- Lessons without a link are never auto-opened.
- A user mark (`opened`/`skipped`/`missed`) is an occurrence row, so the class is never opened
  that day afterwards.

### C.3 Manual paths

- `open_now(id, day=today, claim=True)`: lesson missing or link not openable → `False`.
  `claim=False` → open without touching the log (used for a date after today). `claim=True` →
  `claim_occurrence(manual)` (result ignored), open, on success set status `manual` and emit
  `lesson_opened`.
- `mark(id, status, day)`: `mark_occurrence` + adds `(id, day)` to `_notified`.
- `clear_mark(id, day)`: deletes the row and removes it from `_notified`.
- Main window (`ui/main_window.py`): a click opens with `claim = (day <= today)`.

### C.4 Sleep, restart, clock changes

- Nothing special: polling re-evaluates every 15 s; restart starts with a cold tick; the database
  row prevents a second open on the same date whatever the clock does.
- A sleep across a class start with `catchup_mode = open` **does** open the running class on the
  first tick after wake (it is not a cold tick). See §I.

### C.5 Link opening (`launcher.py`)

- `is_openable(url)`: non-empty and `url.strip().lower()` starts with `http://` or `https://`.
- `open_url`: refuses unopenable values; drops a repeat of the same stripped URL within **3.0 s**
  (monotonic) and reports success; Windows → `ShellExecuteW(None, "open", url, …, SW_SHOWNORMAL)`,
  success when the return value is > 32, **no** fallback to `webbrowser`; macOS → `open <url>`
  (5 s timeout), falling back to `webbrowser.open(new=2)`; other platforms → `webbrowser`.

### C.6 Notifications (`app.py`, tray messages)

| Event | Title | Body | Shown when |
|---|---|---|---|
| reminder | Скоро пара | `{subject} о {start}` (10 s) | reminder emitted (only computed when notifications on) |
| auto/manual open | Відкриваю пару | `{subject} — {start}` (6 s) | notifications on |
| catch-up | Пара вже почалася | `{subject} почалася о {start}. Відкрийте AutoPara, щоб підключитися.` (12 s) | notifications on; the window is brought forward and the banner queued regardless |
| update found | Доступна нова версія | `AutoPara {version}` (10 s) | notifications on |
| update downloaded | Оновлення готове | `AutoPara {version} завантажено. Натисніть, щоб встановити.` (10 s) | window shown always; message when notifications on |

Clicking a tray message shows the window.

### C.7 "Now / next" pill (`core/nextup.py`)

- `find(lessons, now, settled)`: today's lessons (`occurs_on`), not in `settled`, with `end > now`;
  pick the earliest `start_time`. Ongoing (`start <= now`) → minutes to end, else minutes to start,
  each `max(1, round(seconds / 60))`. `settled` = lesson ids with a `skipped` row today.
- `label`: `Зараз · {subject} · ще {m} хв`; `Далі · {subject} · через {m} хв` when `m <= 90`, else
  `Далі · {subject} · о {start}`. Tooltip `{subject}\n{start}–{end}[\n{teacher without parens}]`.
- Refreshed every 30 s; hidden when nothing qualifies or the navigation bar is narrower than 1000 px.
- Always computed for the **chosen** group, even while another group is shown.

### C.8 Single instance

- `app._already_running`: connect to the local socket `AutoPara.SingleInstance` (300 ms); if it
  answers, write `show` and exit 0. Otherwise the running instance listens on it and any connection
  surfaces the window (`show_window`: reload, show, raise, activate).

---

## D. Settings and defaults

`settings` table, seeded by `Storage._seed_defaults` with `INSERT OR IGNORE`; typed view
`Storage.settings()`.

| Key | Default | Read as | UI |
|---|---|---|---|
| `selected_course_id` | — | int or none | import / group dialog |
| `selected_group_id` | — | int or none | import / group dialog |
| `lead_minutes` | `1` | int, bad → 1 | spin 0–120, suffix « хв до початку» |
| `class_duration_minutes` | `80` | int, bad → 80 | spin 10–240, suffix « хв» |
| `autostart_enabled` | `1` | `== "1"` | checkbox; the registry/plist is the source of truth when the dialog opens |
| `catchup_mode` | `notify` | any string; empty → notify | 3 radio buttons |
| `notifications_enabled` | `1` | `== "1"` | checkbox |
| `notify_minutes` | `10` | int, bad → 10 | spin 1–120 (disabled when notifications are off) |
| `theme` | `system` | any string; empty → system; unknown resolves like system | combo / rail toggle |
| `view_mode` | `week` | `day`/`week`/`month`, else week | segmented control |
| `update_mode` | `ask` | `ask`/`auto`/`off`, else ask | combo |
| `skipped_update_version` | `""` | string | "Пропустити версію" |
| `last_update_check` | `""` | ISO datetime | written after each answer |
| `schedule_active_from` | `""` | ISO datetime; bad → none | written on import |
| `schedule_copy_path` | `""` | path | written on import |
| `last_import_path` | `""` | path | written on import |

- On every launch `app.run` calls `autostart.sync(autostart_enabled)` (rewrites a stale command,
  removes the entry when disabled) and stores back what the registry/plist actually says.
- Not persisted: the date on screen (`MainWindow.anchor`, today at launch), window size
  (`resize(1240, 800)` at construction) and position, the "other group" filter, search text.
- Autostart command (`autostart.startup_command`): frozen → `"<exe>" --hidden`; from source →
  `"<pythonw.exe or python.exe>" "<repo>\autopara_launch.pyw" --hidden`.

---

## E. UI inventory

All visible text is Ukrainian (`tests/test_ui.py::test_interface_is_ukrainian` allows only the Latin
words AutoPara, Zoom, Meet, docx). Subject and teacher text is data and shown verbatim.

### E.1 Main window (`ui/main_window.py`)

- Title `AutoPara — автозапуск пар` (must equal the installer's `APP_DISPLAY`). Initial size
  1240×800. Closing hides to the tray; only the tray's quit exits.
- Layout: rail column (80 px) | content (margins 0, 20, 24, 24; spacing 16): navigation bar, filter
  bar, update strip, catch-up banner, then the grid or the month or the import landing.
- States (`reload`):
  - no group selected → landing in "no schedule" state, nav and filter hidden, mark tooltip
    `Розклад ще не імпортовано`;
  - chosen group has no lessons at all → landing titled `Тут поки порожньо`, text `У групі {group}
    немає пар в імпортованому розкладі. Натисніть «Додати пару» або імпортуйте інший файл.`;
  - otherwise the view from `view_mode`.
- Rail (`_build_sidebar`, top→bottom): app mark 30 px (tooltip = course name, group name,
  specialty, `відкриття за {lead} хв`, one per line); stretch; black pill (52 px wide) holding:
  round add button 40 px (`Додати пару`, opens the editor as a one-off on the date on screen);
  icon buttons 36 px: import (`Імпортувати розклад…`), group (`Обрати інший курс і групу…`,
  disabled until something is imported), settings (`Налаштування`), theme (`Перемкнути на темну
  тему` / `Перемкнути на світлу тему`, icon moon / sun); under the pill the clock (`Поточний час`,
  `HH:MM:SS`, 1 s tick, 50 % opacity). No text on the rail.
- Theme toggle pins the opposite of what is showing (`theme.next_theme`).

### E.2 Navigation bar (`ui/nav_bar.py`)

- Title + quiet year: day `{DayName}, {d} {month genitive}` + year; week (`week_parts`)
  `{d1}–{d2} {month}` / `{d1} {month1} – {d2} {month2}` / across years `{d1} {month1} {year1} –
  {d2} {month2}` + `{year2}`; month `{Month nominative}` + year.
- "Now/next" pill (C.7), 28 px high.
- Segmented control `День` · `Тиждень` · `Місяць` (40 px, segments 32 px, gliding thumb 220 ms).
- `Сьогодні` (tooltip `Повернутися до сьогодні`), arrows with tooltips `Попередній {день|тиждень|
  місяць}` / `Наступний …`. Stepping: day ±1 day, week ±7 days, month via `shift_month` (clamps to
  the month's last day).

### E.3 Filter bar (`ui/filter_bar.py`, `core/filters.py`)

- Search field, placeholder `Пошук: предмет або викладач`, tooltip `Знайти пару за предметом чи
  викладачем\nEnter — до наступного збігу · Shift+Enter — до попереднього · Esc — очистити`; a
  search glyph at the start, a clear glyph at the end while text is present.
- Subject combo (min 180 px, tooltip `Показати лише один предмет`, first item `Усі предмети`),
  subjects = distinct by fold, alphabetical in Ukrainian order (`filters.alphabetical`, alphabet
  `абвгґдеєжзиіїйклмнопрстуфхцчшщьюяabcdefghijklmnopqrstuvwxyz`, other characters after it by code
  point).
- Group combo (min 200 px, tooltip `Показати розклад іншої групи цього курсу`), visible only when
  the course has more than one group; choosing another group shows its lessons without changing
  the chosen group; reset when the chosen group changes.
- Summary: `Знайдено {n} з {total}` / `Нічого не знайдено` / after a failed jump `Збігів більше
  немає`; hidden when no filter is active.
- `Скинути` (tooltip `Прибрати пошук і фільтри`), visible while a filter is active; clears text and
  subject, keeps the group.
- Matching (`LessonFilter.matches`): subject filter = fold-equal subject; every whitespace-separated
  word of the folded query must be a substring of `fold(subject) + " " + fold(teacher)`.
- Jump (`find_match`): from the last visible date forwards (Enter) or the first visible date
  backwards (Shift+Enter), day by day up to 400 days, to the first date with a matching lesson; the
  anchor moves there (animated).

### E.4 Week and day grid (`ui/week_grid.py`)

- Rows: hours 08:00–18:00 (11 rows, `HOUR_HEIGHT = 66` px, labels `HH:00` right-aligned in a 74 px
  gutter). Header 72 px: weekday name (short `Пн`… in the day view's corner) over the date number in
  a 32 px circle; today's circle is the ink pill; date tooltip `{d} {month genitive} {year}`.
- Week: Monday–Saturday; Sunday added only when a shown lesson occurs on it. Day: one column, name
  and date in the corner over the gutter.
- Columns stretch equally, min width 150 px; the canvas is pinned to `74 + 150·columns` ×
  `72 + 11·66`.
- Placement (`span_for`, pure): start clamped to `[08:00, 19:00)`; end = `min(19:00, max(end,
  start + 26 min))`; rows from the hours the class touches; top/bottom margins in minutes
  (bottom at least 1); pixels = `round(minutes · 66 / 60)`. The card's slot is exactly
  `span · 66` px tall; card margins 4 px left/right.
- Empty hour: tooltip `{DayName}, {d} {month} · HH:00` and, on a second line, `Клацніть, щоб
  створити пару`. Left click → menu
  `➕ Створити пару · {DayName}, {d} {month}, HH:00` / separator / `Скасувати`; choosing it opens the
  editor as a one-off on that date and hour. Without a selected group: message `Спершу імпорт` /
  `Імпортуйте розклад, перш ніж додавати пари.`
- Today's column is tinted; hover tints a cell; a drag target cell shows a dashed accent ring.
- "Now" line: 2 px ink rule across today's column at the current minute plus a badge `HH:MM` in the
  gutter; 50 % opacity; refreshed every 30 s; hidden when today is not shown or the time is outside
  08:00–19:00.
- "Next" outline: week → today's earliest lesson with `end > now` and no occurrence row; day view
  → same, keyed by date, only when the day shown is today.

### E.5 Class card (`ui/class_card.py`)

- Chip row (priority order when narrow: state, provider, group; the first always shows): state chip
  `✓ відкрито` (opened or manual) / `не відкрито` (missed) / `пропущено` (skipped) / `без
  посилання` (no link); provider chip `Zoom` / `Meet` / `Посилання` (unknown; none without a link);
  group chip `{n} групи|груп` (only when shared; tooltip `Спільна пара: {groups}`).
- Then subject (bold), teacher (parentheses stripped), optional detail line (`Групи: …` when
  shared, else the link's host), stretch, time `HH:MM–HH:MM`.
- **Line dropping** (`LAYOUTS`, first that fits the given height): (subject lines, teacher, time,
  detail) = (3,T,T,T) (3,T,T,F) (2,T,T,F) (2,T,F,F) (2,F,T,F) (2,F,F,F) (1,T,T,F) (1,T,F,F)
  (1,F,T,F) (1,F,F,F). Needed height = `2·4 + chip row + n·subject line + teacher + time + detail +
  (rows−1)·2`. Without a height → (2,T,T,F). Text is wrapped and cut in whole lines, the last ending
  in `…` (`clamp_lines`).
- States: normal (tinted with the subject colour), next (2 px ink outline), opened (muted fill,
  muted subject), missed (red bar, warning fill), skipped (dashed, muted), no link (dashed, amber
  fill). Bar: 3 px, inset 6 px from the left and 9 px top/bottom.
- Tooltip lines: subject; teacher; `HH:MM–HH:MM`; `Групи: …` (when any); the URL or `Без
  посилання`; `Клац — підключитися · правий клац — меню · перетягніть, щоб перенести`.
- Left click (no drag) → join: open the link (claim only for today or earlier). Without a link the
  actions menu opens instead. On failure: `Не вдалося відкрити` / `Посилання не вдалося відкрити у
  браузері.`
- Right click or the Menu key → actions menu: `Відкрити посилання` (or `Додати посилання…` when
  there is no link), `Редагувати…`, separator, `Позначити як відкриту`, `Позначити як пропущену`,
  `Зняти позначку` (only when the date has an occurrence row), separator, `Видалити пару`.
- Delete: `Видалити пару` / `Видалити «{subject}» з розкладу?`; for a series `«{subject}»
  повторюється щотижня до {d} {month}. Видалити всю серію?\n\nЩоб пропустити лише одну дату,
  оберіть «Позначити як пропущену».` Yes/No.
- Drag (past the platform drag distance) carries `application/x-autopara-lesson` = `"{id}|{iso
  date}"` (or `"{id}"`). Drop on an hour: keep the duration (or the class length setting when the
  duration is 0). Repeating lesson dragged from a known date → dialog `Перенести пару`: `«{subject}»
  повторюється щотижня.\n\nПеренести лише {d} {month} чи всю серію?` with `Лише цю дату` (default),
  `Всю серію`, `Скасувати`. Dropping back on the same date and time does nothing. "Only this date" →
  `move_occurrence`; "whole series" / non-repeating → `move_lesson` (template → new weekday; dated →
  new date). The month view moves to another day keeping the time.
- Subject colour: `md5(subject.strip().casefold() as UTF-8)`, first 8 hex digits as an integer,
  modulo 7 into the palette in §F.

### E.6 Month view (`ui/month_view.py`)

- One card, a header row `Пн`…`Нд`, 6×7 tiles from the Monday on or before the 1st. Tile min height
  92 px; date in a 28 px circle (today = ink pill; other months quiet), tooltip `{DayName}, {d}
  {month}`; tile tooltip `{n} пара|пари|пар`.
- Up to 3 chips in start-time order: subject bar, `HH:MM [mark ]{subject}` elided (marks `✓`
  opened/manual, `✕` skipped, `!` missed); tooltip `{subject}\n{start}–{end}[\n{teacher}]`. Chips
  that do not fit the tile's height are hidden and counted with the overflow as `ще {n}`.
- Click a tile → that day in the day view. A 28 px `+` appears on hover (tooltip `Додати пару на
  цей день`) → editor one-off on that date. Chips are draggable onto another tile.

### E.7 Catch-up banner (`ui/catchup_banner.py`)

- Amber card above the calendar, hidden when the queue is empty. Warning glyph. Title `Пара вже
  почалася: {subject}`; detail `Початок о {start}, до {end}` + `  ·  {teacher}` + `  ·  ще {n} у
  черзі`. Buttons `Підключитися зараз` (primary; disabled with tooltip `У цієї пари немає
  посилання` when there is no link, else tooltip = URL) and `Закрити` (marks the class `missed`).
- Several classes queue (no duplicates) and are shown one at a time. When the scheduler reports a
  lesson opened it is dropped from the queue. The window is brought forward on each offer.

### E.8 Update strip (`ui/update_banner.py`)

- Neutral card, download glyph. Available: `Доступна нова версія AutoPara {v}`, detail = first
  non-empty line of the release notes (leading `#*- ` stripped, max 140 chars) or `Зараз у вас
  {current}.`; buttons `Що нового` (when the page URL is trusted), `Пропустити версію`, `Пізніше`,
  primary `Завантажити` (installable and an asset exists) or `Відкрити сторінку` (page only) or
  none.
- Downloading: `Завантажую AutoPara {v}…`, `{p}%`, an 8 px progress bar, no buttons.
- Ready: `Оновлення {v} завантажено`; Windows `Встановлення закриє AutoPara і запустить нову
  версію.` + `Встановити й перезапустити` (+ `Пізніше`); macOS `Відкрийте образ і перетягніть
  AutoPara у «Програми», замінивши стару.` + `Відкрити образ`.
- A failed download returns to the offer. Manual check results: `Оновлень немає` / `У вас остання
  версія AutoPara — {version}.`; `Не вдалося оновити` / `Не вдалося зв'язатися з GitHub або
  завантажити файл. Перевірте з'єднання з інтернетом і спробуйте ще раз.`

### E.9 Import landing (`ui/import_landing.py`)

- Card with the 96 px app mark, title `Розклад ще не імпортовано` (24 pt, 800), text `Імпортуйте
  файл .docx із розкладом — і AutoPara відкриватиме пари сама, за кілька хвилин до початку.`, a
  drop well (460 px wide, 180 px high; `Перетягніть сюди файл .docx`; after a choice the file
  **name** and `Натисніть, щоб обрати інший`), and `Обрати файл…`.
- The whole landing accepts the drop; only local files ending in `.docx` (case-insensitive) are
  accepted, the first one wins; refusing happens on drag-enter. Clicking the well browses.

### E.10 Import dialog (`ui/setup_dialog.py`)

- Title `Імпорт розкладу` (window and heading); hint `Оберіть файл .docx із розкладом, а потім свій
  курс і групу. Документ може бути будь-якою мовою — записи створюються українською.`; compact drop
  well (124 px); `Курс` combo (`{course}  ·  {n} пар` / `немає пар`); `Група` list
  (`{group}   —   {specialty}   ({n} пар)`); summary `<b>{n}</b> пар · {k} спільних з іншою групою
  · {m} без посилання` or `У цієї групи немає пар у документі.`; `Скасувати` / `Імпортувати`
  (primary, disabled until a group is chosen).
- Opens on: a dropped path, else the archived copy, else the last import path, when they exist.
  Preselects the previous course (by ordinal) and group (by name), else the first group.
- File picker title `Оберіть розклад`, starting in `~/Desktop`, filter `Документи Word (*.docx)`.
- Errors: `Не вдалося прочитати файл` / `{path}\n\n{error}`; `Нічого не знайдено` / `У цьому
  документі немає таблиць розкладу.`; `Імпорт не вдався` / `Не вдалося зберегти курс.` / `Не
  вдалося зберегти групу.`
- Accept: `import_courses`, then store the chosen course and group ids.

### E.11 Course and group dialog (`ui/group_dialog.py`)

- `Курс і група`; hint `Оберіть іншу групу з уже імпортованого розкладу. Файл перечитувати не
  потрібно.`; `Курс` combo; `Група` list `{group}[  ·  {specialty}]  ·  {n} пара|пари|пар` or
  `немає пар`; double-click accepts; `Скасувати` / `Обрати`. Writes the two settings only.

### E.12 Class editor (`ui/edit_dialog.py`)

- Window title `Редагувати пару` / `Нова пара`; heading `Редагувати пару` / `Створити пару`.
- Fields: `Предмет` (placeholder `Назва предмета`), `Викладач` (`Викладач (необов'язково)`),
  `Повторення` (`Один раз`, `Щотижня до дати`, `Щотижня, без кінця`; hidden for imported lessons),
  `Дата`, `Повторювати до` (minimum = start date), `День` (weekday names), `Час` (`HH:mm` – `HH:mm`),
  `Посилання` (placeholder `https://… (Zoom або Google Meet)`). Date fields `dd.MM.yyyy`, Ukrainian
  calendar popup, Monday first, weekends muted (not red).
- Hints: repeat — `{DayName}, {d} {month} — лише цього дня.` / `Щотижня, у {day acc.}: з {d}
  {month} по {d} {month} — {n} раз|рази|разів.` (n = weeks + 1, min 1) / `Щотижня, у {day acc.},
  доки пару не видалено.`; length — `Тривалість: {h} год {m} хв` or `Кінець має бути пізніше за
  початок.`; link — `Без посилання пара не відкриється автоматично.` / `Відкриваються лише
  посилання http:// та https://.` / `Розпізнано: Zoom|Google Meet|Невідомий сервіс`.
- The end time follows the start (start + class length) whenever it would not be after it.
- New class: date = the date asked for (else today), start = the hour asked for (else 08:00), end =
  start + class length, series end = date + 28 days, kind = one-off.
- Validation: `Потрібна назва` / `Вкажіть назву предмета.`; `Хибне посилання` / `Посилання має
  починатися з http:// або https://.`; `Хибний час` / `Кінець пари має бути пізніше за її початок.`;
  `Хибна дата` / `Серія не може закінчитися раніше, ніж починається.`
- Teacher is stored wrapped in parentheses when the user did not type them. `pair = pair_slot(start)`.
  New lessons belong to the chosen group only.
- Buttons `Скасувати` (plain) / `Зберегти` (primary, default).

### E.13 Settings (`ui/settings_dialog.py`)

- `Налаштування`. Form: `Відкривати посилання` (lead), `Тривалість пари`, `Тема` (`Як у системі`,
  `Світла`, `Темна`).
- `Сповіщення`: `Нагадувати про пару перед початком` + `Час сповіщення`.
- `Якщо пару пропущено`: `Сповістити мене й відкрити після підтвердження`, `Відкрити одразу`,
  `Позначити як пропущену`; hint `Пара, яка вже тривала на момент запуску AutoPara, ніколи не
  відкривається сама -- застосунок спершу питає, навіть у режимі «Відкрити одразу».`
- Autostart: Windows `Запускати AutoPara разом із Windows` + `Автозапуск додає AutoPara до
  автозавантаження Windows і запускає його згорнутим у трей. Вимкнути можна також у Диспетчері
  завдань → Автозавантаження.`; macOS `Запускати AutoPara під час входу в систему` + `Автозапуск
  додає AutoPara до об'єктів входу і запускає його згорнутим у трей. Вимкнути можна також у
  Системних параметрах → Загальні → Об'єкти входу.` Failure: `Не вдалося змінити автозапуск` /
  `Windows відхилив зміну запису автозавантаження в реєстрі.`
- `Оновлення`: combo `Повідомляти про нову версію`, `Завантажувати автоматично`, `Не перевіряти`;
  `Версія {v}`; `Перевірити зараз` (saves the mode first); hint `Нові версії беруться з GitHub.
  Навіть із автоматичним завантаженням установлення починається лише після вашого підтвердження.`;
  `Тривалість пари впливає лише на пари, додані або імпортовані надалі.`
- `Скасувати` / `Зберегти`. Saving re-applies the theme and reloads.

### E.14 Tray (`ui/tray.py`)

- Tooltip `AutoPara — автозапуск пар`. Menu: `Показати розклад`, `Імпортувати розклад…`,
  `Налаштування…`, `Перевірити оновлення`, separator, `Вийти з AutoPara`. Single or double click
  shows the window. The glyph is the app mark (`ui/assets/logo.png`), not themed.

### E.15 Keyboard and pointer

| Input | Where | Effect |
|---|---|---|
| Ctrl+F (platform Find) | main window | focus and select the search field |
| Enter / Shift+Enter | search field | jump to the next / previous date with a match |
| Esc | search field with text | clear it |
| Menu key / right click | class card | actions menu |
| Enter | dialogs | the default (primary) button |
| Esc | dialogs | cancel |
| double click | group dialog list | choose |
| click / double click | tray icon | show the window |
| drag | card, month chip | move (E.5) |

### E.16 Ukrainian forms

- Months: nominative `Січень … Грудень`, genitive `січня … грудня` (`normalize.MONTH_NAMES`,
  `MONTH_GENITIVE`).
- Plurals: `groups_word` (2–4 → групи, else груп; 12–14 → груп); `pairs_word` (1 → пара, 2–4 →
  пари, else пар; 11–14 → пар); `times_word` (раз / рази / разів, same rule).
- Course label `{Roman} курс`.

---

## F. Design tokens

`autopara/core/theme.py` (`PALETTES`), `autopara/ui/styles.qss` (per-widget rules), constants in the
widget modules.

### F.1 Colours

| Token | Light | Dark |
|---|---|---|
| canvas | #f5f5f6 | #0a0a0b |
| bg | #ffffff | #151517 |
| surface | #ffffff | #151517 |
| sunken | #f5f5f6 | #1d1d20 |
| text | #0e0e10 | #f2f2f4 |
| text_muted | #6b6b75 | #a0a0aa |
| text_faint | #7e7e88 | #8a8a94 |
| border | #e6e6ea | #2a2a2e |
| border_soft | #ececef | #232326 |
| grid_line | #e2e2e7 | #2a2a2e |
| accent | #0e0e10 | #f2f2f4 |
| accent_fill | #0e0e10 | #f2f2f4 |
| accent_hover | #2b2b30 | #d6d6da |
| accent_ink | #0e0e10 | #f2f2f4 |
| accent_soft | #efeff1 | #26262b |
| accent_text | #ffffff | #0e0e10 |
| today_bg | #fafafb | #19191c |
| now_bg | #fafafb | #19191c |
| card_bg | #ffffff | #1d1d20 |
| opened_bg | #fafafb | #18181a |
| missed_bg | #fffafa | #241a1a |
| nolink_bg | #fffdf4 | #22200f |
| skipped_bg | #fafafb | #18181a |
| chip_bg | #f0f0f2 | #26262b |
| success_bg | #dff4e5 | #17301f |
| success_fg | #1f7a47 | #7edc9e |
| danger | #e5534d | #f0665c |
| danger_bg | #fdecea | #33191a |
| danger_fg | #c23a30 | #ff8a84 |
| warn_bg | #fff3c9 | #33290c |
| warn_fg | #7d6200 | #f1d86b |
| zoom_bg | #e1f0fd | #142b40 |
| zoom_fg | #1f67ad | #8cc8f5 |
| meet_bg | #dff4e5 | #17301f |
| meet_fg | #1f7a47 | #7edc9e |
| tooltip_bg | #0e0e10 | #f2f2f4 |
| tooltip_fg | #ffffff | #0e0e10 |
| scroll | #dcdce0 | #33333a |
| scroll_hover | #c4c4ca | #4a4a52 |
| drop_bg | #f0f0f2 | #1d1d20 |
| drop_border | #cfcfd6 | #3a3a40 |
| drop_hover_bg | #efeff1 | #26262b |
| hero_bg | #a8bdd6 | #26262b |
| banner_bg | #fff3c9 | #33290c |
| banner_border | #f0dc8a | #5c4a12 |
| banner_text | #5c4700 | #f1d86b |
| rail_bg | #0e0e10 | #1d1d20 |
| rail_icon | #ffffff | #f2f2f4 |
| rail_hover | #2b2b30 | #2e2e33 |
| rail_primary_bg | #ffffff | #f2f2f4 |
| rail_primary_fg | #0e0e10 | #0e0e10 |

- Subject palette, identical in both themes (`class_card.SUBJECT_COLORS`): coral `#ec6b66`, lemon
  `#f1f36a`, sky `#86cdf7`, violet `#8566e6`, mint `#8fe8a8`, pink `#f26c9c`, dusty blue `#a8bdd6`.
- Card tint (`tinted`): per channel `round(base + (subject − base) · t)`, `t = 0.16` light /
  `0.20` dark; base = `card_bg` for cards, `sunken` for month chips. Only normal/next cards and
  non-settled month chips are tinted. Missed uses `danger` for the bar.
- `theme = system` reads `HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\
  AppsUseLightTheme` (missing → light) on Windows; `defaults read -g AppleInterfaceStyle` == `Dark`
  on macOS. Read at start and when settings are saved, not live.

### F.2 Typography

- Inter (Regular, Medium, SemiBold, Bold, ExtraBold) bundled in `autopara/ui/fonts` with `OFL.txt`.
  Family order: Windows `Segoe UI`, `Segoe UI Variable Text`, `Inter`, `Arial`; elsewhere `Inter`,
  `Segoe UI Variable Text`, `Segoe UI`, `-apple-system`, `Helvetica Neue`, `Arial`. Text is drawn
  anti-aliased and unhinted.
- Sizes (pt; ×1.15 on macOS, `MAC_POINT_SCALE`): base 10; dialog title 16/700; nav title 21/700
  with the year 21/400 faint; hero title 24/800; next-up pill 9/600; day name 9/600 (muted, today
  text); day date 11/700; hour label 8.5/500 faint; card subject 9.5/700; teacher and time 8/500
  muted; chips 8/700 (group chip 8/600); month weekday 9/700; month date 10/700 (other month 500
  faint); month chip text 8/600; "ще N" 8/600; banners title 10/700, detail 9; form hint 9 muted;
  section label 9/700 muted (8 px top padding); empty state 10 muted; drop well text 10 (hover 600),
  file 10/700; filter summary 9/600; clock 8/600; now badge 8/700; buttons 600 (primary 700,
  segments 700); menu items 500; tooltip 9.

### F.3 Shape and spacing

- Spacing unit 8 px, tight 4 px. Radii family: fully round for buttons, rail, segmented control,
  date circles; 14 for big cards (grid, month, landing, banners, drop well) and the rail pill 26;
  12 for menus; 10 for plates (cards, inputs, lists, drop ring, tooltips); 8 for list items and menu
  items; 6 for chips; 4 for the progress bar; 2 for the card bar.
- Exact heights (radius = half): buttons 34 + 1 px border (radius 18); rail icon buttons 36; add
  circle 40; nav arrows 34+border; segmented 40 with 32 px segments; month plus 28; clock 60×20
  (radius 10); next-up 28; progress 8. Inputs 36 px (`metrics.INPUT_HEIGHT`), padding 6×12, radius
  10; filter inputs round (radius 18).
- No shadows anywhere; cards are separated by tint and a 1 px hairline. No backdrop blur.
- Geometry: rail 80 px, pill 52 px (margins 8 top/bottom, spacing 2, 6 px under the add button),
  rail column margins 24/24, spacing 12. Grid: hour 66, gutter 74, column min 150, header 72, date
  circle 32, min card 26 min, card padding left 17 / right 10 / vertical 4, spacing 2, chip spacing
  5. Month: tile min 92, date circle 28, tile padding 8×6, max 3 chips, card padding 12. Landing
  column 460, well 180 (compact 124). Dialog min widths: editor 440, settings 470, import 560,
  group 460; dialog padding 24×20. Menu padding 6, items 8 26 8 14. Tooltip padding 10×7, max width
  360, offset (14, 20), flips above the pointer near the screen bottom, hides after 12 s or on
  leave/press/wheel/key/deactivate.

### F.4 Motion (`ui/transitions.py`, `ui/nav_bar.py`)

| What | Duration | Easing |
|---|---|---|
| Period/view morph (arrows, Сьогодні, month day click, search jump, segmented control) | 320 ms | in-out cubic |
| New calendar drift on a period change (none on a view change) | 14 px | with the morph |
| One-sided class grow/shrink | from/to 90 % | with the morph |
| Segmented thumb glide | 220 ms | out cubic |
| Popup (menu, tooltip) grow-in | 200 ms (140 + 60), from 90 % | out cubic |
| Dialog fade-in | 140 ms | out cubic |
| Round "+" hover fill | 220 ms | in-out quad |

- Morph: snapshot the calendar (empty layer + one picture per class with its box), apply the
  change, snapshot again; a class present on both sides grows from the old box to the new one
  (container transform, pictures cross-dissolve, not scaled); within one view the same class is the
  same lesson in the same place (next week's same weekday), across views the same lesson on the same
  date. The overlay passes clicks through; a newer move finishes the old one; motion is skipped when
  the window is hidden. The Python app has no reduced-motion setting.

---

## G. Update flow

`autopara/core/updater.py` (no Qt) and `autopara/core/update_service.py`.

- Schedule: on start `tidy(updates/)` (delete `*.part` and files whose name carries a version not
  newer than the running one); if `update_mode != off`, first check 15 s after start, then every
  6 h. Manual checks (settings, tray `Перевірити оновлення`) always run. One check at a time.
- Request: `GET https://api.github.com/repos/<GITHUB_REPO>/releases/latest`, headers `Accept:
  application/vnd.github+json`, `User-Agent: AutoPara/<version>`, 10 s timeout, body ≤ 2 MB;
  HTTP 404 → no release. `GITHUB_REPO = "sevcenkoa864-oss/AutoPara"` (see §I).
- Trust: every URL must be `https` on `github.com` / `githubusercontent.com` or a subdomain; every
  redirect is checked; system trust store with `certifi` only when the store is empty.
- `parse_release`: drafts and pre-releases ignored; tag must match `v?X.Y.Z`; page URL kept only
  when it starts with `https://github.com/<repo>/`.
- `pick_asset`: Windows `^AutoPara-.+-Setup\.exe$`; macOS `^AutoPara.*\.dmg$` and the Intel word
  (`Intel`, case-insensitive) must match the machine (`x86_64`/`amd64`/`i386` = Intel); plain name
  (no path); URL under `https://github.com/<repo>/releases/download/`; SHA-256 from the asset's
  `digest` (`sha256:<hex>`) when present.
- `mac_asset_name(version)`: `AutoPara-Intel-<v>.dmg` on Intel, `AutoPara-<v>.dmg` otherwise.
- Versions: three integers compared numerically; unparsable is never newer.
- Decision (`_checked`): store `last_update_check`; not newer → `up_to_date` (dialog only when
  manual); equals `skipped_update_version` and automatic → silent; `auto` + installable + asset →
  download; else offer.
- Installable = frozen build on Windows or macOS. From source → offer with the release page only.
- Download: into `updates/<name>.part`, 30 s timeout, 64 KiB chunks, progress per whole percent;
  size and SHA-256 verified, then renamed; an already-verified file is reused; any failure deletes
  the part file.
- Apply (only on the user's click; only a file inside `updates/`): Windows runs
  `cmd /S /C "ping -n 3 127.0.0.1 >nul & "<installer>" /S & start "" "<app exe>""` detached with no
  window and the app quits; macOS `open <dmg>` and the app keeps running.
- Failures of automatic checks are silent; manual ones and failed downloads show the error dialog.

---

## H. Installer, uninstaller and release

`installer/AutoPara.nsi`, `.github/workflows/installer.yml`, `docs/RELEASING.md`,
`tools/bump_version.py`, `build.ps1`, `build_mac.py`.

### H.1 Windows (NSIS)

- UTF-8 with BOM, `Unicode true`, MUI2, language Ukrainian only. `Name` = `AutoPara — автозапуск
  пар`. `OutFile ..\dist\AutoPara-${APP_VERSION}-Setup.exe`. LZMA solid.
- Per-user: `InstallDir $LOCALAPPDATA\Programs\AutoPara`, `InstallDirRegKey HKCU
  Software\AutoPara InstallDir`, `RequestExecutionLevel user`; never writes HKLM.
- Pages: welcome, components, directory, install, finish (run checkbox `Запустити AutoPara`, text
  `AutoPara встановлено.\r\n\r\nПри першому запуску перетягніть у вікно файл розкладу (.docx), а
  потім оберіть свій курс і групу.`); uninstall: confirm, progress.
- Icons: `build\AutoPara.ico` when present (rendered from `ui/assets/logo.png` at 16–256 px).
- Sections: `AutoPara (обов'язково)` (read-only): remove `$INSTDIR\_internal`, copy the bundle,
  write `InstallDir`, the uninstaller, the Add/Remove entry under HKCU (`DisplayName`,
  `DisplayVersion`, `Publisher` = AutoPara, `DisplayIcon`, `UninstallString`, `InstallLocation`,
  `NoModify` 1, `NoRepair` 1, `EstimatedSize` KB), Start Menu folder `AutoPara` with `AutoPara.lnk`
  and `Видалити AutoPara.lnk`. `Ярлик на робочому столі` (optional). `Запускати разом із Windows`
  (optional) writes `HKCU\…\Run\AutoPara = "$INSTDIR\AutoPara.exe" --hidden` (identical to the
  app's own command). Descriptions: `Програма AutoPara та все, що їй потрібно для роботи.`,
  `Створити ярлик на робочому столі.`, `Запускати AutoPara у трей разом із Windows, щоб пари
  відкривалися навіть тоді, коли ви забули увімкнути програму. Це можна змінити в налаштуваннях.`
- Version info: `VIProductVersion ${APP_VERSION}.0`, product/file names = display name, company and
  copyright = AutoPara.
- `.onInit`: unless silent, if a window titled `AutoPara — автозапуск пар` exists, `AutoPara зараз
  працює.\r\n\r\nЗакрийте її через значок у треї та натисніть OK.` OK continues, Cancel aborts.
- Uninstall: delete the exe, uninstaller, `_internal` and `$INSTDIR`; Start Menu entries and folder;
  desktop shortcut; the Run value, the Uninstall key and `Software\AutoPara`. Then, unless silent,
  ask `Видалити також збережений розклад і налаштування?\r\n\r\n$APPDATA\AutoPara` — Yes deletes
  `%APPDATA%\AutoPara`; No or `/S` keeps it.

### H.2 macOS

- `build_mac.py` builds `AutoPara.app` with PyInstaller and a `.dmg` named by
  `updater.mac_asset_name` (Apple Silicon `AutoPara-<v>.dmg`, Intel `AutoPara-Intel-<v>.dmg`).
  Unsigned and not notarised (first launch through the context menu `Відкрити`).

### H.3 Release workflow

- On push to `main` (ignoring `android/**`) and manual dispatch; concurrency group
  `installer-release`. Windows job: read `APP_VERSION` with `^!define\s+APP_VERSION\s+"([^"]+)"`,
  run tests, install NSIS if missing, `build.cmd`, upload the artifact; if `v<version>` has no
  release, create it with title `AutoPara <version>` and Ukrainian notes for Windows, both Macs and
  Android. `build-mac` (`macos-14`) and `build-mac-intel` (`macos-15-intel`, after `build-mac` so
  the Apple Silicon image is listed first) attach their image only when the release lacks it.
  `build-android` builds and attaches `AutoPara-<version>.apk` the same way.
- The version lives in `installer/AutoPara.nsi` and `autopara/__init__.py` (a test keeps them equal;
  `tools/bump_version.py` writes both); Android reads `__version__`; `versionCode = major·10000 +
  minor·100 + patch`.

---

## I. Known gaps, traps, and code/doc disagreements

From `docs/HANDOFF.md`:

1. The updater has never been exercised end to end; the Windows apply chain has never run on
   Windows hardware.
2. macOS images are unsigned and not notarised; the update path only opens the `.dmg`. The Intel
   image has not run on Intel hardware; the `macos-15-intel` runner label may be retired.
3. The Windows installer's strings and the redesigned look have not been seen on a Windows machine.
4. Rounded popups (`ui/popups.py`) were only seen on a real Mac.
5. Qt traps (corner squares when radius > half height, scroll area sizing, column stretch memory,
   hidden widgets only scheduling layouts, macOS point size) are Qt-specific; the web equivalents
   to watch are fixed card heights, equal columns and whole-line clipping (E.4, E.5).
6. Publishing a release ships an update to every older install.

Found while reading the code for this spec:

7. **Updater points at the old repository name.** `updater.GITHUB_REPO = "sevcenkoa864-oss/AutoPara"`,
   but the repository is now `reyjendiz/AutoPara` (the old name redirects). The API answer carries
   `browser_download_url` and `html_url` under `reyjendiz`, so `pick_asset`'s prefix check rejects
   every asset and the page URL is dropped: the Python app can find a newer version but never
   offer a download or a page. The Rust updater must use the current repository.
8. **The class-length setting is not used by import.** `SetupDialog._load` calls `parse_file(path)`
   with the default 80 minutes, although Settings says the length applies to future imports.
9. **Docs disagree on uninstall.** `docs/ARCHITECTURE.md` ("Reinstalling replaces; uninstalling
   removes everything") says uninstall deletes `%APPDATA%\AutoPara` unconditionally and mentions
   an `Installation.purge_previous` that no longer exists; the `.nsi`, README and CONTRIBUTING say
   it asks and keeps the data on No or `/S`. The `.nsi` is what ships.
10. `open_now(claim=True)` leaves a `manual` row when the browser refuses (the claim is made before
    the open and not changed on failure).
11. Re-import never deletes courses or groups that disappeared from the document (B.2).
12. Waking from sleep with `catchup_mode = open` opens a class that is already running (C.4). The
    plan (step 3.4) requires that nothing opens after start **or wake** until the user picks.
13. `launcher.is_openable` accepts `http://`; the plan (step 3.5) asks for `https` only.
14. No time-zone or DST handling; times are naive local (C.1). The plan asks for DST tests in
    `Europe/Kyiv`.
15. Single-instance and fire-once rely on one process per user; two processes racing are stopped
    only by the `UNIQUE` constraint.
16. `tests/conftest.py` falls back to **any** `~/Desktop/*.docx` as "the real timetable". Running
    the suite on a machine with other `.docx` files on the Desktop can point it at an unrelated
    document; the baseline must run with an isolated home directory.
17. `looks_like_schedule` / `is_course_heading` are dead code.
18. `theme` and `catchup_mode` are not validated when read; an unknown `catchup_mode` behaves as
    notify, an unknown theme as system.
19. Window size and position are not persisted (the plan adds this in 4.3).

---

## J. Baseline (step 0.4)

Python suite at `8e73aff`, run on the development machine (Windows 11, Python 3.12.10, PySide6
6.12.0, pytest 9.1.1) with `QT_QPA_PLATFORM=offscreen` and `USERPROFILE`, `HOME`, `APPDATA`,
`LOCALAPPDATA` pointed at an empty temporary profile, so no real timetable was found (§I.16) and the
user's profile was not touched.

| Result | Count |
|---|---|
| passed | 674 |
| failed | 1 |
| skipped | 20 (the real-timetable variants of the `document_*` checks) |
| runtime | 145 s |

The one failure is
`tests/test_installer.py::TestUpdateAssetNaming::test_the_repository_the_updater_asks_is_the_one_this_is_cloned_from`:
`updater.GITHUB_REPO` (`sevcenkoa864-oss/AutoPara`) is not the repository the checkout is cloned
from (`reyjendiz/AutoPara`). The suite itself catches §I.7; the bug is in the shipped 1.10.0.

---

## K. Visual baseline (step 0.6)

Reference pictures (last refreshed in `1b2d079`, rendered by `python tools/screenshots.py`):

| File | Size | Theme | What |
|---|---|---|---|
| `docs/week-grid.png` | 1240×800 | light | week view |
| `docs/week-grid-dark.png` | 1240×800 | dark | week view |
| `docs/month-view.png` | 1240×800 | light | month view |
| `docs/day-view.png` | 1240×800 | light | day view |
| `docs/edit-dialog.png` | 440×444 | light | class editor for today's first class |
| `docs/settings.png` | 470×915 | light | settings dialog |

Setup in `tools/screenshots.py`, to be reproduced by `cargo xtask screenshots` (5.11):

- Timetable: `tests/sample_timetable.py` (`rust/fixtures/golden/sample_timetable.docx`), imported
  into a fresh database; course = the first (`I курс`), group = its first (`А2-Бд26-11 група`).
  The scheduler is constructed but never started, so no class carries a verdict.
- Clock pinned to **Monday 2026-03-02 09:40:20** (the middle of the 09:30 class): today's date
  circle, the "now" line at 09:40, the pill `Зараз · Загальна педагогіка · ще 70 хв`, and the rail
  clock `09:40:20` all show. The anchor is the pinned date.
- Window 1240×800; animations off; the theme applied explicitly per picture (light, dark, light,
  light); `view_mode` set per picture.
- Editor: `EditDialog` for the first lesson that occurs on the pinned date (Історія України,
  08:00), with `day` = the pinned date. Settings: `SettingsDialog` with default settings; the
  autostart box shows whatever the rendering machine's registry says (unchecked in the reference).
- Dialogs are grabbed after three event-loop passes and a resize to their size hint.
- The reference images were rendered offscreen in **Inter** with the Windows autostart wording, i.e.
  on a platform that is neither Windows (which would use Segoe UI) nor macOS. Comparisons on Windows
  must expect Segoe UI there, or pin Inter for the screenshot run.
