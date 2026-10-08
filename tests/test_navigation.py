"""Moving through time: the navigation bar, dated columns, and what a click means on each date.

Like ``test_dated_lessons`` these build their own timetable, so they run (and say the same thing)
without the schedule document. The browser is stubbed throughout.
"""

from __future__ import annotations

from datetime import date, timedelta

import pytest

from autopara.core.models import STATUS_SKIPPED, Lesson
from autopara.core.scheduler import Scheduler
from autopara.core.storage import Storage
from autopara.importer.normalize import week_start
from autopara.importer.schedule_parser import ParsedCourse, ParsedGroup, ParsedLesson
from autopara.ui.class_card import ClassCard
from autopara.ui.nav_bar import month_title, week_title
from autopara.ui.week_grid import GridCell, WeekGrid

# 2026-03-02 is a Monday.
MONDAY = date(2026, 3, 2)
URL = "https://meet.google.com/aaa-bbbb-ccc"


def weekly(day_index: int = 2, subject: str = "Шаблонна пара") -> Lesson:
    return Lesson(
        id=1,
        course_id=1,
        day_index=day_index,
        pair=2,
        start_time="09:30",
        end_time="10:50",
        subject=subject,
        url=URL,
        provider="google_meet",
    )


def dated(day: date, **overrides) -> Lesson:
    base = dict(
        id=0,
        course_id=1,
        day_index=day.weekday(),
        pair=1,
        start_time="08:00",
        end_time="09:20",
        subject="Разова пара",
        url=URL,
        provider="google_meet",
        on_date=day,
    )
    base.update(overrides)
    return Lesson(**base)


@pytest.fixture
def world(tmp_path):
    storage = Storage(tmp_path / "nav.db")
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
    yield storage, group
    storage.close()


@pytest.fixture
def window(world, qapp, monkeypatch):
    from autopara.ui.main_window import MainWindow

    storage, _ = world
    win = MainWindow(storage, Scheduler(storage))
    win.anchor = MONDAY
    win.reload()
    return win


def cards(grid: WeekGrid) -> list[ClassCard]:
    return [
        card for card in grid.findChildren(ClassCard)
        if card.parent() is not None and not card.parent().isHidden()
    ]


class TestTitles:
    def test_a_week_inside_one_month(self):
        assert week_title(MONDAY) == "2–8 березня 2026"

    def test_a_week_across_two_months(self):
        assert week_title(date(2026, 3, 30)) == "30 березня – 5 квітня 2026"

    def test_a_week_across_a_new_year(self):
        assert week_title(date(2026, 12, 28)) == "28 грудня 2026 – 3 січня 2027"

    def test_a_month(self):
        assert month_title(date(2026, 10, 7)) == "Жовтень 2026"


