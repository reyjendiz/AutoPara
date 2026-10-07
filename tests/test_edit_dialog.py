"""Adding a class for a date: the three kinds of repeat, and what each one stores.

Like the other new test files this builds its own timetable, so it says the same thing without the
schedule document.
"""

from __future__ import annotations

from datetime import date, timedelta

import pytest
from PySide6.QtCore import QDate
from PySide6.QtWidgets import QLabel, QPushButton

from autopara.core.models import Lesson
from autopara.core.storage import Storage
from autopara.importer.schedule_parser import ParsedCourse, ParsedGroup, ParsedLesson
from autopara.ui.edit_dialog import (
    REPEAT_ONCE,
    REPEAT_UNTIL,
    REPEAT_WEEKLY,
    EditDialog,
    times_word,
)

# 2026-03-02 is a Monday.
MONDAY = date(2026, 3, 2)
URL = "https://zoom.us/j/1"


@pytest.fixture
def world(tmp_path):
    storage = Storage(tmp_path / "dialog.db")
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
    yield storage, group
    storage.close()


def pick(dialog, mode):
    dialog.repeat_combo.setCurrentIndex(dialog.repeat_combo.findData(mode))


def fill(dialog, subject="Консультація"):
    dialog.subject_edit.setText(subject)
    dialog.url_edit.setText(URL)


def only_dated(storage, group):
    return [l for l in storage.lessons_for_group(group.id) if l.is_manual]


class TestNewClassOnADate:
    def test_a_date_gesture_opens_on_that_date_as_a_one_off(self, world, qapp):
        storage, group = world
        day = MONDAY + timedelta(days=16)
        dialog = EditDialog(storage, group.id, None, day=day, start_time="14:00")
        assert dialog.repeat_mode == REPEAT_ONCE
        assert dialog.date_edit.date() == QDate(day.year, day.month, day.day)
        assert dialog.start_edit.time().hour() == 14

    def test_saving_a_one_off_stores_its_date_and_weekday(self, world, qapp):
        storage, group = world
        day = MONDAY + timedelta(days=16)  # a Wednesday
        dialog = EditDialog(storage, group.id, None, day=day, start_time="14:00")
        fill(dialog)
        dialog._accept()

        (saved,) = only_dated(storage, group)
        assert (saved.on_date, saved.repeat_until, saved.day_index) == (day, None, day.weekday())
        assert saved.start_time == "14:00" and saved.is_manual

    def test_a_series_stores_both_ends(self, world, qapp):
        storage, group = world
        dialog = EditDialog(storage, group.id, None, day=MONDAY, start_time="09:30")
        fill(dialog)
        pick(dialog, REPEAT_UNTIL)
        end = MONDAY + timedelta(days=21)
        dialog.until_edit.setDate(QDate(end.year, end.month, end.day))
        dialog._accept()

        (saved,) = only_dated(storage, group)
        assert (saved.on_date, saved.repeat_until) == (MONDAY, end)

    def test_a_series_cannot_end_before_it_starts(self, world, qapp):
        storage, group = world
        dialog = EditDialog(storage, group.id, None, day=MONDAY, start_time="09:30")
        pick(dialog, REPEAT_UNTIL)
        earlier = MONDAY - timedelta(days=7)
        dialog.until_edit.setDate(QDate(earlier.year, earlier.month, earlier.day))
        assert dialog.until_edit.date() >= dialog.date_edit.date(), "the field refuses it"

    def test_the_end_date_follows_a_later_start(self, world, qapp):
        storage, group = world
        dialog = EditDialog(storage, group.id, None, day=MONDAY, start_time="09:30")
        pick(dialog, REPEAT_UNTIL)
        later = MONDAY + timedelta(days=100)
        dialog.date_edit.setDate(QDate(later.year, later.month, later.day))
        assert dialog.until_edit.date() >= dialog.date_edit.date()

    def test_a_weekly_class_has_no_dates(self, world, qapp):
        storage, group = world
        dialog = EditDialog(storage, group.id, None, day=MONDAY, start_time="09:30")
        fill(dialog)
        pick(dialog, REPEAT_WEEKLY)
        dialog.day_combo.setCurrentIndex(3)
        dialog._accept()

        (saved,) = only_dated(storage, group)
        assert (saved.on_date, saved.repeat_until, saved.day_index) == (None, None, 3)


