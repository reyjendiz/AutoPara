"""Layout guarantees behind the look: nothing overlaps, nothing is clipped, the line is on time.

These render real widgets offscreen and look at pixels or geometry, because every one of them was
a bug that no amount of reading the code showed. They build their own timetable, so they run
without the schedule document.
"""

from __future__ import annotations

from datetime import date, datetime

import pytest
from PySide6.QtGui import QColor
from PySide6.QtWidgets import QLabel

from autopara.core import theme
from autopara.core.models import THEME_DARK, THEME_LIGHT, Lesson
from autopara.ui.class_card import CARD_MARGIN_H, CARD_MARGIN_LEFT, ClassCard
from autopara.ui.week_grid import (
    DAY_START_MINUTES,
    FIRST_HOUR,
    HEADER_HEIGHT,
    HOUR_HEIGHT,
    GridCell,
    WeekGrid,
    minutes_to_pixels,
)

# 2026-03-02 is a Monday.
MONDAY = date(2026, 3, 2)


def use_theme(qapp, name):
    """Apply a theme only when it is not already the one showing.

    Re-applying the stylesheet restyles every widget the process still holds, and a suite leaves
    plenty of them about: doing it in every test made a seven-second run take fifty.
    """
    if qapp.styleSheet() == "" or theme.active() != name:
        theme.apply(qapp, name)


def lesson(
    lesson_id: int, start: str, end: str, subject: str, day_index: int = 0, **extra
) -> Lesson:
    return Lesson(
        id=lesson_id,
        course_id=1,
        day_index=day_index,
        pair=1,
        start_time=start,
        end_time=end,
        subject=subject,
        url="https://zoom.us/j/1",
        provider="zoom",
        **extra,
    )


@pytest.fixture
def grid(qapp):
    widget = WeekGrid()
    widget.resize(1100, 800)
    yield widget
    widget.close()
    widget.deleteLater()


def shown(widget, qapp, lessons, today=MONDAY):
    widget.render_week(lessons, today=today, monday=MONDAY)
    widget.show()
    qapp.processEvents()


class TestCardsDoNotHideEachOther:
    def test_two_classes_that_touch_within_an_hour_are_both_drawn_in_full(self, grid, qapp):
        """08:00-09:20 and 09:30-10:50 share the 09:00 row. The second one's slot used to paint
        an opaque rectangle over the bottom of the first, cutting its card off at 09:00."""
        theme.apply(qapp, THEME_DARK)
        try:
            shown(grid, qapp, [lesson(1, "08:00", "09:20", "A"), lesson(2, "09:30", "10:50", "B")])
            first = next(c for c in grid.findChildren(ClassCard) if c.lesson.id == 1)
            image = grid.grab().toImage()

            # Ten pixels into the 09:00 row, inside the first card and above the second.
            card_point = first.mapTo(grid, first.rect().bottomLeft())
            # Near the right edge, where no text can be: the card's time sits low on the left, and its
            # exact place depends on the platform's font.
            x, y = card_point.x() + first.width() - 12, card_point.y() - 8
            painted = QColor(image.pixel(x, y))
            card_bg = QColor(theme.token("card_bg"))
            assert painted == card_bg, f"the bottom of the card is covered: {painted.name()}"
        finally:
            theme.apply(qapp, THEME_LIGHT)

    def test_a_slot_paints_nothing(self, grid, qapp):
        shown(grid, qapp, [lesson(1, "08:00", "09:20", "A")])
        from PySide6.QtWidgets import QWidget

        found = [w for w in grid.findChildren(QWidget) if w.objectName() == "CardSlot"]
        assert found, "cards sit in named slots so the stylesheet can make them transparent"
        assert "#CardSlot" in theme.stylesheet(THEME_LIGHT)