class TestDatedColumns:
    def test_columns_are_the_dates_asked_for(self, world, qapp):
        storage, group = world
        grid = WeekGrid()
        grid.render_week(storage.lessons_for_group(group.id), monday=MONDAY, today=MONDAY)
        assert grid._dates[0] == MONDAY and grid._dates[5] == MONDAY + timedelta(days=5)
        assert len(grid._dates) == 6

    def test_a_one_off_appears_only_in_its_week(self, world, qapp):
        storage, group = world
        storage.add_lesson(dated(MONDAY + timedelta(days=7)), [group.id])
        lessons = storage.lessons_for_group(group.id)

        grid = WeekGrid()
        grid.render_week(lessons, monday=MONDAY, today=MONDAY)
        assert [card.lesson.subject for card in cards(grid)] == ["Шаблонна пара"]

        grid.render_week(lessons, monday=MONDAY + timedelta(days=7), today=MONDAY)
        assert sorted(card.lesson.subject for card in cards(grid)) == [
            "Разова пара", "Шаблонна пара",
        ]

    def test_a_series_is_drawn_on_each_of_its_dates(self, world, qapp):
        storage, group = world
        storage.add_lesson(
            dated(MONDAY, repeat_until=MONDAY + timedelta(days=14)), [group.id]
        )
        grid = WeekGrid()
        lessons = storage.lessons_for_group(group.id)
        shown = []
        for week in range(4):
            grid.render_week(lessons, monday=MONDAY + timedelta(days=7 * week), today=MONDAY)
            shown.append(sum(c.lesson.subject == "Разова пара" for c in cards(grid)))
        assert shown == [1, 1, 1, 0]

    def test_signals_carry_the_date_not_just_the_lesson(self, world, qapp):
        storage, group = world
        grid = WeekGrid()
        grid.render_week(
            storage.lessons_for_group(group.id), monday=MONDAY + timedelta(days=7), today=MONDAY
        )
        clicked: list[tuple[int, date]] = []
        grid.lesson_clicked.connect(lambda lesson_id, day: clicked.append((lesson_id, day)))

        card = cards(grid)[0]
        card.clicked.emit(card.lesson.id)
        assert clicked == [(card.lesson.id, MONDAY + timedelta(days=9))]

    def test_an_empty_slot_reports_its_own_date(self, world, qapp):
        storage, group = world
        grid = WeekGrid()
        grid.render_week(
            storage.lessons_for_group(group.id), monday=MONDAY + timedelta(days=14), today=MONDAY
        )
        seen: list[tuple[date, str]] = []
        grid.slot_clicked.connect(lambda day, start: seen.append((day, start)))
        cell = next(
            c for c in grid.findChildren(GridCell)
            if c.parent() is not None and (c.day_index, c.hour) == (4, 15)
        )
        cell.clicked.emit(cell.day, cell.start_time)
        assert seen == [(MONDAY + timedelta(days=18), "15:00")]

    def test_only_the_real_today_is_marked(self, world, qapp):
        storage, group = world
        grid = WeekGrid()
        grid.render_week(storage.lessons_for_group(group.id), monday=MONDAY, today=date(2026, 3, 4))
        flagged = [
            cell.day for cell in grid.findChildren(GridCell)
            if cell.parent() is not None and cell.property("today") == "true"
        ]
        assert set(flagged) == {date(2026, 3, 4)}


class TestWindowNavigation:
    def test_forward_and_back_move_a_week(self, window):
        window.nav.next_button.click()
        assert window.monday() == MONDAY + timedelta(days=7)
        window.nav.previous_button.click()
        window.nav.previous_button.click()
        assert window.monday() == MONDAY - timedelta(days=7)

    def test_the_title_follows_the_week(self, window):
        assert (window.nav.title.text(), window.nav.sub.text()) == ("2–8 березня", "2026")
        window.nav.next_button.click()
        assert (window.nav.title.text(), window.nav.sub.text()) == ("9–15 березня", "2026")

    def test_today_returns_to_this_week(self, window):
        window.nav.next_button.click()
        window.nav.today_button.click()
        assert window.monday() == week_start(date.today())

    def test_an_empty_week_keeps_the_grid_so_there_is_a_way_out(self, window, world):
        """A week with nothing in it is still a week; hiding the grid would strand the user."""
        storage, group = world
        storage.delete_lesson(storage.lessons_for_group(group.id)[0].id)
        storage.add_lesson(dated(MONDAY + timedelta(days=21)), [group.id])
        window.reload()
        assert window.current_lessons() == []
        assert window.grid.isVisibleTo(window) and window.nav.isVisibleTo(window)

    def test_a_group_with_no_lessons_at_all_shows_the_empty_state(self, window, world):
        storage, group = world
        storage.delete_lesson(storage.lessons_for_group(group.id)[0].id)
        window.reload()
        assert window.grid.isHidden() and window.nav.isHidden()

    def test_marks_belong_to_the_date_on_screen(self, window, world):
        storage, group = world
        lesson = storage.lessons_for_group(group.id)[0]
        window.nav.next_button.click()
        day = window.date_of(lesson)
        window.scheduler.mark(lesson.id, STATUS_SKIPPED, day)
        window.reload()
        assert window.week_statuses(window.current_lessons())[lesson.id] == STATUS_SKIPPED

        window.nav.previous_button.click()
        assert window.week_statuses(window.current_lessons()) == {}


