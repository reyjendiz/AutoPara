# BACKEND — data model, import pipeline, scheduler, storage

> Read `ARCHITECTURE.md` first. Update this file whenever the data model, the parsing rules, or the
> scheduler behavior changes. The code is `rust/crates/autopara-core/src/`: `docx.rs` (reading
> the package), `text.rs`, `days.rs` and `time.rs` (normalisation), `parser.rs` (the rules below),
> `db.rs`, `import.rs`, `legacy.rs`, `scheduler.rs` and `engine.rs`.

## 1. The source document

The reference file is `Робочий_розклад (2).docx` (a real university schedule, Ukrainian).
Its structure, **verified by parsing the actual file** rather than assumed:

- Body order repeats 6 times: heading paragraph (`І КУРС` … `VІ КУРС`), a small `он-лайн`
  paragraph, then one `<w:tbl>`. Tables are matched to courses by document order.
- Row 0 of each table: `День | Пара | Час | <group> | <group> | …`
- Row 1: `День | Пара | Час | <specialty> | <specialty> | …`
- Rows 2+: one lesson slot each.

### Verified facts (regression fixtures)

| Metric | Value |
|---|---|
| Courses / tables | 6 |
| Total lesson events | **61** |
| Rows in the tables, before R5/R12 blocks are joined | 77 |
| Events spanning more than one slot | 15 |
| Events with a link | 53 (revision-dependent, **not** asserted) |
| Events without a link | 8 (revision-dependent, **not** asserted) |
| Per course (I..VI) | 24, 10, 8, 10, **0**, 9 |
| Distinct hyperlink relationships | 63 elements, 21 unique URLs |
| Providers | 44 Zoom, 9 Google Meet |

> **Link counts are not regression fixtures.** The university fills missing links in over time --
> between two revisions of this document the linked total moved 60 -> 63 while the structure was
> byte-identical. Hard-coding the count produces a test that fails on a *document* change rather
> than a *code* change, so the link tests compare against the document itself instead:
>
> * `test_link_count_matches_the_document` -- `sum(lesson.linked_cells)` == the number of
>   `<w:hyperlink>` elements. Catches a link dropped from one lesson (which a URL-set comparison
>   cannot see, because ~20 URLs are shared across ~60 lessons). It counts *cells* rather than
>   lessons because a double class carries a link in each of its slots and R12 merges those slots
>   into one lesson; `ParsedLesson.linked_cells` records how many linked cells folded in, so the
>   equality stays exact instead of being relaxed to an inequality.
> * `test_parser_finds_exactly_the_links_the_document_contains` -- the set of parsed URLs equals
>   the set of relationship targets. Catches mangled or invented URLs.
>
> Both oracles are in the Python suite (`tests/`) and read the `.docx` zip directly. The Rust parser
> is held to the Python parser's output instead: `rust/crates/autopara-core/tests/golden.rs`
> compares every field with the JSON exported from 1.10.0 (`rust/fixtures/golden/`). The structural
> numbers above (61 lessons, per-course counts) *are* asserted: they describe merge handling and
> hold across every revision seen so far.

Course V (`51 група`) legitimately has **no classes at all** — an empty schedule is a valid state,
not an import failure.

## 2. Parsing rules

These rules exist because the document's real XML differs from what a naive reading suggests. Each
was confirmed against the file; do not "simplify" them away.

**R1 — Logical columns, not cell indices.**
`<w:tc>` count varies per row (4 vs 5 vs 7) because merged cells are a *single* `tc` carrying
`<w:gridSpan w:val="N"/>`. The column identity of a cell is the **running sum of gridSpan** of the
cells before it, never its index in the row.

**R2 — Group ownership is a column *range*.**
Header cells themselves can span: in Table 4 (`ІV КУРС`) the grid has 7 columns but only 2 groups,
because `42 група` has `gridSpan=3`. Each group therefore owns `(col_lo, col_hi)`. A lesson cell
occupying `[c, c+span-1]` belongs to **every group whose range overlaps it**. An overlap with more
than one group means one shared class session with multiple groups — a single event, a single
calendar card, and a single browser open.

The original spec proposed detecting shared classes by finding identical text repeated in adjacent
columns. That is the wrong primary strategy for this document: Word stores the shared class as one
merged cell, so there is no repeated text to find. Duplicate-text detection remains only as a
defensive fallback (`dedupe_duplicates`) for documents produced by other tooling.

This is a statement about **columns only**. Down the *rows* the document really does repeat text --
see R12 -- and comparing it there is the primary strategy, not a fallback.

