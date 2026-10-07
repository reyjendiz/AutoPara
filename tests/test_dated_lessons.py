"""Lessons that belong to a date: a one-off class, or a weekly series with an end date.

These tests build their own timetable instead of reading the schedule document, so they say the
same thing on every machine. The browser is stubbed; nothing here opens a URL.
"""

from __future__ import annotations

import sqlite3
from datetime import date, datetime, timedelta

import pytest

from autopara.core import scheduler as scheduler_module
from autopara.core.models import CATCHUP_OPEN, Lesson
from autopara.core.scheduler import (
    ACTION_NONE,
    ACTION_OPEN,
    Scheduler,
    evaluate,
    reminder_due,
)
from autopara.core.storage import Storage
from autopara.importer.schedule_parser import ParsedCourse, ParsedGroup, ParsedLesson

# 2026-03-02 is a Monday.
MONDAY = date(2026, 3, 2)
NEXT_MONDAY = MONDAY + timedelta(days=7)
URL = "https://meet.google.com/aaa-bbbb-ccc"


def lesson(**overrides) -> Lesson:
    base = dict(
        id=1,
        course_id=1,
        day_index=0,
        pair=1,
        start_time="08:00",
        end_time="09:20",
        subject="Історія України",
        url=URL,
        provider="google_meet",
    )
    base.update(overrides)
    return Lesson(**base)


def one_off(day: date, **overrides) -> Lesson:
    return lesson(on_date=day, day_index=day.weekday(), **overrides)


def series(first: date, last: date, **overrides) -> Lesson:
    return lesson(on_date=first, repeat_until=last, day_index=first.weekday(), **overrides)


@pytest.fixture
def opened(monkeypatch):
    """Stub the browser; record what would have been opened."""
    calls: list[str] = []
    monkeypatch.setattr(scheduler_module, "open_url", lambda url: calls.append(url) or True)
    return calls


@pytest.fixture
def world(tmp_path):
    """An imported course with one group and a single weekly class on Wednesday."""
    storage = Storage(tmp_path / "dated.db")
    storage.import_courses(
        [
            ParsedCourse(
                ordinal=1,
                name="I курс",
                groups=[ParsedGroup("11 група", "", 3, 3)],
                lessons=[
                    ParsedLesson(
                        day_index=2,
                        pair=2,
                        start_time="09:30",
                        end_time="10:50",
                        subject="Шаблонна пара",
                        teacher="",
                        url=URL,
                        provider="google_meet",
                        group_names=["11 група"],
                    )
                ],
            )
        ]
    )
    course = storage.courses()[0]
    group = storage.groups(course.id)[0]
    storage.set_setting("selected_course_id", course.id)
    storage.set_setting("selected_group_id", group.id)
    storage.set_setting("schedule_active_from", datetime(2026, 3, 1, 20, 0).isoformat())
    yield storage, group
    storage.close()


def add(storage, group, item: Lesson) -> Lesson:
    item.course_id = group.course_id
    return storage.lesson(storage.add_lesson(item, [group.id]))


class TestOccursOn:
    def test_a_template_happens_every_week_on_its_weekday(self):
        item = lesson()
        assert item.occurs_on(MONDAY)
        assert item.occurs_on(NEXT_MONDAY)
        assert not item.occurs_on(MONDAY + timedelta(days=1))

    def test_a_one_off_happens_on_its_date_only(self):
        item = one_off(NEXT_MONDAY)
        assert item.occurs_on(NEXT_MONDAY)
        assert not item.occurs_on(MONDAY)
        assert not item.occurs_on(NEXT_MONDAY + timedelta(days=7))

    def test_a_series_covers_both_ends_and_steps_by_weeks(self):
        item = series(MONDAY, MONDAY + timedelta(days=14))
        assert [item.occurs_on(MONDAY + timedelta(days=n)) for n in range(0, 22, 7)] == [
            True, True, True, False,
        ]
        assert not item.occurs_on(MONDAY + timedelta(days=1)), "only on its own weekday"
        assert not item.occurs_on(MONDAY - timedelta(days=7))

    def test_a_series_ending_before_it_starts_is_still_its_first_day(self):
        item = series(NEXT_MONDAY, MONDAY)
        assert item.occurs_on(NEXT_MONDAY)
        assert not item.occurs_on(NEXT_MONDAY + timedelta(days=7))