class TestClicksOnOtherDates:
    @pytest.fixture
    def opens(self, window, monkeypatch):
        calls: list[str] = []
        monkeypatch.setattr(
            "autopara.core.scheduler.open_url", lambda url: calls.append(url) or True
        )
        return calls

    def test_peeking_at_a_future_class_does_not_use_it_up(self, window, world, opens):
        """Claiming a date that has not come would leave the class unopened when it does."""
        storage, group = world
        lesson = storage.lessons_for_group(group.id)[0]
        future = date.today() + timedelta(days=30)
        window._lesson_clicked(lesson.id, future)

        assert opens == [URL], "the link itself still opens"
        assert storage.occurrence(lesson.id, future) is None

    def test_a_click_on_a_past_date_is_recorded_as_before(self, window, world, opens):
        storage, group = world
        lesson = storage.lessons_for_group(group.id)[0]
        past = date.today() - timedelta(days=30)
        window._lesson_clicked(lesson.id, past)
        assert storage.occurrence(lesson.id, past) is not None

    def test_dropping_a_one_off_moves_its_date(self, window, world):
        storage, group = world
        target = MONDAY + timedelta(days=8)  # a Tuesday next week
        stored = storage.lesson(storage.add_lesson(dated(MONDAY + timedelta(days=7)), [group.id]))
        window._lesson_dropped(stored.id, target, "11:00")
        moved = storage.lesson(stored.id)
        assert (moved.on_date, moved.day_index, moved.start_time) == (target, 1, "11:00")

    def test_dropping_a_template_changes_the_weekday(self, window, world):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        window._lesson_dropped(template.id, MONDAY + timedelta(days=11), "11:00")  # a Friday
        moved = storage.lesson(template.id)
        assert (moved.day_index, moved.on_date) == (4, None)


class TestMovingOneDateOfARepeatingClass:
    """A weekly class dragged on one date must not drag the whole series with it."""

    WEDNESDAY = MONDAY + timedelta(days=2)
    NEXT_WEDNESDAY = WEDNESDAY + timedelta(days=7)

    def ids_on(self, storage, group, day):
        return [l.id for l in storage.lessons_for_group(group.id) if l.occurs_on(day)]

    def test_only_this_date_moves(self, window, world, monkeypatch):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        monkeypatch.setattr(window, "_ask_move_scope", lambda lesson, day: "one")

        friday = MONDAY + timedelta(days=4)
        window._lesson_dropped(template.id, friday, "11:00", self.WEDNESDAY)

        assert template.id not in self.ids_on(storage, group, self.WEDNESDAY)
        assert template.id in self.ids_on(storage, group, self.NEXT_WEDNESDAY), "the series stays"
        copy_id = next(i for i in self.ids_on(storage, group, friday) if i != template.id)
        copy = storage.lesson(copy_id)
        assert (copy.on_date, copy.start_time, copy.subject, copy.url) == (
            friday, "11:00", template.subject, template.url,
        )
        assert copy.group_names == template.group_names
        assert not copy.repeats, "the moved date is a one-off"
        assert storage.lesson(template.id).day_index == 2, "the weekly template did not move"

    def test_the_whole_series_moves_when_asked(self, window, world, monkeypatch):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        monkeypatch.setattr(window, "_ask_move_scope", lambda lesson, day: "all")

        window._lesson_dropped(template.id, MONDAY + timedelta(days=4), "11:00", self.WEDNESDAY)

        moved = storage.lesson(template.id)
        assert (moved.day_index, moved.start_time, moved.skip_dates) == (4, "11:00", frozenset())

    def test_cancelling_changes_nothing(self, window, world, monkeypatch):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        before = len(storage.lessons_for_group(group.id))
        monkeypatch.setattr(window, "_ask_move_scope", lambda lesson, day: None)

        window._lesson_dropped(template.id, MONDAY + timedelta(days=4), "11:00", self.WEDNESDAY)

        assert len(storage.lessons_for_group(group.id)) == before
        after = storage.lesson(template.id)
        assert (after.day_index, after.start_time, after.skip_dates) == (2, "09:30", frozenset())

    def test_a_one_off_is_moved_without_asking(self, window, world, monkeypatch):
        storage, group = world
        stored = storage.lesson(storage.add_lesson(dated(MONDAY + timedelta(days=7)), [group.id]))
        monkeypatch.setattr(
            window, "_ask_move_scope", lambda *a: pytest.fail("a one-off has no series to ask about")
        )
        target = MONDAY + timedelta(days=8)

        window._lesson_dropped(stored.id, target, "11:00", stored.on_date)

        assert storage.lesson(stored.id).on_date == target

    def test_a_dated_series_can_lose_one_date_too(self, window, world, monkeypatch):
        storage, group = world
        series = storage.lesson(
            storage.add_lesson(
                dated(MONDAY, repeat_until=MONDAY + timedelta(days=21)), [group.id]
            )
        )
        monkeypatch.setattr(window, "_ask_move_scope", lambda lesson, day: "one")
        second = MONDAY + timedelta(days=7)

        window._lesson_dropped(series.id, MONDAY + timedelta(days=9), "13:00", second)

        assert not storage.lesson(series.id).occurs_on(second)
        assert storage.lesson(series.id).occurs_on(MONDAY)
        assert storage.lesson(series.id).occurs_on(MONDAY + timedelta(days=14))

    def test_the_skipped_date_survives_a_reload_and_a_reimport(self, window, world, monkeypatch):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        monkeypatch.setattr(window, "_ask_move_scope", lambda lesson, day: "one")
        window._lesson_dropped(template.id, MONDAY + timedelta(days=4), "11:00", self.WEDNESDAY)

        assert storage.lesson(template.id).skip_dates == frozenset({self.WEDNESDAY})
        assert window.current_lessons() != [] and template.id not in [
            l.id for l in window.current_lessons()
        ]

    def test_the_scope_question_names_the_date_and_the_choices(self, window, qapp, monkeypatch):
        from PySide6.QtWidgets import QMessageBox

        seen = {}

        def fake_exec(box):
            seen["text"] = box.text()
            seen["buttons"] = [b.text() for b in box.buttons()]
            return 0

        monkeypatch.setattr(QMessageBox, "exec", fake_exec)
        lesson = window.storage.lessons_for_group(window.storage.settings().selected_group_id)[0]
        assert window._ask_move_scope(lesson, self.WEDNESDAY) is None  # nothing was clicked
        assert "4 березня" in seen["text"] and lesson.subject in seen["text"]
        assert set(seen["buttons"]) == {"Лише цю дату", "Всю серію", "Скасувати"}  # order is the platform's


