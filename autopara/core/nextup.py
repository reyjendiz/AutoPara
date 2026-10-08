"""What to say about the class that is happening, or happens next, today.

A pure function of the lessons and the clock, so the wording and the choice of class are tested
without a window. The window shows the result in a small pill beside the title.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime

from .models import Lesson

# Beyond this many minutes "in 3 hours" stops being useful: the time of day says more.
COUNTDOWN_LIMIT_MINUTES = 90


@dataclass(frozen=True)
class NextUp:
    lesson: Lesson
    ongoing: bool   # it has started and not ended
    minutes: int    # until it starts, or -- when ongoing -- until it ends


def find(
    lessons: list[Lesson], now: datetime, settled: set[int] | None = None
) -> NextUp | None:
    """The class that is on right now, else the next one to start today; None when the day is done.

    ``settled`` is the ids of today's classes that were skipped or are already over for the user's
    purposes: they are not "next".
    """
    today = now.date()
    skip = settled or set()
    candidates = [
        lesson for lesson in lessons
        if lesson.occurs_on(today) and lesson.id not in skip and lesson.ends_on(today) > now
    ]
    if not candidates:
        return None
    lesson = min(candidates, key=lambda item: item.start_time)
    start, end = lesson.starts_on(today), lesson.ends_on(today)
    if start <= now:
        return NextUp(lesson, True, max(1, round((end - now).total_seconds() / 60)))
    return NextUp(lesson, False, max(1, round((start - now).total_seconds() / 60)))


def label(item: NextUp) -> str:
    """``Зараз · Психологія · ще 40 хв`` or ``Далі · Психологія · через 25 хв`` / ``· о 14:40``."""
    if item.ongoing:
        return f"Зараз · {item.lesson.subject} · ще {item.minutes} хв"
    if item.minutes <= COUNTDOWN_LIMIT_MINUTES:
        when = f"через {item.minutes} хв"
    else:
        when = f"о {item.lesson.start_time}"
    return f"Далі · {item.lesson.subject} · {when}"
