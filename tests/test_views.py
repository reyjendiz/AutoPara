"""Day, week and month: the segmented control, the month card, and how each view steps in time.

Self-contained like the other new test files: its own timetable, no schedule document, and the
browser is never reached.
"""

from __future__ import annotations

from datetime import date, timedelta

import pytest
from PySide6.QtCore import QEvent, QPoint, QPointF
from PySide6.QtGui import QEnterEvent
from PySide6.QtWidgets import QLabel, QPushButton

from autopara.core.models import (
    STATUS_SKIPPED,
    VIEW_DAY,
    VIEW_MONTH,
    VIEW_WEEK,
    Lesson,
)
from autopara.core.scheduler import Scheduler
from autopara.core.storage import Storage
from autopara.importer.schedule_parser import ParsedCourse, ParsedGroup, ParsedLesson
from autopara.ui.class_card import ClassCard
from autopara.ui.week_grid import GUTTER_WIDTH, HOUR_HEIGHT, GridCell
from autopara.ui.month_view import (
    MAX_CHIPS,
    MonthChip,
    MonthView,
    first_of,
    month_grid,
    pairs_word,
    shift_month,
)

# 2026-03-02 is a Monday; March 2026 starts on a Sunday.
MONDAY = date(2026, 3, 2)
URL = "https://zoom.us/j/1"


def lesson(lesson_id: int, day: date | None = None, **extra) -> Lesson:
    base = dict(
        id=lesson_id,
        course_id=1,
        day_index=(day or MONDAY).weekday(),
        pair=1,
        start_time="08:00",
        end_time="09:20",
        subject=f"Пара {lesson_id}",
        url=URL,
        provider="zoom",
        on_date=day,
    )
    base.update(extra)
    return Lesson(**base)


@pytest.fixture
def world(tmp_path):
    storage = Storage(tmp_path / "views.db")
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
                        subject="Рядок розкладу",
                        teacher="",
                        url=URL,
                        provider="zoom",
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
def window(world, qapp):
    from autopara.ui.main_window import MainWindow

    storage, _ = world
    win = MainWindow(storage, Scheduler(storage))
    win.anchor = MONDAY
    win.reload()
    return win


class TestMonthMaths:
    def test_a_month_is_six_whole_weeks_starting_on_a_monday(self):
        days = month_grid(date(2026, 3, 1))
        assert len(days) == 42
        assert days[0] == date(2026, 2, 23) and days[0].weekday() == 0
        assert date(2026, 3, 31) in days

    def test_a_month_that_starts_on_a_monday_has_no_leading_days(self):
        assert month_grid(date(2026, 6, 1))[0] == date(2026, 6, 1)

    def test_stepping_clamps_to_a_shorter_month(self):
        assert shift_month(date(2026, 1, 31), 1) == date(2026, 2, 28)
        assert shift_month(date(2026, 3, 31), -1) == date(2026, 2, 28)
        assert shift_month(date(2026, 12, 15), 1) == date(2027, 1, 15)
        assert shift_month(date(2026, 1, 15), -1) == date(2025, 12, 15)

    def test_first_of(self):
        assert first_of(date(2026, 10, 17)) == date(2026, 10, 1)

    def test_plural_of_para(self):
        assert [pairs_word(n) for n in (1, 2, 4, 5, 11, 21, 22)] == [
            "пара", "пари", "пари", "пар", "пар", "пара", "пари",
        ]


