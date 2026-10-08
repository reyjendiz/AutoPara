"""The "next up" pill: which class it names and what it says."""

from __future__ import annotations

from datetime import date, datetime

from autopara.core import nextup
from autopara.core.models import Lesson

MONDAY = date(2026, 3, 2)


def lesson(lesson_id, subject, start, end, day_index=0, **extra) -> Lesson:
    return Lesson(id=lesson_id, course_id=1, day_index=day_index, pair=1, start_time=start,
                  end_time=end, subject=subject, **extra)


MORNING = lesson(1, "Історія України", "08:00", "09:20")
LATER = lesson(2, "Психологія", "11:20", "12:40")
EVENING = lesson(3, "Логіка", "14:40", "16:00")
TUESDAY = lesson(4, "Музика", "08:00", "09:20", day_index=1)
ALL = [LATER, MORNING, TUESDAY, EVENING]


def at(hour, minute=0) -> datetime:
    return datetime(2026, 3, 2, hour, minute)


class TestFind:
    def test_before_the_first_class_it_is_the_first_class(self):
        found = nextup.find(ALL, at(7, 30))
        assert found.lesson is MORNING and not found.ongoing and found.minutes == 30

    def test_during_a_class_it_is_that_class_and_the_minutes_left(self):
        found = nextup.find(ALL, at(8, 40))
        assert found.lesson is MORNING and found.ongoing and found.minutes == 40

    def test_between_classes_it_is_the_next_one(self):
        found = nextup.find(ALL, at(10, 0))
        assert found.lesson is LATER and not found.ongoing and found.minutes == 80

    def test_a_class_ending_this_minute_is_over(self):
        assert nextup.find(ALL, at(9, 20)).lesson is LATER

    def test_after_the_last_class_there_is_nothing(self):
        assert nextup.find(ALL, at(16, 0)) is None
        assert nextup.find(ALL, at(20, 0)) is None

    def test_only_todays_classes_count(self):
        assert nextup.find([TUESDAY], at(7, 0)) is None

    def test_a_date_a_class_was_moved_away_from_is_not_today(self):
        gone = lesson(5, "Перенесена", "09:00", "10:20", skip_dates=frozenset({MONDAY}))
        assert nextup.find([gone], at(8, 0)) is None

    def test_a_skipped_class_is_not_next(self):
        assert nextup.find(ALL, at(7, 0), settled={MORNING.id}).lesson is LATER

    def test_nothing_at_all_is_nothing(self):
        assert nextup.find([], at(8, 0)) is None

    def test_a_class_about_to_start_never_says_zero_minutes(self):
        assert nextup.find(ALL, datetime(2026, 3, 2, 7, 59, 40)).minutes == 1


class TestLabel:
    def test_a_class_that_is_on(self):
        assert nextup.label(nextup.find(ALL, at(8, 40))) == "Зараз · Історія України · ще 40 хв"

    def test_a_class_soon(self):
        assert nextup.label(nextup.find(ALL, at(7, 35))) == "Далі · Історія України · через 25 хв"

    def test_a_class_far_off_says_the_time_instead_of_a_long_countdown(self):
        assert nextup.label(nextup.find(ALL, at(9, 30))) == "Далі · Психологія · о 11:20"

    def test_the_limit_itself_still_counts_down(self):
        assert nextup.label(nextup.find(ALL, at(9, 50))).endswith("через 90 хв")