class TestStorage:
    def test_dates_round_trip(self, world):
        storage, group = world
        stored = add(storage, group, series(NEXT_MONDAY, NEXT_MONDAY + timedelta(days=14)))
        assert stored.on_date == NEXT_MONDAY
        assert stored.repeat_until == NEXT_MONDAY + timedelta(days=14)
        assert stored.is_manual and stored.is_dated

    def test_the_weekday_follows_the_date(self, world):
        storage, group = world
        wrong = one_off(NEXT_MONDAY)
        wrong.day_index = 4  # a caller that forgot to keep it consistent
        assert add(storage, group, wrong).day_index == NEXT_MONDAY.weekday()

    def test_a_template_has_no_dates(self, world):
        storage, group = world
        imported = storage.lessons_for_group(group.id)[0]
        assert imported.on_date is None and imported.repeat_until is None
        assert not imported.is_dated

    def test_update_changes_the_dates(self, world):
        storage, group = world
        stored = add(storage, group, one_off(NEXT_MONDAY))
        stored.on_date = NEXT_MONDAY + timedelta(days=2)
        stored.repeat_until = NEXT_MONDAY + timedelta(days=16)
        storage.update_lesson(stored)
        again = storage.lesson(stored.id)
        assert again.on_date == NEXT_MONDAY + timedelta(days=2)
        assert again.day_index == (NEXT_MONDAY + timedelta(days=2)).weekday()
        assert again.repeat_until == NEXT_MONDAY + timedelta(days=16)

    def test_moving_a_one_off_changes_its_date(self, world):
        storage, group = world
        stored = add(storage, group, one_off(NEXT_MONDAY))
        target = NEXT_MONDAY + timedelta(days=3)
        storage.move_lesson(stored.id, 3, 3, "11:20", "12:40", on_date=target)
        moved = storage.lesson(stored.id)
        assert (moved.on_date, moved.day_index, moved.start_time) == (target, 3, "11:20")
        assert moved.repeat_until is None

    def test_moving_a_series_keeps_its_length(self, world):
        storage, group = world
        stored = add(storage, group, series(NEXT_MONDAY, NEXT_MONDAY + timedelta(days=14)))
        target = NEXT_MONDAY + timedelta(days=1)
        storage.move_lesson(stored.id, 1, 1, "08:00", "09:20", on_date=target)
        moved = storage.lesson(stored.id)
        assert moved.on_date == target
        assert moved.repeat_until == NEXT_MONDAY + timedelta(days=15)
        assert moved.day_index == target.weekday()

    def test_moving_a_template_ignores_dates(self, world):
        storage, group = world
        imported = storage.lessons_for_group(group.id)[0]
        storage.move_lesson(imported.id, 4, 3, "11:20", "12:40")
        moved = storage.lesson(imported.id)
        assert moved.day_index == 4 and moved.on_date is None

    def test_reimport_keeps_dated_lessons(self, world):
        storage, group = world
        kept = add(storage, group, series(NEXT_MONDAY, NEXT_MONDAY + timedelta(days=14)))
        course = ParsedCourse(
            ordinal=1,
            name="I курс",
            groups=[ParsedGroup("11 група", "", 3, 3)],
            lessons=[],
        )
        storage.import_courses([course])
        survivors = storage.lessons_for_group(group.id)
        assert [item.id for item in survivors] == [kept.id]

    def test_a_database_from_before_dates_is_upgraded(self, tmp_path):
        path = tmp_path / "old.db"
        old = sqlite3.connect(path)
        old.executescript(
            """
            CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
            CREATE TABLE courses (id INTEGER PRIMARY KEY AUTOINCREMENT,
                                  ordinal INTEGER NOT NULL UNIQUE, name TEXT NOT NULL);
            CREATE TABLE lessons (
                id INTEGER PRIMARY KEY AUTOINCREMENT, course_id INTEGER NOT NULL,
                day_index INTEGER NOT NULL, pair INTEGER NOT NULL,
                start_time TEXT NOT NULL, end_time TEXT NOT NULL, subject TEXT NOT NULL,
                teacher TEXT NOT NULL DEFAULT '', url TEXT,
                provider TEXT NOT NULL DEFAULT 'unknown', needs_link INTEGER NOT NULL DEFAULT 0,
                is_manual INTEGER NOT NULL DEFAULT 0, source_key TEXT NOT NULL DEFAULT ''
            );
            INSERT INTO courses(ordinal, name) VALUES (1, 'I курс');
            INSERT INTO lessons(course_id, day_index, pair, start_time, end_time, subject)
                VALUES (1, 0, 1, '08:00', '09:20', 'Стара пара');
            """
        )
        old.commit()
        old.close()

        storage = Storage(path)
        try:
            row = storage.connection.execute("SELECT * FROM lessons").fetchone()
            assert row["on_date"] is None and row["repeat_until"] is None
            assert storage.lesson(row["id"]).occurs_on(MONDAY)
        finally:
            storage.close()
        Storage(path).close()  # opening it a second time must not trip over the new columns