class TestMonthView:
    def render(self, qapp, lessons, statuses=None, first=date(2026, 3, 1), today=MONDAY, height=700):
        view = MonthView()
        view.resize(1100, height)
        view.render_month(first, lessons, statuses, today)
        view.show()
        qapp.processEvents()
        return view

    def tile(self, view, day):
        return next(t for t in view.tiles if t.day == day)

    def chips(self, tile):
        return tile.findChildren(MonthChip)

    def test_every_day_of_six_weeks_has_a_tile(self, qapp):
        view = self.render(qapp, [])
        assert len(view.tiles) == 42

    def test_a_one_off_shows_on_its_own_day_only(self, qapp):
        day = date(2026, 3, 11)
        view = self.render(qapp, [lesson(1, day)])
        assert len(self.chips(self.tile(view, day))) == 1
        assert sum(len(self.chips(t)) for t in view.tiles) == 1

    def test_a_series_shows_on_each_of_its_dates(self, qapp):
        view = self.render(qapp, [lesson(1, MONDAY, repeat_until=MONDAY + timedelta(days=14))])
        dated = [t.day for t in view.tiles if self.chips(t)]
        assert dated == [MONDAY, MONDAY + timedelta(days=7), MONDAY + timedelta(days=14)]

    def test_a_weekly_class_shows_on_every_matching_weekday(self, qapp):
        view = self.render(qapp, [lesson(1, None, day_index=2)])
        shown = [t.day for t in view.tiles if self.chips(t)]
        assert shown and all(day.weekday() == 2 for day in shown)
        assert len(shown) == 6  # six weeks on screen

    def test_a_crowded_day_counts_what_it_does_not_draw(self, qapp):
        crowd = [lesson(n, MONDAY, start_time=f"{8 + n:02d}:00", end_time=f"{9 + n:02d}:00")
                 for n in range(1, MAX_CHIPS + 3)]
        view = self.render(qapp, crowd, height=1000)  # tall enough for the most a tile draws
        tile = self.tile(view, MONDAY)
        shown = [c for c in tile.chips if c.isVisible()]
        assert len(shown) == MAX_CHIPS
        more = [l.text() for l in tile.findChildren(QLabel) if l.objectName() == "MonthMore"]
        assert more == ["ще 2"]

    def test_a_short_tile_draws_fewer_chips_and_counts_the_rest(self, qapp):
        crowd = [lesson(n, MONDAY, start_time=f"{8 + n:02d}:00", end_time=f"{9 + n:02d}:00")
                 for n in range(1, MAX_CHIPS + 3)]
        view = self.render(qapp, crowd, height=520)
        tile = self.tile(view, MONDAY)
        shown = [c for c in tile.chips if c.isVisible()]
        assert len(shown) < MAX_CHIPS
        more = next(l for l in tile.findChildren(QLabel) if l.objectName() == "MonthMore")
        assert more.isVisible() and more.text() == f"ще {len(crowd) - len(shown)}"

    def test_the_chips_never_overlap_the_date(self, qapp):
        """The bug a crowded Monday showed: chips squeezed upward until they sat on the date."""
        from autopara.ui.month_view import DATE_CIRCLE

        crowd = [lesson(n, MONDAY, start_time=f"{8 + n:02d}:00", end_time=f"{9 + n:02d}:00")
                 for n in range(1, MAX_CHIPS + 3)]
        for height in (420, 520, 700, 1000):
            view = self.render(qapp, crowd, height=height)
            tile = self.tile(view, MONDAY)
            date_bottom = DATE_CIRCLE + 6
            for chip in tile.chips:
                if chip.isVisible():
                    assert chip.geometry().top() >= date_bottom, f"overlap at height {height}"
                    assert chip.geometry().bottom() <= tile.height(), f"clipped at height {height}"

    def test_a_tile_with_room_hides_nothing(self, qapp):
        view = self.render(qapp, [lesson(1, MONDAY)], height=1000)
        tile = self.tile(view, MONDAY)
        assert all(c.isVisible() for c in tile.chips)
        assert not any(l.isVisible() for l in tile.findChildren(QLabel) if l.objectName() == "MonthMore")

    def test_chips_are_in_time_order(self, qapp):
        late = lesson(1, MONDAY, start_time="15:00", end_time="16:00")
        early = lesson(2, MONDAY, start_time="08:00", end_time="09:00")
        view = self.render(qapp, [late, early])
        tile = self.tile(view, MONDAY)
        assert [c.lesson.id for c in self.chips(tile)] == [2, 1]

    def test_a_verdict_is_spelled_on_the_chip_not_only_coloured(self, qapp):
        item = lesson(1, MONDAY)
        view = self.render(qapp, [item], {(1, MONDAY.isoformat()): STATUS_SKIPPED})
        chip = self.chips(self.tile(view, MONDAY))[0]
        assert "✕" in chip.label._full

    def test_clicking_a_day_selects_it(self, qapp):
        view = self.render(qapp, [])
        picked: list[date] = []
        view.day_selected.connect(picked.append)
        tile = self.tile(view, date(2026, 3, 18))
        tile.selected.emit(tile.day)
        assert picked == [date(2026, 3, 18)]

    def test_the_plus_asks_for_a_new_class_on_that_day(self, qapp):
        view = self.render(qapp, [])
        asked: list[date] = []
        view.add_requested.connect(asked.append)
        tile = self.tile(view, date(2026, 3, 18))
        tile.add_button.click()
        assert asked == [date(2026, 3, 18)]

    def test_the_plus_appears_only_under_the_pointer(self, qapp):
        view = self.render(qapp, [])
        tile = self.tile(view, MONDAY)
        assert tile.add_button.isHidden()
        point = QPointF(4, 4)
        tile.enterEvent(QEnterEvent(point, point, point))
        assert not tile.add_button.isHidden()
        tile.leaveEvent(QEvent(QEvent.Leave))
        assert tile.add_button.isHidden()

    def test_today_and_other_months_are_marked(self, qapp):
        view = self.render(qapp, [], first=date(2026, 3, 1), today=MONDAY)
        assert self.tile(view, MONDAY).property("today") == "true"
        assert self.tile(view, date(2026, 2, 23)).property("other") == "true"
        assert self.tile(view, date(2026, 3, 10)).property("other") == "false"

    def test_rerendering_never_detaches_a_widget(self, qapp):
        """Reparenting a live widget to None makes it a top-level window; it flashed on screen."""
        view = self.render(qapp, [lesson(1, MONDAY)])
        before = list(view.tiles)
        view.render_month(date(2026, 3, 1), [lesson(1, MONDAY)], None, MONDAY)
        qapp.sendPostedEvents(None, QEvent.DeferredDelete)
        for tile in before:
            try:
                assert tile.parent() is not None, "a stale tile must not become a window"
            except RuntimeError:
                pass  # already deleted: also fine
        assert len(view.tiles) == 42