class TestRowsAreAnHourTall:
    def test_a_grid_shown_empty_and_filled_later_keeps_full_hours(self, grid, qapp):
        """The window is shown before its week arrives (an import, a first run, a view change).
        The scroll area sized the canvas for the empty grid, and the filled one was squeezed into
        it: 60 px hours and a header cut through."""
        grid.resize(1100, 600)  # shorter than the 798 px the rows need, as on a laptop
        grid.show()
        qapp.processEvents()
        shown(grid, qapp, [lesson(1, "08:00", "09:20", "A")])

        tops = sorted(
            {c.geometry().top() for c in grid.findChildren(GridCell) if c.parent() is not None}
        )
        assert {b - a for a, b in zip(tops, tops[1:])} == {HOUR_HEIGHT}
        assert tops[0] == HEADER_HEIGHT


class TestColumnsAreEqual:
    def test_a_long_subject_does_not_widen_its_column(self, grid, qapp):
        long_subject = "Основи інклюзивної освіти та корекційної педагогіки для дошкільників"
        lessons = [
            lesson(1, "08:00", "09:20", long_subject),
            lesson(2, "08:00", "09:20", "Ск", day_index=1),
        ]
        shown(grid, qapp, lessons)
        widths = {
            cell.day: cell.width()
            for cell in grid.findChildren(GridCell)
            if cell.parent() is not None and cell.hour == FIRST_HOUR
        }
        assert max(widths.values()) - min(widths.values()) <= 1, widths


class TestChipsNeverReadWrong:
    def test_a_narrow_card_gives_up_chips_rather_than_cutting_their_words(self, grid, qapp):
        """In a week of six columns a card is about 150 px: too narrow for state, provider and
        group together. The squeezed row read "Zoon", "✓від" and "2 гру"."""
        item = lesson(
            1, "08:00", "09:20", "Загальна педагогіка", group_names=["11 група", "12 група"]
        )
        grid.render_week([item], statuses={1: "opened"}, today=MONDAY, monday=MONDAY)
        grid.show()
        qapp.processEvents()

        card = grid.findChildren(ClassCard)[0]
        visible = [chip for chip in card._chips if not chip.isHidden()]
        assert visible, "at least one chip always stays"
        assert len(visible) < len(card._chips), "the card is too narrow for all three"
        for chip in visible:
            assert chip.width() >= chip.sizeHint().width() - 2, f"{chip.text()!r} is squeezed"
            assert chip.mapTo(card, chip.rect().topRight()).x() <= card.width()
        assert card._chips[0] in visible, "what became of the class is the last chip to go"

    def test_the_chips_that_stay_get_their_full_width(self, grid, qapp):
        """Hiding the third chip only *schedules* a new layout; until it ran, the two that stayed
        were still as narrow as when all three had competed for the room ("Zoon", "✓від").

        The window is sized from the chips' own widths (fonts differ between platforms), to be just
        wide enough for the first two and not for the third.
        """
        group_names = ["11 група", "12 група"]
        item = lesson(1, "08:00", "09:20", "Загальна педагогіка", group_names=group_names)
        probe = ClassCard(item, status="opened", height=88)
        first, second, third = (chip.sizeHint().width() for chip in probe._chips)
        probe.deleteLater()
        card_width = CARD_MARGIN_LEFT + CARD_MARGIN_H + first + second + 5 + 4
        assert card_width < CARD_MARGIN_LEFT + CARD_MARGIN_H + first + second + third + 10
        column = card_width + 8                       # the slot's own 4 px margins
        grid.resize(74 + 6 * column + 16, 800)        # gutter, six columns, scroll bar and frame

        grid.show()
        qapp.processEvents()
        grid.render_week([item], statuses={1: "opened"}, today=MONDAY, monday=MONDAY)
        qapp.processEvents()  # one pass only: no second chance for the layout to catch up

        card = grid.findChildren(ClassCard)[0]
        visible = [chip for chip in card._chips if not chip.isHidden()]
        assert len(visible) == 2, [c.text() for c in visible]
        for chip in visible:
            assert abs(chip.width() - chip.sizeHint().width()) <= 1, chip.text()

    def test_a_wide_card_shows_them_all(self, qapp):
        item = lesson(
            1, "08:00", "09:20", "Загальна педагогіка", group_names=["11 група", "12 група"]
        )
        card = ClassCard(item, status="opened", height=88)
        card.resize(400, 88)
        card.show()
        qapp.processEvents()
        assert all(not chip.isHidden() for chip in card._chips)
        assert len(card._chips) == 3
        card.close()

    def test_a_class_without_a_link_names_no_provider(self, qapp):
        item = lesson(1, "08:00", "09:20", "Без посилання")
        item.url = None
        item.needs_link = True
        card = ClassCard(item, height=88)
        texts = [chip.text() for chip in card._chips]
        assert texts == ["без посилання"]