**R3 — Links live in two places.**
A URL appears both as a `<w:hyperlink r:id="rIdN">` (resolved through
`word/_rels/document.xml.rels`) **and** as visible run text. Collect both, then dedupe
order-preservingly, or every link is captured twice.

**R4 — The day column is mostly empty.**
`День` uses `<w:vMerge w:val="restart"/>` on the first row of a day and `<w:vMerge/>` (continue) on
the rest, with **no text** in the continuation cells. The parser carries the last seen day forward.

**R5 — Lesson cells use vertical merges too.**
25 cells carry `vMerge`. A `restart` cell **with content** is one lesson block spanning the
following `continue` rows (e.g. `Дисципліни вільного вибору` covers 4–6 consecutive pairs); its end
time extends to the end of the last continued row. A `restart` cell with **no** content is just
merged empty space and is ignored.

**R12 — A class repeated in the next slot is still one class.**
The author writes a double class two ways: as the vMerge block of R5, or by simply retyping the cell
in the next row. Both mean the same thing — the class does not stop and nobody leaves it — so
`merge_consecutive_slots` folds the second slot into the first, which keeps the first slot's `pair`
and `start_time` and takes the last slot's `end_time`. Without it the scheduler opens a second
browser tab an hour and a half into a meeting the user is already sitting in, and the grid draws two
cards with a gap between them. 12 blocks in the reference document merge this way, 77 rows → 61
lessons.

Two rows merge when the day, the group set and the subject match, and the teacher and URL are
*compatible* — equal, or blank on one side, because the link is often written only on the first row.
The block then inherits whichever value was filled.

Adjacency is the load-bearing part, and it is a **slot** rule: the next row's pair must be exactly
one past the last pair the block already covers, or it must start exactly when the block ends. It is
never "this subject appears twice today" — course IV's Friday runs *Формування мистецько-творчої
компетентності* at pairs 2, 3 and 5, and that is a merged 09:30–12:40 block plus a genuinely
separate afternoon session. Measuring from the *last* pair covered (`normalize.last_pair_covered`)
rather than from the block's own `pair` is what lets a run of three chain, and what lets an R5 vMerge
block absorb a retyped row after it.

**R6 — Pair inference.**
`Пара` is blank on one row. The `Час` -> `Пара` mapping is perfectly consistent and is the source of
truth when the pair is missing:

`8.00 -> 1, 9.30 -> 2, 11.20 -> 3, 13.00 -> 4, 14.40 -> 5, 16.10 -> 6`

Default class duration is **80 minutes** (derived from the grid: 8.00–9.20, 9.30–10.50, …),
configurable via settings.

`TIME_TO_PAIR` also carries slots **7–10** (17.40, 19.10, 20.40, 22.10), continuing the same
80 + 10 minute rhythm. The document never uses them; `ACADEMIC_PAIRS = 6` is the count a *parsed*
lesson is expected to fall inside, and the regression fixtures still assert `1 <= pair <= 6`.

Since the grid became time-proportional (`FRONTEND.md`, "One minute, one pixel") `pair` no longer
decides where anything is drawn. It survives because it is part of `source_key`, which is what
lets a re-imported class keep its row id: two evening classes on the same day need different pair
numbers or they collide. A lesson's `pair` is therefore always the slot it *starts* in; an R5 or R12
block covers several, and `last_pair_covered` is what reads the far end.

**R7 — Classes without links.**
17 cells have a subject and teacher but no URL (e.g. `(див. розклад на сайті ХНПУ)`). They are
stored with `needs_link = 1`, are never auto-opened, and render with a "no link" badge. The user can
supply a URL later through Edit mode.

**R8 — Unicode normalization.**
`П`ятниця` in this document uses a **backtick (U+0060)**, not an apostrophe. Course headings mix
Cyrillic `І` (U+0406) with Latin `V`. The filename on disk is **NFD**-normalized
(`и` + combining breve U+0306). Therefore: normalize to NFC and fold `` ` ``, `’`, `ʼ` -> `'` before
matching day names; match courses by **ordinal position**, never by literal heading string.

**R9 — Cell text layout.** Within a lesson cell, lines (split on `<w:br/>` and paragraph
boundaries) are: subject, then `(Teacher Name)` in parentheses, then the URL. The teacher is the
first parenthesized line; the subject is the first non-empty line; anything URL-shaped is stripped
from the display text.

**R10 — Any language in, Ukrainian out.**
The document is not required to be Ukrainian. `normalize._DAY_ALIASES` recognises day names in
Ukrainian, Russian, English, Polish, German, Spanish, French, Italian and Romanian, plus the usual
abbreviations, matched longest-first after NFC + apostrophe folding so a cell like
`Понеділок 02.09` still resolves. Ambiguous two-letter abbreviations shared across languages
(German `so` = Sonntag vs Polish `sob` = sobota) are deliberately absent rather than guessed at.