class TestSegmentedControl:
    def test_it_offers_the_three_views_in_ukrainian(self, window):
        assert [b.text() for b in window.nav.view_buttons.values()] == [
            "День", "Тиждень", "Місяць",
        ]

    def test_the_week_is_the_default(self, window):
        assert window.view_mode() == VIEW_WEEK
        assert window.nav.view_buttons[VIEW_WEEK].isChecked()

    def test_choosing_a_view_is_remembered_in_storage(self, window, world):
        storage, _ = world
        window.nav.view_buttons[VIEW_MONTH].click()
        assert storage.settings().view_mode == VIEW_MONTH
        assert window.nav.view_buttons[VIEW_MONTH].isChecked()

    def test_a_garbled_setting_falls_back_to_the_week(self, world):
        storage, _ = world
        storage.set_setting("view_mode", "fortnight")
        assert storage.settings().view_mode == VIEW_WEEK

    def test_only_the_surface_a_view_needs_is_shown(self, window):
        window.set_view_mode(VIEW_MONTH)
        assert window.month.isVisibleTo(window) and window.grid.isHidden()
        window.set_view_mode(VIEW_DAY)
        assert window.grid.isVisibleTo(window) and window.month.isHidden()
        window.set_view_mode(VIEW_WEEK)
        assert window.grid.isVisibleTo(window) and window.month.isHidden()


