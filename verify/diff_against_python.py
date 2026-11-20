"""Differential verification: runs original Python AutoPara vs Rust autopara-core
on edge cases, sample_timetable.docx, filters, nextup, and scheduler rules.

Zero mismatches = behavioral parity proven (rust-anything methodology).
"""

from __future__ import annotations

import dataclasses
import json
import subprocess
import sys
import tempfile
from datetime import date, datetime
from pathlib import Path

ORIG_ROOT = Path(r"C:\Users\anire\.gemini\antigravity\scratch\AutoPara-orig")
RUST_BIN = Path(r"D:\para\target\debug\autopara-verify.exe")

sys.path.insert(0, str(ORIG_ROOT))

# Stub PySide6 minimally so core/scheduler.py pure functions import without Qt installed.
import types

pyside = types.ModuleType("PySide6")
qtcore = types.ModuleType("PySide6.QtCore")


class _Dummy:
    def __init__(self, *a, **kw):
        pass


qtcore.QObject = _Dummy
qtcore.QTimer = _Dummy
qtcore.Signal = lambda *a, **kw: None
pyside.QtCore = qtcore
sys.modules.setdefault("PySide6", pyside)
sys.modules.setdefault("PySide6.QtCore", qtcore)

from autopara.core import filters, nextup, scheduler
from autopara.core.models import Lesson
from autopara.importer import normalize, schedule_parser
from tests import sample_timetable


def to_lesson(d: dict) -> Lesson:
    return Lesson(
        id=d.get("id", 1),
        course_id=d.get("course_id", 1),
        day_index=d.get("day_index", 0),
        pair=d.get("pair", 1),
        start_time=d.get("start_time", "08:00"),
        end_time=d.get("end_time", "09:20"),
        subject=d.get("subject", ""),
        teacher=d.get("teacher", ""),
        url=d.get("url"),
        provider=d.get("provider", "unknown"),
        needs_link=d.get("needs_link", False),
        is_manual=d.get("is_manual", False),
        source_key=d.get("source_key", ""),
        group_names=d.get("group_names", []),
        on_date=date.fromisoformat(d["on_date"]) if d.get("on_date") else None,
        repeat_until=date.fromisoformat(d["repeat_until"]) if d.get("repeat_until") else None,
        skip_dates=frozenset(date.fromisoformat(x) for x in d.get("skip_dates", [])),
    )


def subject_color_py(subject: str) -> str:
    import hashlib

    palette = ["#ec6b66", "#f1f36a", "#86cdf7", "#8566e6", "#8fe8a8", "#f26c9c", "#a8bdd6"]
    digest = hashlib.md5(subject.strip().casefold().encode("utf-8")).hexdigest()
    return palette[int(digest[:8], 16) % len(palette)]


def run_python(op: str, args: list):
    if op == "normalize_text":
        return normalize.normalize_text(args[0])
    if op == "day_index":
        return normalize.day_index(args[0])
    if op == "parse_time":
        return normalize.parse_time(args[0])
    if op == "parse_time_range":
        return list(normalize.parse_time_range(args[0]))
    if op == "pair_from_time":
        return normalize.pair_from_time(args[0])
    if op == "pair_slot":
        return normalize.pair_slot(args[0])
    if op == "last_pair_covered":
        return normalize.last_pair_covered(args[0], args[1])
    if op == "add_minutes":
        return normalize.add_minutes(args[0], args[1])
    if op == "minutes_between":
        return normalize.minutes_between(args[0], args[1])
    if op == "clean_url":
        return normalize.clean_url(args[0])
    if op == "find_urls":
        return normalize.find_urls(args[0])
    if op == "detect_provider":
        return normalize.detect_provider(args[0])
    if op == "is_course_heading":
        return normalize.is_course_heading(args[0])
    if op == "course_label":
        return normalize.course_label(args[0])
    if op == "subject_color":
        return subject_color_py(args[0])
    if op == "parse_file":
        courses = schedule_parser.parse_file(args[0], args[1] if len(args) > 1 else 80)
        return [dataclasses.asdict(c) for c in courses]
    if op == "evaluate":
        d = scheduler.evaluate(
            to_lesson(args[0]),
            datetime.fromisoformat(args[1]),
            args[2],
            args[3],
        )
        return {"action": d.action, "lesson_id": d.lesson_id, "reason": d.reason}
    if op == "reminder_due":
        return scheduler.reminder_due(
            to_lesson(args[0]),
            datetime.fromisoformat(args[1]),
            args[2],
            args[3],
        )
    if op == "filter_subjects":
        return filters.subjects([to_lesson(x) for x in args[0]])
    if op == "filter_apply":
        flt = filters.LessonFilter(text=args[1], subject=args[2])
        return [l.id for l in filters.apply([to_lesson(x) for x in args[0]], flt)]
    if op == "nextup":
        item = nextup.find(
            [to_lesson(x) for x in args[0]],
            datetime.fromisoformat(args[1]),
            set(args[2]),
        )
        if item is None:
            return None
        return {
            "lesson_id": item.lesson.id,
            "ongoing": item.ongoing,
            "minutes": item.minutes,
            "label_uk": nextup.label(item),
        }
    raise ValueError(f"Unknown op: {op}")