class TestWhatTheFormShows:
    def visible(self, dialog, widget) -> bool:
        return dialog._form.isRowVisible(widget)

    def test_each_kind_of_repeat_shows_only_the_fields_it_needs(self, world, qapp):
        storage, group = world
        dialog = EditDialog(storage, group.id, None, day=MONDAY)
        pick(dialog, REPEAT_ONCE)
        assert (
            self.visible(dialog, dialog.date_edit),
            self.visible(dialog, dialog.until_edit),
            self.visible(dialog, dialog.day_combo),
        ) == (True, False, False)
        pick(dialog, REPEAT_UNTIL)
        assert (
            self.visible(dialog, dialog.date_edit),
            self.visible(dialog, dialog.until_edit),
            self.visible(dialog, dialog.day_combo),
        ) == (True, True, False)
        pick(dialog, REPEAT_WEEKLY)
        assert (
            self.visible(dialog, dialog.date_edit),
            self.visible(dialog, dialog.until_edit),
            self.visible(dialog, dialog.day_combo),
        ) == (False, False, True)

    def test_the_hint_says_what_will_happen(self, world, qapp):
        storage, group = world
        dialog = EditDialog(storage, group.id, None, day=MONDAY)
        assert "2 березня" in dialog.repeat_hint.text()
        pick(dialog, REPEAT_UNTIL)
        dialog.until_edit.setDate(QDate(2026, 3, 23))
        assert "4 рази" in dialog.repeat_hint.text()
        assert "понеділок" in dialog.repeat_hint.text()

    def test_plural_of_raz(self):
        assert [times_word(n) for n in (1, 2, 4, 5, 11, 12, 21, 22, 25)] == [
            "раз", "рази", "рази", "разів", "разів", "разів", "раз", "рази", "разів",
        ]


class TestEditingExistingClasses:
    def test_an_imported_class_cannot_be_made_dated(self, world, qapp):
        storage, group = world
        imported = storage.lessons_for_group(group.id)[0]
        dialog = EditDialog(storage, group.id, imported)
        assert not dialog._form.isRowVisible(dialog.repeat_combo)
        assert dialog.repeat_mode == REPEAT_WEEKLY
        dialog._accept()
        assert storage.lesson(imported.id).on_date is None

    def test_a_series_reopens_as_a_series(self, world, qapp):
        storage, group = world
        end = MONDAY + timedelta(days=14)
        lesson_id = storage.add_lesson(
            Lesson(
                id=0, course_id=group.course_id, day_index=0, pair=1, start_time="08:00",
                end_time="09:20", subject="Серія", url=URL, provider="zoom",
                on_date=MONDAY, repeat_until=end,
            ),
            [group.id],
        )
        dialog = EditDialog(storage, group.id, storage.lesson(lesson_id))
        assert dialog.repeat_mode == REPEAT_UNTIL
        assert dialog.until_edit.date() == QDate(end.year, end.month, end.day)

    def test_a_manual_weekly_class_can_become_a_one_off(self, world, qapp):
        storage, group = world
        lesson_id = storage.add_lesson(
            Lesson(
                id=0, course_id=group.course_id, day_index=1, pair=1, start_time="08:00",
                end_time="09:20", subject="Ручна", url=URL, provider="zoom",
            ),
            [group.id],
        )
        dialog = EditDialog(storage, group.id, storage.lesson(lesson_id))
        assert dialog.repeat_mode == REPEAT_WEEKLY
        pick(dialog, REPEAT_ONCE)
        dialog.date_edit.setDate(QDate(2026, 3, 11))  # a Wednesday
        dialog._accept()

        changed = storage.lesson(lesson_id)
        assert (changed.on_date, changed.day_index) == (date(2026, 3, 11), 2)


class TestUkrainianDialog:
    def test_no_latin_words_in_the_dialog(self, world, qapp):
        storage, group = world
        dialog = EditDialog(storage, group.id, None, day=MONDAY)
        allowed = {"Zoom", "Meet", "Google", "AutoPara", "docx", "http", "https"}
        latin = set()
        texts = [w.text() for w in dialog.findChildren(QLabel) + dialog.findChildren(QPushButton)]
        texts += [dialog.repeat_combo.itemText(i) for i in range(dialog.repeat_combo.count())]
        texts += [dialog.url_edit.placeholderText(), dialog.subject_edit.placeholderText()]
        for text in texts:
            for word in text.replace("…", " ").replace("(", " ").replace(")", " ").split():
                word = word.strip(".,:;—–-/")
                if word.isascii() and word.isalpha() and word not in allowed:
                    latin.add(word)
        assert not latin, latin

    def test_the_calendar_speaks_ukrainian(self, world, qapp):
        storage, group = world
        dialog = EditDialog(storage, group.id, None, day=MONDAY)
        calendar = dialog.date_edit.calendarWidget()
        assert calendar.locale().language().name == "Ukrainian"
        assert calendar.firstDayOfWeek().name == "Monday"