class TestSteppingInTime:
    def test_the_day_view_steps_by_a_day(self, window):
        window.set_view_mode(VIEW_DAY)
        window.nav.next_button.click()
        assert window.anchor == MONDAY + timedelta(days=1)
        window.nav.previous_button.click()
        window.nav.previous_button.click()
        assert window.anchor == MONDAY - timedelta(days=1)

    def test_the_week_view_steps_by_a_week(self, window):
        window.nav.next_button.click()
        assert window.anchor == MONDAY + timedelta(days=7)

    def test_the_month_view_steps_by_a_month(self, window):
        window.anchor = date(2026, 1, 31)
        window.set_view_mode(VIEW_MONTH)
        window.nav.next_button.click()
        assert window.anchor == date(2026, 2, 28)

    def test_the_arrows_say_what_they_step_by(self, window):
        window.set_view_mode(VIEW_DAY)
        assert window.nav.next_button.toolTip() == "Наступний день"
        window.set_view_mode(VIEW_MONTH)
        assert window.nav.previous_button.toolTip() == "Попередній місяць"
        window.set_view_mode(VIEW_WEEK)
        assert window.nav.next_button.toolTip() == "Наступний тиждень"

    def test_each_view_titles_its_period(self, window):
        window.set_view_mode(VIEW_DAY)
        assert (window.nav.title.text(), window.nav.sub.text()) == ("Понеділок, 2 березня", "2026")
        window.set_view_mode(VIEW_MONTH)
        assert (window.nav.title.text(), window.nav.sub.text()) == ("Березень", "2026")
        window.set_view_mode(VIEW_WEEK)
        assert (window.nav.title.text(), window.nav.sub.text()) == ("2–8 березня", "2026")

    def test_today_returns_from_any_view(self, window):
        window.set_view_mode(VIEW_MONTH)
        window.nav.today_button.click()
        assert window.anchor == date.today()


class TestDayView:
    def test_it_shows_one_column_with_that_days_classes(self, window, world):
        storage, group = world
        window.anchor = MONDAY + timedelta(days=2)  # a Wednesday: the template class
        storage.add_lesson(lesson(0, MONDAY + timedelta(days=9), subject="Не сьогодні"), [group.id])
        window.set_view_mode(VIEW_DAY)
        cards = [c for c in window.grid.findChildren(ClassCard) if not c.parent().isHidden()]
        assert [c.lesson.subject for c in cards] == ["Рядок розкладу"]
        assert window.grid._dates == [window.anchor]

    def test_the_day_is_named_over_the_gutter_not_floating_in_the_wide_column(self, window):
        window.set_view_mode(VIEW_DAY)
        window.show()
        names = [l for l in window.grid.findChildren(QLabel)
                 if l.objectName() == "DayName" and l.isVisibleTo(window)]
        assert [l.text() for l in names] == ["Пн"]
        left = names[0].mapTo(window.grid._canvas, names[0].rect().topLeft()).x()
        assert left < GUTTER_WIDTH, "the weekday sits over the time gutter"

    def test_it_is_as_wide_as_the_work_area_and_as_tall_as_a_week_hour(self, window, qapp):
        window.set_view_mode(VIEW_DAY)
        window.show()
        qapp.processEvents()
        cells = [c for c in window.grid.findChildren(GridCell) if c.isVisibleTo(window)]
        assert {c.height() for c in cells} == {HOUR_HEIGHT}
        assert cells[0].width() >= window.grid.viewport().width() - GUTTER_WIDTH - 2

    def test_a_date_with_nothing_on_it_is_still_a_day(self, window):
        window.anchor = MONDAY + timedelta(days=1)
        window.set_view_mode(VIEW_DAY)
        assert window.grid.isVisibleTo(window)
        assert not [c for c in window.grid.findChildren(ClassCard) if not c.parent().isHidden()]

    def test_the_verdict_of_that_date_is_shown(self, window, world):
        storage, group = world
        template = storage.lessons_for_group(group.id)[0]
        day = MONDAY + timedelta(days=2)
        window.scheduler.mark(template.id, STATUS_SKIPPED, day)
        window.anchor = day
        window.set_view_mode(VIEW_DAY)
        card = next(c for c in window.grid.findChildren(ClassCard) if not c.parent().isHidden())
        assert card.state == "skipped"


