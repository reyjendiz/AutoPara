"""Search and filters in the window: what the three views show, the count, the group switch, the jump."""

from __future__ import annotations

from datetime import date, timedelta

import pytest

from autopara.core.filters import alphabetical
from autopara.core.models import VIEW_DAY, VIEW_MONTH, VIEW_WEEK, Lesson
from autopara.core.scheduler import Scheduler
from autopara.core.storage import Storage
from autopara.importer.schedule_parser import ParsedCourse, ParsedGroup, ParsedLesson
from autopara.ui.class_card import ClassCard
from autopara.ui.month_view import MonthChip

MONDAY = date(2026, 3, 2)  # a Monday


def parsed(day, pair, subject, teacher, groups) -> ParsedLesson:
    start = {1: "08:00", 2: "09:30", 3: "11:20"}[pair]
    hour, minute = map(int, start.split(":"))
    end = f"{(hour * 60 + minute + 80) // 60:02d}:{(hour * 60 + minute + 80) % 60:02d}"
    return ParsedLesson(
        day_index=day, pair=pair, start_time=start, end_time=end, subject=subject,
        teacher=teacher, url="https://zoom.us/j/1", provider="zoom", group_names=groups,
    )


@pytest.fixture
def world(tmp_path):
    storage = Storage(tmp_path / "filters.db")
    both = ["11 група", "12 група"]
    storage.import_courses(
        [
            ParsedCourse(
                ordinal=1,
                name="I курс",
                groups=[ParsedGroup("11 група", "", 3, 3), ParsedGroup("12 група", "", 4, 4)],
                lessons=[
                    parsed(0, 1, "Загальна педагогіка", "(Роман Н.М.)", both),
                    parsed(1, 1, "Історія України", "(Іваненко І.І.)", ["11 група"]),
                    parsed(3, 2, "Музичне виховання", "(Лисенко Д.А.)", ["12 група"]),
                ],
            )
        ]
    )
    course = storage.courses()[0]
    first = storage.groups(course.id)[0]
    storage.set_setting("selected_course_id", course.id)
    storage.set_setting("selected_group_id", first.id)
    yield storage, first
    storage.close()


@pytest.fixture
def window(world, qapp):
    from autopara.ui.main_window import MainWindow

    storage, _ = world
    win = MainWindow(storage, Scheduler(storage))
    win.resize(1240, 800)
    win.anchor = MONDAY
    win.reload()
    win.show()
    qapp.processEvents()
    yield win
    win.close()


def week_subjects(window) -> list[str]:
    from PySide6.QtWidgets import QApplication

    QApplication.processEvents()  # cards built a moment ago are only shown once events run
    return sorted(
        (
            c.lesson.subject for c in window.grid.findChildren(ClassCard)
            if c.parent() is not None and not c.parent().isHidden()
        ),
        key=alphabetical,  # code points would put "І" before "З"
    )


class TestSearch:
    def test_everything_is_shown_until_something_is_typed(self, window):
        assert week_subjects(window) == ["Загальна педагогіка", "Історія України"]
        assert window.filterbar.summary.isHidden()

    def test_typing_hides_what_does_not_match(self, window):
        window.filterbar.search.setText("історія")
        assert week_subjects(window) == ["Історія України"]

    def test_the_teacher_is_searched_too(self, window):
        window.filterbar.search.setText("роман")
        assert week_subjects(window) == ["Загальна педагогіка"]

    def test_the_count_says_how_much_was_found(self, window):
        window.filterbar.search.setText("історія")
        assert window.filterbar.summary.text() == "Знайдено 1 з 2"

    def test_nothing_found_keeps_the_calendar_and_says_so(self, window):
        window.filterbar.search.setText("хімія")
        assert week_subjects(window) == []
        assert not window.grid.isHidden(), "the grid stays, or there would be no way back"
        assert window.filterbar.summary.text() == "Нічого не знайдено"
        assert not window.filterbar.isHidden()

    def test_the_day_and_month_views_are_filtered_too(self, window, qapp):
        window.filterbar.search.setText("історія")
        window.storage.set_setting("view_mode", VIEW_MONTH)
        window.reload()
        chips = window.month.findChildren(MonthChip)
        assert chips and {c.lesson.subject for c in chips} == {"Історія України"}

        window.storage.set_setting("view_mode", VIEW_DAY)
        window.anchor = MONDAY + timedelta(days=1)  # the Tuesday it is on
        window.reload()
        assert week_subjects(window) == ["Історія України"]
        window.anchor = MONDAY  # a Monday: pedagogy is filtered out
        window.reload()
        assert week_subjects(window) == []

    def test_reset_brings_everything_back(self, window):
        window.filterbar.search.setText("історія")
        assert not window.filterbar.reset_button.isHidden()
        window.filterbar.reset_button.click()
        assert window.filterbar.search.text() == ""
        assert week_subjects(window) == ["Загальна педагогіка", "Історія України"]
        assert window.filterbar.reset_button.isHidden()

    def test_escape_clears_the_box(self, window, qapp):
        from PySide6.QtCore import Qt
        from PySide6.QtTest import QTest

        window.filterbar.search.setText("історія")
        QTest.keyClick(window.filterbar.search, Qt.Key_Escape)
        assert window.filterbar.search.text() == ""


