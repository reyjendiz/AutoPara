"""Dataclasses shared between storage, scheduler and UI.

These mirror the SQLite schema documented in docs/BACKEND.md section 3.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date, datetime, time

# Occurrence lifecycle.
STATUS_OPENED = "opened"   # auto-opened by the scheduler at (start - lead)
STATUS_MANUAL = "manual"   # the user opened it, from a card click or a catch-up notification
STATUS_MISSED = "missed"   # the class ended before anything opened it
STATUS_SKIPPED = "skipped" # the user dismissed it

PROVIDER_ZOOM = "zoom"
PROVIDER_MEET = "google_meet"
PROVIDER_UNKNOWN = "unknown"

# What to do about a class whose trigger moment was missed (docs/BACKEND.md section 4).
CATCHUP_NOTIFY = "notify"   # show the in-app prompt and open only when the user confirms
CATCHUP_OPEN = "open"       # open it straight away
CATCHUP_MISSED = "missed"   # do not open; just mark it

# What the calendar shows: one day, one week or one month. Stored as the ``view_mode`` setting.
VIEW_DAY = "day"
VIEW_WEEK = "week"
VIEW_MONTH = "month"
VIEW_MODES = (VIEW_DAY, VIEW_WEEK, VIEW_MONTH)

# Whether, and how, to look for a newer release on GitHub (docs/BACKEND.md section 7).
UPDATE_ASK = "ask"    # tell the user a version is out and let them choose to download it
UPDATE_AUTO = "auto"  # download it in the background; installing still waits for a click
UPDATE_OFF = "off"    # never check by itself
UPDATE_MODES = (UPDATE_ASK, UPDATE_AUTO, UPDATE_OFF)

# Colour scheme. ``system`` follows Windows' own app theme, which is what a fresh install uses.
THEME_SYSTEM = "system"
THEME_LIGHT = "light"
THEME_DARK = "dark"


def parse_hhmm(value: str) -> time:
    hour, minute = (int(part) for part in value.split(":"))
    return time(hour, minute)


@dataclass
class Course:
    id: int
    ordinal: int
    name: str


@dataclass
class Group:
    id: int
    course_id: int
    name: str
    specialty: str
    col_lo: int
    col_hi: int


@dataclass
class Lesson:
    id: int
    course_id: int
    day_index: int          # 0=Mon .. 6=Sun
    pair: int               # 1..6
    start_time: str         # 'HH:MM'
    end_time: str           # 'HH:MM'
    subject: str
    teacher: str = ""
    url: str | None = None
    provider: str = PROVIDER_UNKNOWN
    needs_link: bool = False
    is_manual: bool = False
    source_key: str = ""
    group_names: list[str] = field(default_factory=list)
    # A lesson without ``on_date`` is a weekly template (every imported class). With ``on_date`` it
    # belongs to that date alone, or -- when ``repeat_until`` is also set -- to every week from
    # ``on_date`` up to and including ``repeat_until``. ``day_index`` always equals
    # ``on_date.weekday()`` for a dated lesson. See docs/BACKEND.md section 3.
    on_date: date | None = None
    repeat_until: date | None = None

    @property
    def is_dated(self) -> bool:
        return self.on_date is not None

    def occurs_on(self, day: date) -> bool:
        """Whether this lesson takes place on ``day`` -- the one place that question is answered.

        A pure function of the lesson and the date, so the scheduler's pure decision functions and
        the calendar views agree on what a day contains. It knows nothing about when the timetable
        was imported; ``scheduler.predates_schedule`` owns that.
        """
        if day.weekday() != self.day_index:
            return False
        if self.on_date is None:
            return True
        last = self.repeat_until if self.repeat_until is not None else self.on_date
        return self.on_date <= day <= max(last, self.on_date)

    @property
    def start(self) -> time:
        return parse_hhmm(self.start_time)

    @property
    def end(self) -> time:
        return parse_hhmm(self.end_time)

    @property
    def pair_span(self) -> int:
        """How many consecutive pairs this lesson covers (a merged block covers several)."""
        from ..importer.normalize import last_pair_covered

        return max(1, last_pair_covered(self.start_time, self.end_time) - self.pair + 1)

    def starts_on(self, day: date) -> datetime:
        return datetime.combine(day, self.start)

    def ends_on(self, day: date) -> datetime:
        return datetime.combine(day, self.end)

    def time_range(self) -> str:
        return f"{self.start_time}–{self.end_time}"


@dataclass
class Occurrence:
    id: int
    lesson_id: int
    occur_date: str  # 'YYYY-MM-DD'
    status: str
    fired_at: str


@dataclass
class Settings:
    """Typed view over the ``settings`` key/value table."""

    selected_course_id: int | None = None
    selected_group_id: int | None = None
    lead_minutes: int = 1
    class_duration_minutes: int = 80
    autostart_enabled: bool = True
    catchup_mode: str = CATCHUP_NOTIFY
    last_import_path: str = ""
    notifications_enabled: bool = True
    notify_minutes: int = 10
    theme: str = THEME_SYSTEM
    view_mode: str = VIEW_WEEK
    update_mode: str = UPDATE_ASK
    skipped_update_version: str = ""  # a release the user said "not this one" to
    last_update_check: str = ""       # ISO datetime of the last answer from GitHub
    schedule_active_from: str = ""  # ISO datetime; classes before it are not this app's business
    schedule_copy_path: str = ""    # our own copy of the imported .docx (the original may go)