class TestFromMonthToDay:
    def test_clicking_a_day_in_the_month_opens_that_day(self, window):
        window.set_view_mode(VIEW_MONTH)
        target = date(2026, 3, 18)
        tile = next(t for t in window.month.tiles if t.day == target)
        tile.selected.emit(tile.day)
        assert window.view_mode() == VIEW_DAY
        assert window.anchor == target

    def test_the_plus_in_the_month_opens_the_dialog_on_that_day(self, window, monkeypatch):
        seen: list[tuple] = []
        monkeypatch.setattr(
            window, "_edit_lesson", lambda lesson, day=None, start_time=None: seen.append((lesson, day))
        )
        window.set_view_mode(VIEW_MONTH)
        tile = next(t for t in window.month.tiles if t.day == date(2026, 3, 18))
        tile.add_button.click()
        assert seen == [(None, date(2026, 3, 18))]


class TestUkrainianViews:
    def test_no_latin_words_on_any_view(self, window):
        allowed = {"AutoPara", "Zoom", "Meet", "docx"}
        latin = set()
        for mode in (VIEW_DAY, VIEW_WEEK, VIEW_MONTH):
            window.set_view_mode(mode)
            for widget in window.findChildren(QLabel) + window.findChildren(QPushButton):
                text = getattr(widget, "text", lambda: "")()
                for word in text.replace("…", " ").split():
                    word = word.strip("·—–-()")
                    if word.isascii() and word.isalpha() and len(word) > 2 and word not in allowed:
                        latin.add(word)
        assert not latin, latin


class _FakeDrop:
    """The few things a tile asks of a drag event."""

    def __init__(self, mime):
        self._mime = mime

    def mimeData(self):  # noqa: N802 - Qt naming
        return self._mime

    def acceptProposedAction(self):  # noqa: N802 - Qt naming
        pass


