# Golden fixtures

Output of the Python implementation at `8e73aff` (version 1.10.0). The Rust parser and scheduler
must reproduce these files exactly (`cargo xtask golden`). Nothing here comes from the real
timetable: every subject, teacher and link is invented.

| Input | What it exercises |
|---|---|
| `sample_timetable.docx` | `tests/sample_timetable.py`, the suite's synthetic timetable: six courses, shared classes by `gridSpan`, an R12 double class, R5 vMerge blocks, a wide header, an empty course, a backtick Friday. |
| `edge_cases.docx` | Built with the same helpers: a row before any day, a shared class written as duplicated text (fallback dedupe), an explicit `8.00-9.20` range, a blank pair cell, a retyped double class without teacher or link, a URL only in prose with trailing punctuation, the same subject twice a day with a gap, a dated day cell, an off-slot start, a content-less vMerge restart, an unknown day word, an unreadable time, `ПТ.`, a hyperlink whose text is not the URL, a nested table, NFD text, `w:tab` and `w:cr`, a one-row table, a three-column header with a cell overlapping two groups, a non-hyperlink relationship. |
| `multilingual.docx` | Polish, German, French and Spanish day names, `8:00–9:20` and `9.30 - 10.50` ranges, a Russian table running to Sunday. |

For each input:

- `<name>.parse.json` — `parse_file(path)` with the default 80-minute class length: courses (ordinal,
  name), groups (name, specialty, column range) and lessons (every `ParsedLesson` field plus
  `needs_link` and `source_key`), in the parser's order.
- `<name>.scheduler.json` —
  - `pure`: `evaluate` for every lesson on its weekday in the week of `reference_monday`, at offsets
    of −601…+1 s around the start and around the end, with lead 0/1/5 minutes and
    `already_fired` false/true; `evaluate` the day before and after; `reminder_due` with notify 0/10;
    `predates_schedule` with the import stamped 1 s before, at and after the start. `lesson` is the
    index within the course's `lessons` in the parse file.
  - `simulations`: the real `Scheduler` driven tick by tick (every `tick_seconds`) over a temporary
    database with the document imported, for the first group with classes in each course. Each
    scenario lists its settings, its awake segments (`cold_start` true = a new scheduler, i.e. an app
    start), every signal and browser hand-off in order (`lesson` = `source_key`), and the final
    `occurrences` rows. Scenarios: a full week running from 07:00, a 10:00 start in each catch-up
    mode, a sleep across a class start in `open` mode, a restart inside the lead window, an import at
    noon, and a five-minute lead with notifications off.

The generator ran the repository's own Python code once; it is not committed (plan rule R1).
