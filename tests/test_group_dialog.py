"""Switching to another course and group without reading the file again."""

from __future__ import annotations

import pytest
from PySide6.QtCore import Qt
from PySide6.QtWidgets import QLabel, QPushButton

from autopara.core.scheduler import Scheduler
from autopara.core.storage import Storage
from autopara.importer.schedule_parser import ParsedCourse, ParsedGroup, ParsedLesson
from autopara.ui.group_dialog import GroupDialog

URL = "https://zoom.us/j/1"


def parsed(subject: str, groups: list[str]) -> ParsedLesson:
    return ParsedLesson(
        day_index=2, pair=2, start_time="09:30", end_time="10:50", subject=subject,
        teacher="", url=URL, provider="zoom", group_names=groups,
    )


@pytest.fixture
def world(tmp_path):
    """Two courses: the first with two groups (one lesson each), the second with one and no lessons."""
    storage = Storage(tmp_path / "groups.db")
    storage.import_courses(
        [
            ParsedCourse(
                ordinal=1,
                name="I курс",
                groups=[ParsedGroup("11 група", "", 3, 3), ParsedGroup("12 група", "Дошк.", 4, 4)],
                lessons=[parsed("Для 11", ["11 група"]), parsed("Для 12", ["12 група"])],
            ),
            ParsedCourse(
                ordinal=2, name="II курс", groups=[ParsedGroup("21 група", "", 3, 3)], lessons=[]
            ),
        ]
    )
    first = storage.courses()[0]
    group = storage.groups(first.id)[0]
    storage.set_setting("selected_course_id", first.id)
    storage.set_setting("selected_group_id", group.id)
    yield storage
    storage.close()


def group_names(dialog):
    return [dialog.group_list.item(i).text() for i in range(dialog.group_list.count())]


class TestDialog:
    def test_it_opens_on_the_current_course_and_group(self, world, qapp):
        dialog = GroupDialog(world)
        assert dialog.course_combo.currentText() == "I курс"
        assert dialog.group_list.currentItem().text().startswith("11 група")

    def test_every_imported_course_is_offered(self, world, qapp):
        dialog = GroupDialog(world)
        assert [dialog.course_combo.itemText(i) for i in range(dialog.course_combo.count())] == [
            "I курс", "II курс",
        ]

    def test_choosing_a_course_lists_its_groups_with_their_size(self, world, qapp):
        dialog = GroupDialog(world)
        assert any("1 пара" in name for name in group_names(dialog))
        dialog.course_combo.setCurrentIndex(1)
        assert group_names(dialog) == ["21 група  ·  немає пар"]

    def test_a_groups_speciality_is_shown(self, world, qapp):
        assert "Дошк." in group_names(GroupDialog(world))[1]

    def test_choosing_stores_the_course_and_the_group(self, world, qapp):
        dialog = GroupDialog(world)
        dialog.group_list.setCurrentRow(1)
        dialog._accept()
        settings = world.settings()
        group = world.group(settings.selected_group_id)
        assert group.name == "12 група" and settings.selected_course_id == group.course_id

    def test_choosing_in_another_course(self, world, qapp):
        dialog = GroupDialog(world)
        dialog.course_combo.setCurrentIndex(1)
        dialog._accept()
        assert world.group(world.settings().selected_group_id).name == "21 група"

    def test_cancelling_changes_nothing(self, world, qapp):
        before = world.settings().selected_group_id
        dialog = GroupDialog(world)
        dialog.course_combo.setCurrentIndex(1)
        dialog.reject()
        assert world.settings().selected_group_id == before

    def test_nothing_imported_means_nothing_to_choose(self, tmp_path, qapp):
        empty = Storage(tmp_path / "empty.db")
        dialog = GroupDialog(empty)
        assert not dialog.choose_button.isEnabled()
        empty.close()

    def test_it_is_all_in_ukrainian(self, world, qapp):
        dialog = GroupDialog(world)
        for widget in dialog.findChildren(QLabel) + dialog.findChildren(QPushButton):
            for word in widget.text().replace("…", " ").split():
                assert not (word.isascii() and word.isalpha() and len(word) > 2), word


class TestOnTheRail:
    @pytest.fixture
    def window(self, world, qapp):
        from autopara.ui.main_window import MainWindow

        win = MainWindow(world, Scheduler(world))
        win.reload()
        return win

    def test_the_button_is_an_icon_with_a_tooltip_and_no_words(self, window):
        button = window.group_button
        assert button.text() == "" and not button.icon().isNull()
        assert button.toolTip() == "Обрати інший курс і групу…"

    def test_the_rail_still_carries_no_text(self, window):
        pill = window.add_button.parent()
        for button in pill.findChildren(QPushButton):
            assert button.text() == ""

    def test_it_is_disabled_until_something_is_imported(self, tmp_path, qapp):
        from autopara.ui.main_window import MainWindow

        empty = Storage(tmp_path / "none.db")
        win = MainWindow(empty, Scheduler(empty))
        win.reload()
        assert not win.group_button.isEnabled()
        empty.close()

    def test_switching_group_changes_what_the_calendar_shows(self, window, world):
        subjects = lambda: sorted(l.subject for l in window.group_lessons())  # noqa: E731
        assert subjects() == ["Для 11"]
        dialog = GroupDialog(world)
        dialog.group_list.setCurrentRow(1)
        dialog._accept()
        window.reload()
        assert subjects() == ["Для 12"]