class TestMonthDragAndDrop:
    DAY = date(2026, 3, 11)  # a Wednesday

    def render(self, qapp, lessons):
        view = MonthView()
        view.resize(1100, 700)
        view.render_month(date(2026, 3, 1), lessons, None, MONDAY)
        view.show()
        qapp.processEvents()
        return view

    def tile(self, view, day):
        return next(t for t in view.tiles if t.day == day)

    @staticmethod
    def mouse(kind, point, buttons):
        from PySide6.QtCore import QEvent, Qt
        from PySide6.QtGui import QMouseEvent

        types = {"press": QEvent.MouseButtonPress, "move": QEvent.MouseMove,
                 "release": QEvent.MouseButtonRelease}
        button = Qt.LeftButton if kind != "move" else Qt.NoButton
        return QMouseEvent(types[kind], QPointF(point), QPointF(point), button, buttons, Qt.NoModifier)

    def test_dragging_a_chip_carries_its_class_and_its_date(self, qapp, monkeypatch):
        from PySide6.QtCore import Qt
        from PySide6.QtGui import QDrag

        from autopara.ui.class_card import LESSON_MIME, read_drag_payload

        view = self.render(qapp, [lesson(7, self.DAY)])
        tile = self.tile(view, self.DAY)
        chip = tile.chips[0]
        grabbed = []
        monkeypatch.setattr(QDrag, "exec", lambda drag, *a: grabbed.append(drag.mimeData()) or 0)

        start = chip.geometry().center()
        tile.mousePressEvent(self.mouse("press", start, Qt.LeftButton))
        tile.mouseMoveEvent(self.mouse("move", start + QPoint(40, 40), Qt.LeftButton))

        assert len(grabbed) == 1
        assert read_drag_payload(grabbed[0].data(LESSON_MIME)) == (7, self.DAY)

    def test_a_press_on_empty_space_does_not_start_a_drag(self, qapp, monkeypatch):
        from PySide6.QtCore import Qt
        from PySide6.QtGui import QDrag

        view = self.render(qapp, [lesson(7, self.DAY)])
        tile = self.tile(view, self.DAY)
        grabbed = []
        monkeypatch.setattr(QDrag, "exec", lambda drag, *a: grabbed.append(1) or 0)

        empty = QPoint(tile.width() // 2, tile.height() - 4)
        tile.mousePressEvent(self.mouse("press", empty, Qt.LeftButton))
        tile.mouseMoveEvent(self.mouse("move", empty + QPoint(40, 0), Qt.LeftButton))
        assert grabbed == []

    def test_a_click_still_opens_the_day_but_a_drag_does_not(self, qapp, monkeypatch):
        from PySide6.QtCore import Qt
        from PySide6.QtGui import QDrag

        view = self.render(qapp, [lesson(7, self.DAY)])
        tile = self.tile(view, self.DAY)
        opened = []
        tile.selected.connect(opened.append)
        monkeypatch.setattr(QDrag, "exec", lambda drag, *a: 0)
        centre = tile.chips[0].geometry().center()

        tile.mousePressEvent(self.mouse("press", centre, Qt.LeftButton))
        tile.mouseReleaseEvent(self.mouse("release", centre, Qt.NoButton))
        assert opened == [self.DAY]

        tile.mousePressEvent(self.mouse("press", centre, Qt.LeftButton))
        tile.mouseMoveEvent(self.mouse("move", centre + QPoint(40, 40), Qt.LeftButton))
        tile.mouseReleaseEvent(self.mouse("release", centre, Qt.NoButton))
        assert opened == [self.DAY], "letting go after a drag is not a click"

    def test_dropping_on_a_day_reports_the_class_the_day_and_where_it_came_from(self, qapp):
        from PySide6.QtCore import QMimeData

        from autopara.ui.class_card import LESSON_MIME, drag_payload

        view = self.render(qapp, [lesson(7, self.DAY)])
        target = self.tile(view, self.DAY + timedelta(days=3))
        dropped = []
        view.lesson_dropped.connect(lambda *args: dropped.append(args))
        mime = QMimeData()
        mime.setData(LESSON_MIME, drag_payload(7, self.DAY))

        target.dropEvent(_FakeDrop(mime))

        assert dropped == [(7, self.DAY + timedelta(days=3), self.DAY)]

    def test_a_day_lights_up_while_a_class_is_over_it(self, qapp):
        from PySide6.QtCore import QMimeData

        from autopara.ui.class_card import LESSON_MIME, drag_payload

        view = self.render(qapp, [])
        tile = self.tile(view, self.DAY)
        mime = QMimeData()
        mime.setData(LESSON_MIME, drag_payload(7, MONDAY))

        tile.dragEnterEvent(_FakeDrop(mime))
        assert tile.property("dropping") == "true"
        tile.dropEvent(_FakeDrop(mime))
        assert tile.property("dropping") == "false"

    def test_something_that_is_not_a_class_is_ignored(self, qapp):
        from PySide6.QtCore import QMimeData

        view = self.render(qapp, [])
        tile = self.tile(view, self.DAY)
        dropped = []
        view.lesson_dropped.connect(lambda *args: dropped.append(args))
        mime = QMimeData()
        mime.setText("hello")
        tile.dragEnterEvent(_FakeDrop(mime))
        tile.dropEvent(_FakeDrop(mime))
        assert dropped == [] and tile.property("dropping") == "false"