def build_cases(docx_path: Path) -> list[dict]:
    cases: list[dict] = []

    for s in [
        "  П`ятниця  ",
        "П’ятниця\t\n  1",
        "Історія України",
        "  ",
        "don't\u02BCt",
        "йогурт",  # NFD
    ]:
        cases.append({"op": "normalize_text", "args": [s]})

    for d in [
        "Понеділок",
        "Понеділок 02.09",
        "П`ятниця",
        "Вівторок.",
        "Wednesday",
        "środa",
        "Freitag",
        "lunedì",
        "sâmbătă",
        "so",
        "пн",
        "нд",
        "неділя",
        "",
        "random",
    ]:
        cases.append({"op": "day_index", "args": [d]})

    for t in ["8.00", "08:00", "9.30", "23:59", "24:00", "9.60", "bad", "14.40"]:
        cases.append({"op": "parse_time", "args": [t]})

    for tr in [
        "8.00",
        "8.00-9.20",
        "09:30 – 10:50",
        "14.40 - 13.00",
        "pair 11:20 to 12:40",
        "no time here",
    ]:
        cases.append({"op": "parse_time_range", "args": [tr]})

    for t in ["8.00", "09:30", "11.20", "16:10", "22:10", "10:00", "07:00"]:
        cases.append({"op": "pair_from_time", "args": [t]})
        cases.append({"op": "pair_slot", "args": [t]})

    for s, e in [("08:00", "09:20"), ("09:30", "12:40"), ("08:00", "12:40"), ("14:40", "16:00")]:
        cases.append({"op": "last_pair_covered", "args": [s, e]})
        cases.append({"op": "minutes_between", "args": [s, e]})

    for s, m in [("08:00", 80), ("23:30", 80), ("16:10", 80)]:
        cases.append({"op": "add_minutes", "args": [s, m]})

    for u in [
        "https://zoom.us/j/12345.",
        "https://meet.google.com/aaa-bbbb-ccc);",
        "See https://zoom.us/j/1 and http://meet.google.com/x-y-z!",
    ]:
        cases.append({"op": "clean_url", "args": [u]})
        cases.append({"op": "find_urls", "args": [u]})

    for u in [
        None,
        "",
        "https://meet.google.com/aaa-bbbb-ccc",
        "https://us02web.zoom.us/j/123",
        "https://zoom.com/j/999",
        "https://teams.microsoft.com/l/meetup",
    ]:
        cases.append({"op": "detect_provider", "args": [u]})

    for h in ["І КУРС", "IV Course", "Year 2", "2 rok", "hello world"]:
        cases.append({"op": "is_course_heading", "args": [h]})

    for o in [1, 2, 4, 6, 9]:
        cases.append({"op": "course_label", "args": [o]})

    for subj in ["Історія України", "Психологія", "Загальна педагогіка", "Daily standup", "Логіка"]:
        cases.append({"op": "subject_color", "args": [subj]})

    # Full synthetic .docx parsing (all 6 tables, R1..R12)
    cases.append({"op": "parse_file", "args": [str(docx_path), 80]})

    # Scheduler evaluate + reminder_due across full lifecycle
    base_lesson = {
        "id": 7,
        "day_index": 0,
        "pair": 2,
        "start_time": "09:30",
        "end_time": "10:50",
        "subject": "Загальна педагогіка",
        "teacher": "Роман Н.М.",
        "url": "https://us02web.zoom.us/j/1000000001",
    }
    dated_lesson = {**base_lesson, "id": 8, "on_date": "2026-03-02", "repeat_until": "2026-03-16", "skip_dates": ["2026-03-09"]}
    for ts in [
        "2026-03-02T09:15:00",
        "2026-03-02T09:20:00",
        "2026-03-02T09:29:00",
        "2026-03-02T09:30:00",
        "2026-03-02T10:15:00",
        "2026-03-02T10:50:00",
        "2026-03-03T09:29:00",
        "2026-03-09T09:29:00",
        "2026-03-16T09:29:00",
        "2026-03-23T09:29:00",
    ]:
        cases.append({"op": "evaluate", "args": [base_lesson, ts, 1, False]})
        cases.append({"op": "evaluate", "args": [dated_lesson, ts, 1, False]})
        cases.append({"op": "evaluate", "args": [base_lesson, ts, 1, True]})
        cases.append({"op": "reminder_due", "args": [base_lesson, ts, 10, False]})
        cases.append({"op": "reminder_due", "args": [dated_lesson, ts, 10, False]})

    # Filters + Ukrainian alphabetical order + NextUp
    sample_lessons = [
        {"id": 1, "day_index": 0, "start_time": "08:00", "end_time": "09:20", "subject": "Історія України", "teacher": "Іваненко І.І."},
        {"id": 2, "day_index": 0, "start_time": "09:30", "end_time": "10:50", "subject": "Загальна педагогіка", "teacher": "Роман Н.М."},
        {"id": 3, "day_index": 0, "start_time": "11:20", "end_time": "12:40", "subject": "Європейська культура", "teacher": "Бондар А.В."},
        {"id": 4, "day_index": 0, "start_time": "14:40", "end_time": "16:00", "subject": "Ґрунтознавство", "teacher": "Мельник С.Д."},
    ]
    cases.append({"op": "filter_subjects", "args": [sample_lessons]})
    cases.append({"op": "filter_apply", "args": [sample_lessons, "роман педаг", ""]})
    cases.append({"op": "filter_apply", "args": [sample_lessons, "", "історія україни"]})
    cases.append({"op": "nextup", "args": [sample_lessons, "2026-03-02T07:00:00", []]})
    cases.append({"op": "nextup", "args": [sample_lessons, "2026-03-02T08:40:00", []]})
    cases.append({"op": "nextup", "args": [sample_lessons, "2026-03-02T09:25:00", [1]]})
    cases.append({"op": "nextup", "args": [sample_lessons, "2026-03-02T17:00:00", []]})

    return cases


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        docx_path = Path(tmp) / "sample.docx"
        sample_timetable.build(docx_path)
        cases = build_cases(docx_path)

        payload = "\n".join(json.dumps(c, ensure_ascii=False) for c in cases) + "\n"
        proc = subprocess.run(
            [str(RUST_BIN)],
            input=payload,
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=True,
        )
        rust_results = [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]
        assert len(rust_results) == len(cases), f"len mismatch: {len(rust_results)} vs {len(cases)}"

        mismatches = 0
        for idx, (case, rust_val) in enumerate(zip(cases, rust_results), start=1):
            py_val = run_python(case["op"], case["args"])
            if py_val != rust_val:
                mismatches += 1
                print(f"MISMATCH #{idx} ({case['op']}):")
                print(f"  args: {case['args']}")
                print(f"  py  : {py_val}")
                print(f"  rust: {rust_val}")

        print(f"Differential check: {len(cases) - mismatches}/{len(cases)} cases matched ({mismatches} mismatches)")
        return 1 if mismatches else 0


if __name__ == "__main__":
    raise SystemExit(main())