Everything the parser *emits* is Ukrainian regardless of the input: `DAY_NAMES` is the Ukrainian
list, and a course is always stored as `course_label(ordinal)` — `I курс`, `II курс`, … The
heading text is not used for the name at all, which also disposes of the Cyrillic/Latin numeral
mixing in R8. Courses are matched to tables by document order.

Subject and teacher text is copied verbatim. Translating it would be inventing content: the class
name in the document is the class name.

**R11 — The `Час` cell may carry a range.**
The reference document writes a bare start (`8.00`); timetables from other tooling write
`8.00-9.20`. `parse_time_range` returns both, and an explicit end time beats the assumed class
duration. A start time outside the six standard slots no longer drops the row: `pair_slot` puts it
in the grid row that had already begun (the grid has six rows; a lesson is not obliged to fit one).

## 3. Data model (SQLite, `%APPDATA%\AutoPara\autopara2.db`)

`rust/crates/autopara-core/src/db.rs`. The schema is the Python app's as of 1.10.0, so the 1.x
import (`legacy.rs`) can copy rows with their ids unchanged. Migrations are a list (`MIGRATIONS`);
`PRAGMA user_version` records how many have run, and a new column or table is a new entry at the
end, never an edit of an old one.

```sql
settings(key TEXT PRIMARY KEY, value TEXT)

courses(id INTEGER PK, ordinal INTEGER UNIQUE, name TEXT)

groups(id INTEGER PK, course_id -> courses.id, name TEXT,
       specialty TEXT, col_lo INTEGER, col_hi INTEGER, UNIQUE(course_id, name))

lessons(id INTEGER PK, course_id -> courses.id,
        day_index INTEGER,        -- 0=Mon … 6=Sun
        pair INTEGER,             -- 1..6, the slot the lesson *starts* in (R5/R12 span several)
        start_time TEXT,          -- 'HH:MM'
        end_time TEXT,            -- 'HH:MM'
        subject TEXT, teacher TEXT,
        url TEXT, provider TEXT,  -- zoom | google_meet | unknown
        needs_link INTEGER, is_manual INTEGER,
        source_key TEXT,          -- stable identity for re-import matching
        on_date TEXT NULL,        -- 'YYYY-MM-DD'; NULL = weekly template
        repeat_until TEXT NULL)   -- 'YYYY-MM-DD'; with on_date = weekly series

lesson_groups(lesson_id -> lessons.id, group_id -> groups.id)   -- shared classes

lesson_skips(lesson_id -> lessons.id, skip_date TEXT)           -- a date moved out of a series

occurrences(id INTEGER PK, lesson_id -> lessons.id,
            occur_date TEXT,      -- 'YYYY-MM-DD'
            status TEXT,          -- opened | missed | manual | skipped
            fired_at TEXT,
            UNIQUE(lesson_id, occur_date))

assist_sessions, assist_segments, assist_notes                  -- Lecture Assist text, docs/ASSIST.md
```

`autopara-core` never reads the clock: every function that stamps a time takes `now` from the
caller (`clock.rs`), which is what lets the scheduler tests run on a fake clock.

### Lessons that belong to a date

| `on_date` | `repeat_until` | Meaning |
|---|---|---|
| NULL | — | Weekly template. Every imported class, and every manual class made before dates existed. |
| date | NULL | One-off: that date only. |
| date | date | Weekly series: that weekday, from `on_date` to `repeat_until` inclusive, minus its `lesson_skips`. |

`Lesson::occurs_on(day)` (`model.rs`) is the one place that decides whether a lesson happens on a
date; the scheduler and the calendar views both go through it, so they cannot disagree about what a
day contains. `day_index` always equals the weekday of `on_date` for a dated lesson. Dated lessons
are always manual (`is_manual = 1`, `source_key = ''`), which is what lets a re-import leave them
alone.

`move_lesson` moves a lesson (a series carries `repeat_until` along by the same number of days);
`move_occurrence` moves one date of a series: a `lesson_skips` row plus a one-off copy.

`settings` keys (`DEFAULTS` in `db.rs`, read through `Storage::settings`):