class TestPureDecisions:
    def test_evaluate_ignores_the_same_weekday_on_another_date(self):
        item = one_off(NEXT_MONDAY)
        before_start = datetime.combine(MONDAY, datetime.min.time()).replace(hour=7, minute=59)
        assert evaluate(item, before_start, 1, False).action == ACTION_NONE

        on_the_day = before_start + timedelta(days=7)
        assert evaluate(item, on_the_day, 1, False).action == ACTION_OPEN

    def test_reminder_respects_the_date(self):
        item = one_off(NEXT_MONDAY)
        early = datetime(2026, 3, 2, 7, 55)
        assert reminder_due(item, early, 10, False) is False
        assert reminder_due(item, early + timedelta(days=7), 10, False) is True


class TestSchedulerFiresOnce:
    def test_a_one_off_opens_once_on_its_date(self, world, opened, qapp):
        storage, group = world
        add(storage, group, one_off(NEXT_MONDAY))
        scheduler = Scheduler(storage)

        scheduler.tick(datetime(2026, 3, 2, 7, 59))   # the Monday before: not its date
        assert opened == []

        scheduler.tick(datetime(2026, 3, 9, 7, 59))
        scheduler.tick(datetime(2026, 3, 9, 7, 59, 30))
        assert opened == [URL]

        scheduler.tick(datetime(2026, 3, 16, 7, 59))  # a week later: it has happened
        assert opened == [URL]

    def test_a_series_opens_once_per_week_until_its_end(self, world, opened, qapp):
        storage, group = world
        add(storage, group, series(MONDAY, MONDAY + timedelta(days=14)))
        scheduler = Scheduler(storage)

        for week in range(4):
            day = MONDAY + timedelta(days=7 * week)
            scheduler.tick(datetime.combine(day, datetime.min.time()).replace(hour=7, minute=59))
            scheduler.tick(datetime.combine(day, datetime.min.time()).replace(hour=8, minute=0))

        assert len(opened) == 3, "the fourth Monday is after repeat_until"

    def test_the_template_still_opens_on_every_week(self, world, opened, qapp):
        storage, _ = world
        scheduler = Scheduler(storage)
        for day in (date(2026, 3, 4), date(2026, 3, 11)):
            scheduler.tick(datetime(day.year, day.month, day.day, 9, 29))
        assert len(opened) == 2

    def test_a_running_dated_class_is_never_opened_unasked_on_a_cold_start(
        self, world, opened, qapp
    ):
        storage, group = world
        add(storage, group, one_off(NEXT_MONDAY))
        storage.set_setting("catchup_mode", CATCHUP_OPEN)
        scheduler = Scheduler(storage)
        offered: list[int] = []
        scheduler.catchup_available.connect(offered.append)

        scheduler.tick(datetime(2026, 3, 9, 8, 30))

        assert opened == []
        assert offered
