"""Which classes the calendar shows: a text search and a subject filter.

One rule, in one place, so the week, the day, the month, the "found N" count and the jump to the
next match cannot disagree about what a filter means. A pure function of the lessons and the
filter -- nothing here touches Qt or the database.

The search is forgiving in the ways people are: case does not matter, neither do the apostrophe
variants a Ukrainian text mixes (' ` ’ ʼ), and every word typed has to be found, in the subject or
the teacher, in any order -- so ``роман педаг`` finds "Загальна педагогіка (Роман Н.М.)".
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date, timedelta

from ..importer.normalize import normalize_text
from .models import Lesson

# How far ahead (or back) "next match" looks before giving up: a year and a bit of a timetable.
SEARCH_HORIZON_DAYS = 400


def _fold(text: str) -> str:
    return normalize_text(text or "").casefold()


@dataclass(frozen=True)
class LessonFilter:
    text: str = ""
    subject: str = ""  # an exact subject as it is written in the timetable; "" means all

    @property
    def active(self) -> bool:
        return bool(self.text.strip() or self.subject)

    def matches(self, lesson: Lesson) -> bool:
        if self.subject and _fold(lesson.subject) != _fold(self.subject):
            return False
        words = _fold(self.text).split()
        if not words:
            return True
        haystack = f"{_fold(lesson.subject)} {_fold(lesson.teacher)}"
        return all(word in haystack for word in words)


def apply(lessons: list[Lesson], flt: LessonFilter) -> list[Lesson]:
    """The lessons the filter lets through, in their original order."""
    if not flt.active:
        return list(lessons)
    return [lesson for lesson in lessons if flt.matches(lesson)]


_ALPHABET = "абвгґдеєжзиіїйклмнопрстуфхцчшщьюяabcdefghijklmnopqrstuvwxyz"
_ORDER = {letter: index for index, letter in enumerate(_ALPHABET)}


def alphabetical(text: str) -> list[int]:
    """A sort key in Ukrainian alphabetical order (code points put "і" before "а")."""
    return [_ORDER.get(char, len(_ALPHABET) + ord(char)) for char in _fold(text)]


def subjects(lessons: list[Lesson]) -> list[str]:
    """Every distinct subject, once, alphabetically -- what the subject drop-down offers."""
    seen: dict[str, str] = {}
    for lesson in lessons:
        seen.setdefault(_fold(lesson.subject), normalize_text(lesson.subject))
    return sorted(seen.values(), key=alphabetical)


def find_match(
    lessons: list[Lesson], flt: LessonFilter, start: date, direction: int = 1
) -> date | None:
    """The nearest date after (``direction`` 1) or before (-1) ``start`` with a matching class."""
    candidates = apply(lessons, flt)
    if not candidates:
        return None
    step = timedelta(days=1 if direction >= 0 else -1)
    day = start + step
    for _ in range(SEARCH_HORIZON_DAYS):
        if any(lesson.occurs_on(day) for lesson in candidates):
            return day
        day += step
    return None