class TestMonthDropMovesTheDate:
    def test_a_class_dropped_on_another_day_keeps_its_time(self, window, world, monkeypatch):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]  # Wednesdays, 09:30
        wednesday = MONDAY + timedelta(days=2)
        monkeypatch.setattr(window, "_ask_move_scope", lambda lesson, day: "one")

        window._month_dropped(template.id, MONDAY + timedelta(days=10), wednesday)  # the Thursday after

        copy = next(
            l for l in storage.lessons_for_group(group.id) if l.on_date == MONDAY + timedelta(days=10)
        )
        assert copy.start_time == "09:30" and copy.end_time == "10:50"
        assert not storage.lesson(template.id).occurs_on(wednesday)

    def test_one_date_can_move_to_the_same_weekday_of_another_week(self, window, world, monkeypatch):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        wednesday = MONDAY + timedelta(days=2)
        later = wednesday + timedelta(days=14)
        monkeypatch.setattr(window, "_ask_move_scope", lambda lesson, day: "one")

        window._month_dropped(template.id, later, wednesday)

        assert not storage.lesson(template.id).occurs_on(wednesday)
        assert any(l.on_date == later for l in storage.lessons_for_group(group.id))

    def test_the_whole_series_can_follow(self, window, world, monkeypatch):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        monkeypatch.setattr(window, "_ask_move_scope", lambda lesson, day: "all")

        window._month_dropped(template.id, MONDAY + timedelta(days=10), MONDAY + timedelta(days=2))

        assert storage.lesson(template.id).day_index == 3  # Thursday

    def test_dropping_a_class_back_on_its_own_day_does_nothing(self, window, world, monkeypatch):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        wednesday = MONDAY + timedelta(days=2)
        monkeypatch.setattr(
            window, "_ask_move_scope", lambda *a: pytest.fail("nothing moved, nothing to ask")
        )
        window._month_dropped(template.id, wednesday, wednesday)
        assert len(storage.lessons_for_group(group.id)) == 1
