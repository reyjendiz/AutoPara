"""The search and subject filter: what matches, and how the jump to the next match walks dates."""

from __future__ import annotations

from datetime import date, timedelta

from autopara.core import filters
from autopara.core.filters import LessonFilter
from autopara.core.models import Lesson

MONDAY = date(2026, 3, 2)


def lesson(lesson_id, subject, teacher="", day_index=0, on_date=None, **extra) -> Lesson:
    return Lesson(
        id=lesson_id, course_id=1, day_index=day_index, pair=1, start_time="08:00",
        end_time="09:20", subject=subject, teacher=teacher, on_date=on_date, **extra,
    )


PEDAGOGY = lesson(1, "Загальна педагогіка", "(Роман Н.М.)", 0)
HISTORY = lesson(2, "Історія України", "(Іваненко І.І.)", 1)
EXTRA = lesson(3, "Основи п'ятиденного плану", "(Коваль О.П.)", 2)
ALL = [PEDAGOGY, HISTORY, EXTRA]


class TestMatching:
    def test_an_empty_filter_lets_everything_through(self):
        assert not LessonFilter().active
        assert filters.apply(ALL, LessonFilter()) == ALL

    def test_whitespace_alone_is_not_a_filter(self):
        assert not LessonFilter(text="   ").active

    def test_a_word_is_found_in_the_subject(self):
        assert filters.apply(ALL, LessonFilter(text="педагог")) == [PEDAGOGY]

    def test_a_word_is_found_in_the_teacher(self):
        assert filters.apply(ALL, LessonFilter(text="роман")) == [PEDAGOGY]

    def test_case_does_not_matter(self):
        assert filters.apply(ALL, LessonFilter(text="ІСТОРІЯ")) == [HISTORY]

    def test_every_word_must_be_found_in_any_order(self):
        assert filters.apply(ALL, LessonFilter(text="роман педаг")) == [PEDAGOGY]
        assert filters.apply(ALL, LessonFilter(text="педаг іваненко")) == []

    def test_apostrophe_variants_are_the_same_letter(self):
        assert filters.apply(ALL, LessonFilter(text="п’ятиденного")) == [EXTRA]
        assert filters.apply(ALL, LessonFilter(text="п`ятиденного")) == [EXTRA]

    def test_nothing_found_is_an_empty_list_not_an_error(self):
        assert filters.apply(ALL, LessonFilter(text="хімія")) == []

    def test_a_subject_filter_is_exact_and_ignores_case(self):
        assert filters.apply(ALL, LessonFilter(subject="історія україни")) == [HISTORY]
        assert filters.apply(ALL, LessonFilter(subject="Історія")) == []

    def test_subject_and_text_both_have_to_hold(self):
        flt = LessonFilter(text="роман", subject="Історія України")
        assert filters.apply(ALL, flt) == []

    def test_the_original_order_is_kept(self):
        assert filters.apply(ALL, LessonFilter(text="а")) == [PEDAGOGY, HISTORY, EXTRA]


class TestSubjects:
    def test_each_subject_once_alphabetically(self):
        twice = [*ALL, lesson(4, "загальна педагогіка", "(Інший В.)", 3)]
        assert filters.subjects(twice) == [
            "Загальна педагогіка", "Історія України", "Основи п'ятиденного плану",
        ]

    def test_no_lessons_no_subjects(self):
        assert filters.subjects([]) == []


class TestFindMatch:
    def test_the_next_date_with_a_matching_class(self):
        # History is on Tuesdays; from a Wednesday the next one is six days on.
        wednesday = MONDAY + timedelta(days=2)
        assert filters.find_match(ALL, LessonFilter(text="історія"), wednesday) == (
            MONDAY + timedelta(days=8)
        )

    def test_the_start_date_itself_is_not_the_answer(self):
        tuesday = MONDAY + timedelta(days=1)
        assert filters.find_match(ALL, LessonFilter(text="історія"), tuesday) == (
            tuesday + timedelta(days=7)
        )

    def test_backwards_finds_the_previous_one(self):
        wednesday = MONDAY + timedelta(days=2)
        assert filters.find_match(ALL, LessonFilter(text="історія"), wednesday, -1) == (
            MONDAY + timedelta(days=1)
        )

    def test_a_one_off_is_found_on_its_own_date_only(self):
        oneoff = lesson(9, "Семінар", day_index=4, on_date=MONDAY + timedelta(days=11))
        assert filters.find_match([oneoff], LessonFilter(text="семінар"), MONDAY) == (
            MONDAY + timedelta(days=11)
        )
        assert filters.find_match([oneoff], LessonFilter(text="семінар"), MONDAY + timedelta(days=11)) is None

    def test_a_date_a_class_was_moved_away_from_is_not_a_match(self):
        skipped = MONDAY + timedelta(days=1)
        moved = lesson(2, "Історія України", day_index=1, skip_dates=frozenset({skipped}))
        found = filters.find_match([moved], LessonFilter(text="історія"), MONDAY)
        assert found == skipped + timedelta(days=7)

    def test_no_match_anywhere_gives_none(self):
        assert filters.find_match(ALL, LessonFilter(text="хімія"), MONDAY) is None