class TestNowLine:
    def now_at(self, grid, qapp, moment: datetime):
        shown(grid, qapp, [], today=moment.date())
        grid._place_now(moment)

    def test_it_sits_at_the_minute_the_clock_says(self, grid, qapp):
        moment = datetime(2026, 3, 2, 10, 30)
        self.now_at(grid, qapp, moment)

        anchor = next(
            c for c in grid.findChildren(GridCell)
            if c.parent() is not None and c.day == MONDAY and c.hour == FIRST_HOUR
        )
        expected = anchor.geometry().top() + minutes_to_pixels(10 * 60 + 30 - DAY_START_MINUTES)
        line = grid._now_line.geometry()
        assert not grid._now_line.isHidden()
        assert line.top() + 1 == expected
        assert line.left() == anchor.geometry().left() and line.width() == anchor.width()
        assert grid._now_badge.text() == "10:30"

    def test_the_line_and_its_badge_are_half_transparent(self, grid, qapp):
        for marker in (grid._now_line, grid._now_badge):
            assert marker.graphicsEffect().opacity() == 0.5

    def test_it_is_hidden_when_today_is_not_on_screen(self, grid, qapp):
        shown(grid, qapp, [], today=date(2026, 3, 20))  # a week the grid is not showing
        grid._place_now(datetime(2026, 3, 20, 10, 0))
        assert grid._now_line.isHidden() and grid._now_badge.isHidden()

    def test_it_is_hidden_outside_the_hours_the_grid_draws(self, grid, qapp):
        self.now_at(grid, qapp, datetime(2026, 3, 2, 6, 15))
        assert grid._now_line.isHidden()
        grid._place_now(datetime(2026, 3, 2, 23, 40))
        assert grid._now_line.isHidden()