| Key | Default | Meaning |
|---|---|---|
| `selected_course_id`, `selected_group_id` | — | The chosen timetable. Also what the import screen reads back to preselect the same course and group when a new document is imported. |
| `lead_minutes` | `1` | How early a class opens. |
| `class_duration_minutes` | `80` | Assumed length when the document gives no end time (R11). The import uses it; 1.x did not (PORT_SPEC §I.8). |
| `autostart_enabled` | `1` | See ARCHITECTURE.md "Autostart is on out of the box". |
| `catchup_mode` | `notify` | `notify` \| `open` \| `missed` — what to do about a class whose start has passed. |
| `notifications_enabled` | `1` | Reminders and open/catch-up notices. |
| `notify_minutes` | `10` | How long before a class the reminder appears. |
| `update_mode` | `ask` | `ask` \| `auto` \| `off` — see section 7. Anything else reads as `ask`. |
| `skipped_update_version` | `""` | A release the user answered "Пропустити версію" to; automatic checks stay quiet about exactly it. |
| `last_update_check` | `""` | ISO time of the last answer from GitHub. |
| `view_mode` | `week` | `day` \| `week` \| `month`. The date being looked at is *not* stored: the app opens on today. |
| `theme` | `system` | `system` \| `light` \| `dark`. |
| `schedule_active_from` | `""` | ISO datetime stamped on every import. See "The schedule starts when it is imported". |
| `schedule_copy_path` | `""` | AutoPara's own copy of the imported `.docx`. See "The imported document is copied". |
| `last_import_path` | `""` | Where the document came from. Kept for reference; the copy is what the import screen reopens. |
| `legacy_imported_at` | `""` | Set once the 1.x data has been imported (or there was none), so that import never runs twice. |
| `assist_*` | — | Lecture Assist: consent, devices, models, language, retention. See docs/ASSIST.md. |

### Re-import behavior

Re-importing replaces imported lessons for the affected course but **preserves**:
- lessons the user created manually (`is_manual = 1`),
- user settings.

It **discards the `occurrences` log entirely** (`clear_occurrences`). A new document is a new
week: carrying yesterday's verdicts onto today's classes would show a freshly imported grid
already marked opened and missed, against lessons that may not even be the same ones.

Courses or groups that disappeared from the document are not deleted, as in 1.x (PORT_SPEC §I.11).

### The imported document is copied

`import::keep_copy` copies the `.docx` into `%APPDATA%\AutoPara\timetables\` and records
`schedule_copy_path`. The file the user picks is usually a download or an email attachment that
gets tidied away, and losing it should not cost them the ability to re-import.

Exactly one copy is kept — a new import replaces the old document as completely as it replaces the
old records. Because the copy lives with the database, the uninstaller's "delete my data too"
answer removes it, and a reinstall keeps it.

`source_key` (course ordinal + day + pair + subject) lets a re-imported lesson keep its identity, and
therefore its occurrence history, when the document is updated.

### Data from 1.x

On the first start with an empty `autopara2.db`, `legacy::import_legacy` copies the Python app's
`autopara.db` (opened read-only) row for row, ids unchanged, with every setting except
`schedule_copy_path`, and copies the archived document from `schedules/` into `timetables/`.
Nothing of the Python app's is changed, so reinstalling 1.10.0 still finds its data. See
`docs/UPGRADING.md`.

## 4. Scheduler

`scheduler.rs` holds the decisions as pure functions; `engine.rs` runs them on a task. After each
tick the engine sleeps until the next reminder, trigger, start or end still ahead today (or
midnight), **never longer than 15 s**, and wakes early when a command changes something. The 15 s
cap keeps the Python app's resilience: suspend/resume, clock changes and settings edits need no
re-arming.

Per tick, for each lesson that `occurs_on(today)` and has an openable link (a weekly template, a
one-off dated today, or a series with today inside its range -- each is opened once, because the
key is `(lesson, date)`):

| Condition | Action | Status |
|---|---|---|
| `start - notify <= now < start` | reminder (`reminder_due`, notifications on) | — |
| `start - lead <= now < start` | open browser | `opened` |
| `start <= now < end`, not yet fired | in-app banner: "Підключитися зараз" / "Закрити" | `manual` on connect, `missed` on close |
| `now >= end`, not yet fired | record only | `missed` |

The row is **inserted before** the browser call (`Storage::claim_occurrence`); a conflict on
`UNIQUE(lesson_id, occur_date)` means it already fired and the call is skipped. See
`ARCHITECTURE.md` -> "fire exactly once".

Only `https://` links are opened (`scheduler::is_openable`); a lesson with any other link is treated
as one without a link. 1.x also accepted `http://`.

Times are naive local wall time, as in 1.x. Across a DST change an hour that happens twice opens its
class once (the log is keyed by date), and a class in the hour that does not exist is offered, not
opened. `tests/scheduler.rs` pins both in `Europe/Kyiv`.