class TestSubjectFilter:
    def test_the_drop_down_lists_the_group_subjects_alphabetically(self, window):
        box = window.filterbar.subject
        assert [box.itemText(i) for i in range(box.count())] == [
            "Усі предмети", "Загальна педагогіка", "Історія України",
        ]

    def test_choosing_a_subject_shows_only_it(self, window):
        box = window.filterbar.subject
        box.setCurrentIndex(box.findData("Історія України"))
        assert week_subjects(window) == ["Історія України"]
        assert window.filterbar.summary.text() == "Знайдено 1 з 2"

    def test_the_choice_survives_a_reload_while_the_subject_exists(self, window):
        box = window.filterbar.subject
        box.setCurrentIndex(box.findData("Історія України"))
        window.reload()
        assert window.filterbar.subject.currentData() == "Історія України"


class TestGroupFilter:
    def test_it_lists_the_groups_of_the_course(self, window):
        box = window.filterbar.group
        assert [box.itemText(i) for i in range(box.count())] == ["11 група", "12 група"]
        assert not box.isHidden()

    def test_another_group_is_shown_without_changing_the_chosen_one(self, window, world):
        storage, first = world
        box = window.filterbar.group
        box.setCurrentIndex(box.findText("12 група"))

        window.anchor = MONDAY + timedelta(days=3)  # the Thursday: only group 12 has a class
        window.storage.set_setting("view_mode", VIEW_DAY)
        window.reload()
        assert week_subjects(window) == ["Музичне виховання"]
        assert storage.settings().selected_group_id == first.id, "what auto-opens is unchanged"

    def test_the_subjects_follow_the_group_on_screen(self, window):
        box = window.filterbar.group
        box.setCurrentIndex(box.findText("12 група"))
        subjects = window.filterbar.subject
        assert [subjects.itemText(i) for i in range(subjects.count())] == [
            "Усі предмети", "Загальна педагогіка", "Музичне виховання",
        ]

    def test_choosing_the_chosen_group_again_is_no_filter(self, window):
        box = window.filterbar.group
        box.setCurrentIndex(box.findText("12 група"))
        box.setCurrentIndex(box.findText("11 група"))
        assert window._view_group_id is None

    def test_a_new_choice_of_group_in_the_dialog_resets_the_view(self, window, world):
        storage, first = world
        box = window.filterbar.group
        box.setCurrentIndex(box.findText("12 група"))
        second = storage.groups(first.course_id)[1]
        storage.set_setting("selected_group_id", second.id)
        window.reload()
        assert window.filterbar.group_id() == second.id


class TestJumpToAMatch:
    def add_oneoff(self, storage, group, day, subject):
        return storage.add_lesson(
            Lesson(id=0, course_id=group.course_id, day_index=day.weekday(), pair=1,
                   start_time="08:00", end_time="09:20", subject=subject, on_date=day,
                   url="https://zoom.us/j/9", provider="zoom"),
            [group.id],
        )

    def test_enter_moves_the_calendar_to_the_next_week_with_a_match(self, window, world):
        storage, group = world
        far = MONDAY + timedelta(days=21)  # three weeks on
        self.add_oneoff(storage, group, far, "Семінар з методики")
        window.reload()
        window.filterbar.search.setText("семінар")

        window.filterbar.jump_requested.emit(1)

        assert window.monday() == far - timedelta(days=far.weekday())
        assert week_subjects(window) == ["Семінар з методики"]

    def test_shift_enter_goes_back(self, window, world):
        storage, group = world
        earlier = MONDAY - timedelta(days=14)
        self.add_oneoff(storage, group, earlier, "Семінар з методики")
        window.filterbar.search.setText("семінар")
        window.filterbar.jump_requested.emit(-1)
        assert window.monday() == earlier - timedelta(days=earlier.weekday())

    def test_no_further_match_says_so_and_stays_put(self, window):
        window.filterbar.search.setText("історія")
        window.filterbar.jump_requested.emit(1)  # it recurs weekly, so there is always a next one
        assert window.monday() != MONDAY
        window.anchor = MONDAY
        window.filterbar.search.setText("хімія")
        window.filterbar.jump_requested.emit(1)
        assert window.anchor == MONDAY
        assert window.filterbar.summary.text() == "Збігів більше немає"

    def test_jumping_without_a_filter_does_nothing(self, window):
        window.filterbar.jump_requested.emit(1)
        assert window.anchor == MONDAY

    def test_enter_in_the_box_asks_for_the_jump(self, window, qapp):
        from PySide6.QtCore import Qt
        from PySide6.QtTest import QTest

        asked = []
        window.filterbar.jump_requested.connect(asked.append)
        QTest.keyClick(window.filterbar.search, Qt.Key_Return)
        QTest.keyClick(window.filterbar.search, Qt.Key_Return, Qt.ShiftModifier)
        assert asked == [1, -1]


class TestTheBarItself:
    def test_it_is_hidden_with_the_calendar_when_nothing_is_imported(self, tmp_path, qapp):
        from autopara.ui.main_window import MainWindow

        storage = Storage(tmp_path / "empty.db")
        win = MainWindow(storage, Scheduler(storage))
        win.reload()
        assert win.filterbar.isHidden()
        storage.close()

    def test_a_filter_that_hides_a_whole_group_does_not_fall_back_to_the_empty_screen(self, window):
        window.filterbar.search.setText("хімія")
        assert window.landing.isHidden()

    def test_the_search_shortcut_focuses_the_box(self, window, qapp):
        window.filterbar.focus_search()
        assert window.filterbar.search.hasFocus() or window.filterbar.search.isVisible()