class TestRoundEnds:
    """A button with a round end is round only if its radius is at most half its height.

    Qt draws a corner square -- not a little less round -- when the radius exceeds half the height,
    which is how every button came out as a plain rectangle on a real Mac while looking right in a
    test that never measured one.
    """

    # (object name, exact height, exact width or None)
    SHAPES = [
        ("", 36, None),
        ("Primary", 36, None),
        ("Plain", 36, None),
        ("NavButton", 36, 36),
        ("IconButton", 36, 36),
        ("PrimaryRound", 40, 40),
        ("SegButton", 32, None),
        ("MonthAdd", 28, 28),
    ]

    def make(self, qapp, name):
        from PySide6.QtWidgets import QPushButton

        use_theme(qapp, THEME_LIGHT)
        if name == "PrimaryRound":  # its fill is painted by FadeButton, not by the stylesheet
            from autopara.ui.transitions import FadeButton

            button = FadeButton("", rest="rail_primary_bg")
        else:
            button = QPushButton("Зберегти" if name in ("", "Primary", "Plain", "SegButton") else "")
        if name:
            button.setObjectName(name)
        if name == "SegButton":
            button.setCheckable(True)
            button.setChecked(True)
        button.show()
        qapp.processEvents()
        return button

    @pytest.mark.parametrize("name,height,width", SHAPES)
    def test_every_button_has_the_exact_height_its_radius_assumes(self, qapp, name, height, width):
        button = self.make(qapp, name)
        assert button.height() == height, f"#{name or 'default'} is {button.height()} px tall"
        if width:
            assert button.width() == width
        button.close()

    @pytest.mark.parametrize("name", ["", "Primary", "NavButton", "PrimaryRound"])
    def test_the_corners_of_a_filled_button_are_really_round(self, qapp, name):
        """Look at the pixel where the button's corner would be: on the canvas colour if the corner
        is round, on the button's own fill or border if it is square."""
        from PySide6.QtWidgets import QVBoxLayout, QWidget

        button = self.make(qapp, name)
        stage = QWidget()
        stage.setObjectName("Canvas")
        box = QVBoxLayout(stage)
        box.setContentsMargins(10, 10, 10, 10)
        box.addWidget(button)
        stage.show()
        qapp.processEvents()

        canvas = theme.PALETTES[THEME_LIGHT]["canvas"]
        image = stage.grab().toImage()
        corner = QColor(image.pixel(10, 10)).name()
        centre = QColor(image.pixel(10 + button.width() // 2, 10 + button.height() // 2)).name()
        assert corner == canvas, f"#{name or 'default'} has a square corner ({corner})"
        assert centre != canvas
        stage.close()

    def test_the_segmented_control_is_a_pill_too(self, qapp):
        from PySide6.QtWidgets import QFrame, QVBoxLayout, QWidget

        from autopara.ui.nav_bar import NavBar

        use_theme(qapp, THEME_LIGHT)
        stage = QWidget()
        stage.setObjectName("Canvas")
        box = QVBoxLayout(stage)
        box.setContentsMargins(10, 10, 10, 10)
        bar = NavBar()
        box.addWidget(bar)
        stage.resize(900, 80)
        stage.show()
        qapp.processEvents()

        frame = next(f for f in bar.findChildren(QFrame) if f.objectName() == "Segmented")
        assert frame.height() == 40
        origin = frame.mapTo(stage, frame.rect().topLeft())
        image = stage.grab().toImage()
        corner = QColor(image.pixel(origin.x(), origin.y())).name()
        assert corner == theme.PALETTES[THEME_LIGHT]["canvas"], "a square corner"
        stage.close()


class TestLongTitlesAreCutInWholeLines:
    """The title used to be cut through the middle of a line, top and bottom, behind the chips."""

    def metrics(self, qapp):
        from PySide6.QtGui import QFont, QFontMetrics

        return QFontMetrics(QFont("Inter", 10))

    def test_text_that_fits_is_left_alone(self, qapp):
        from autopara.ui.class_card import clamp_lines

        assert clamp_lines("Фізичне виховання", self.metrics(qapp), 400, 2) == "Фізичне виховання"

    def test_a_long_title_keeps_whole_lines_and_ends_in_an_ellipsis(self, qapp):
        from autopara.ui.class_card import clamp_lines

        fm = self.metrics(qapp)
        title = "Теорія і методика організації образотворчої діяльності дітей дошкільного віку"
        shown = clamp_lines(title, fm, 150, 2)
        lines = shown.split("\n")
        assert len(lines) == 2
        assert lines[-1].endswith("…")
        assert all(fm.horizontalAdvance(line) <= 150 for line in lines)
        assert lines[0].endswith(" ") is False and lines[0] in title, "the first line is whole words"

    def test_a_single_word_wider_than_the_line_is_broken_not_overflowed(self, qapp):
        from autopara.ui.class_card import clamp_lines

        fm = self.metrics(qapp)
        shown = clamp_lines("Надзвичайнодовгеслововбезпробілів", fm, 80, 3)
        assert all(fm.horizontalAdvance(line) <= 80 for line in shown.split("\n"))

    def test_a_card_shows_at_most_the_lines_it_has_room_for_and_never_overlaps_its_chips(
        self, grid, qapp
    ):
        title = "Теорія і методика організації образотворчої діяльності дітей дошкільного віку"
        grid.render_week(
            [lesson(1, "14:40", "16:00", title, teacher="(Шевченко Н.О.)")],
            today=MONDAY,
            monday=MONDAY,
        )
        grid.show()
        qapp.processEvents()
        card = grid.findChildren(ClassCard)[0]
        subject = next(
            w for w in card.findChildren(QLabel) if w.objectName() == "CardSubject"
        )
        chip = card._chips[0]
        assert subject.geometry().top() >= chip.geometry().bottom(), "the title starts below the chips"
        assert subject.geometry().bottom() <= card.height(), "and ends inside the card"
        assert subject.text().count("\n") <= 2