### The schedule starts when it is imported

`import_courses` stamps `schedule_active_from` with the current time, and `tick` skips any lesson
whose start on that date is earlier (`predates_schedule`). Skipped means *entirely*: not opened,
not offered, not recorded as missed.

A timetable describes what happens from the moment you set it up. Importing on a Saturday
afternoon to prepare for the week ahead must not stamp that Saturday's classes "missed". It keys on
the **start** time rather than the end, so a class that was already under way when the import
finished is left alone too.

### A running class is opened only if the app saw it start

Catch-up mode `open` opens a class that is already running only when the scheduler was running,
awake and on an unbroken clock when the class's trigger passed: its previous tick was before the
trigger and at most 45 s earlier, and the wall clock moved in step with the monotonic clock (within
30 s). After an app start, a wake from sleep or a clock change, a running class is always offered
(the missed-class banner) and never opened.

1.x had a one-tick cold-start guard: the class opened 15 s after start anyway, and a wake from sleep
opened it on the first tick (PORT_SPEC §I.12, §I.20). The golden comparison
(`tests/golden_scheduler.rs`) checks that these are the only differences from the 72 Python
simulations.

### User marks

`Storage::mark_occurrence` writes or overwrites an occurrence row directly, bypassing the
claim-before-open protocol. That is correct here and only here: the row records a decision the user
already made from the card menu ("позначити як відкриту" / "як пропущену"), and it is the same row
that makes `evaluate` answer "already fired", so a class marked skipped is never opened.
`clear_occurrence` undoes the mark and makes the class eligible again.

Statuses are per `(lesson, date)`, so a mark belongs to one week and the same class is untouched
on the next one.

`Scheduler::open_now` is the click path. For a date that has not come yet it opens the link and
leaves the log alone, so peeking at next week's class does not consume it.

### Advance reminders

`reminder_due` is a second pure function, deliberately not folded into `evaluate`. Folding it in
would mean a reminder and a trigger were both outcomes of one decision, one keystroke away from a
reminder opening a browser. Reminders are not recorded in `occurrences` — a message is not an
occurrence, and writing one would suppress the real open.

## 5. Autostart

`src-tauri/src/autostart.rs`, without a plugin. Windows: the `HKCU` key
`Software\Microsoft\Windows\CurrentVersion\Run`, value `AutoPara`, data `"<exe path>" --hidden`.
macOS: `~/Library/LaunchAgents/com.autopara.app.plist`, label `com.autopara.app`. Both are the
names 1.x used, so an upgrade replaces the old entry instead of adding a second one. `--hidden`
starts the app in the tray with no window.

It is enabled by default (`autostart_enabled = 1`) and synced to the setting on every launch. The
installer writes the byte-identical command (a test compares the template with
`autostart::startup_command`); if they ever diverge, the app silently rewrites the entry on its next
launch. A debug build never touches the entry, so a run from a checkout cannot point the user's
autostart into `target/`.

## 6. Theming

The tokens are CSS custom properties in `ui/src/styles/tokens.css`, one block per theme; `system`
follows the webview's `prefers-color-scheme`, so a fresh install matches the desktop. The rail
toggle pins the opposite of whatever is showing. The values are PORT_SPEC §F.

## 7. Updates

`src-tauri/src/updates.rs` on top of `tauri-plugin-updater`. Because this is a program that
downloads and then runs an installer, the rules are the point:

- **One source.** The plugin reads `latest.json` from the newest GitHub release of
  `reyjendiz/AutoPara` over HTTPS (the endpoint is in `tauri.conf.json`). Pre-releases are not
  "latest" and so are never offered.
- **Signed files only.** Every downloaded file is checked against the minisign public key in
  `tauri.conf.json`. The private key never enters the repository (docs/RELEASE_CHECKLIST.md).
- **Download may be automatic; installing never is.** `update_mode = auto` fetches the file and
  stops; installing is always the user's click.
- **Where it cannot install, it says so.** A debug build only says a version is out, with the
  release page.
- **A failed automatic check is silent; a failed manual one says so.**
- Schedule: first check 15 s after start, then every 6 h; the manual checks in Settings and the
  tray always run.

Windows runs the downloaded `Setup.exe` silently with `/S /UPDATE /R`; the installer closes the app,
replaces it and starts it again. macOS replaces `AutoPara.app` from a signed `.app.tar.gz` and
restarts (1.x only opened the `.dmg`). Neither has run end to end yet: that needs two published
releases.

1.x copies cannot update themselves to 2.0: see `docs/UPGRADING.md`.
