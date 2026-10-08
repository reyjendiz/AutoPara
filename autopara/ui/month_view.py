"""The month: a card of days, each holding a few small chips for its classes.

It answers "what does this month look like" at a glance and nothing more. A chip is a subject's
colour, a time and a name; the details of a class belong to the day view, so clicking anywhere on a
day opens it there. Creating a class is the small plus that appears when the pointer is over a day.

Like the week grid it keeps no state of its own and is rebuilt from what it is handed, and it
rebuilds by hiding and deleting -- never by ``setParent(None)``, which would turn every live widget
into a window for the moment before it is gone.
"""

from __future__ import annotations

import calendar
from datetime import date, timedelta

from PySide6.QtCore import QMimeData, QPoint, QSize, Qt, Signal
from PySide6.QtGui import QDrag
from PySide6.QtWidgets import (
    QApplication,
    QFrame,
    QGridLayout,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from ..core import theme
from ..core.models import STATUS_MANUAL, STATUS_MISSED, STATUS_OPENED, STATUS_SKIPPED, Lesson
from ..importer.normalize import DAY_NAMES, DAY_SHORT, MONTH_GENITIVE
from . import icons, transitions
from .class_card import LESSON_MIME, card_fill, drag_payload, read_drag_payload, subject_color

WEEKS = 6                # a month always occupies six rows, so it never changes height
MAX_CHIPS = 3            # more than this and the rest are counted, not drawn
CELL_MIN_HEIGHT = 92
DATE_CIRCLE = 28

# What became of a class, shown in words' place: colour alone would be the only cue otherwise.
MARKS = {
    STATUS_OPENED: "✓",
    STATUS_MANUAL: "✓",
    STATUS_SKIPPED: "✕",
    STATUS_MISSED: "!",
}


def month_grid(first_of_month: date) -> list[date]:
    """The 42 dates a month view shows, from the Monday on or before the 1st."""
    start = first_of_month - timedelta(days=first_of_month.weekday())
    return [start + timedelta(days=offset) for offset in range(WEEKS * 7)]


def first_of(day: date) -> date:
    return day.replace(day=1)


def shift_month(day: date, months: int) -> date:
    """``day`` moved by whole months, clamped to the end of a shorter month (31 Jan -> 28 Feb)."""
    index = day.year * 12 + (day.month - 1) + months
    year, month = divmod(index, 12)
    month += 1
    return date(year, month, min(day.day, calendar.monthrange(year, month)[1]))


class ElidedLabel(QLabel):
    """A label that shows as much of its text as fits and ends it with an ellipsis.

    A plain label asks for the width of its whole text, which in a seven-column month would push
    the columns apart; this one asks for none and gives back what it cannot show.
    """

    def __init__(self, text: str = "", parent=None):
        super().__init__(parent)
        self._full = text
        self.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        self._elide()

    def set_full_text(self, text: str) -> None:
        self._full = text
        self._elide()

    def minimumSizeHint(self) -> QSize:  # noqa: N802 - Qt naming
        return QSize(0, super().minimumSizeHint().height())

    def resizeEvent(self, event):  # noqa: N802 - Qt naming
        super().resizeEvent(event)
        self._elide()

    def _elide(self) -> None:
        self.setText(self.fontMetrics().elidedText(self._full, Qt.ElideRight, max(self.width(), 0)))


class MonthChip(QFrame):
    """One class on one day: a bar in the subject's colour, the time and the name."""

    def __init__(self, lesson: Lesson, status: str | None, parent=None):
        super().__init__(parent)
        self.lesson = lesson
        self.setObjectName("MonthChip")
        done = status in (STATUS_OPENED, STATUS_MANUAL, STATUS_SKIPPED)
        self.setProperty("done", "true" if done else "false")
        if not done and status != STATUS_MISSED:
            # The subject's colour tints the chip, the same as it tints the week's card.
            self.setStyleSheet(
                f"#MonthChip {{ background: {card_fill(lesson.subject, theme.token('sunken'))}; }}"
            )
        row = QHBoxLayout(self)
        row.setContentsMargins(6, 2, 6, 2)
        row.setSpacing(5)

        bar = QFrame()
        bar.setObjectName("MonthChipBar")
        bar.setFixedSize(3, 14)
        colour = theme.token("danger") if status == STATUS_MISSED else subject_color(lesson.subject)
        bar.setStyleSheet(f"background: {colour};")
        row.addWidget(bar)

        mark = MARKS.get(status, "")
        text = f"{lesson.start_time} {mark + ' ' if mark else ''}{lesson.subject}"
        self.label = ElidedLabel(text)
        self.label.setObjectName("MonthChipText")
        row.addWidget(self.label, 1)

        self.setToolTip(
            f"{lesson.subject}\n{lesson.start_time}–{lesson.end_time}"
            + (f"\n{lesson.teacher.strip('()')}" if lesson.teacher else "")
        )
        # The chip is only a label for the day: clicks fall through to the day that holds it.
        self.setAttribute(Qt.WA_TransparentForMouseEvents)


class DayTile(QFrame):
    """One day of the month. Clicking it opens that day; the plus creates a class on it."""

    selected = Signal(object)         # the date
    add_requested = Signal(object)    # the date
    lesson_dropped = Signal(int, object, object)  # lesson id, the date dropped on, date dragged from

    def __init__(
        self,
        day: date,
        in_month: bool,
        is_today: bool,
        entries: list[tuple[Lesson, str | None]],
        parent=None,
    ):
        super().__init__(parent)
        self.day = day
        self.setObjectName("MonthCell")
        self.setProperty("other", "false" if in_month else "true")
        self.setProperty("today", "true" if is_today else "false")
        self.setProperty("dropping", "false")
        self.setCursor(Qt.PointingHandCursor)
        self.setAcceptDrops(True)
        self._press_at: QPoint | None = None
        self._pressed_chip: MonthChip | None = None
        self._dragging = False
        self.setMinimumHeight(CELL_MIN_HEIGHT)
        self.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Expanding)

        column = QVBoxLayout(self)
        column.setContentsMargins(8, 6, 8, 6)
        column.setSpacing(2)

        top = QHBoxLayout()
        top.setContentsMargins(0, 0, 0, 0)
        number = QLabel(str(day.day))
        number.setObjectName("MonthDate")
        number.setProperty("today", "true" if is_today else "false")
        number.setProperty("other", "false" if in_month else "true")
        number.setFixedSize(DATE_CIRCLE, DATE_CIRCLE)
        number.setAlignment(Qt.AlignCenter)
        number.setToolTip(f"{DAY_NAMES[day.weekday()]}, {day.day} {MONTH_GENITIVE[day.month - 1]}")
        top.addWidget(number)
        top.addStretch(1)

        self.add_button = transitions.FadeButton("")
        self.add_button.setObjectName("MonthAdd")
        self.add_button.setFixedSize(DATE_CIRCLE, DATE_CIRCLE)
        self.add_button.setIcon(icons.icon("add", theme.token("text_muted"), 14))
        self.add_button.setToolTip("Додати пару на цей день")
        self.add_button.clicked.connect(lambda: self.add_requested.emit(self.day))
        self.add_button.hide()
        top.addWidget(self.add_button)
        column.addLayout(top)

        self.chips: list[MonthChip] = []
        for lesson, status in entries[:MAX_CHIPS]:
            chip = MonthChip(lesson, status)
            self.chips.append(chip)
            column.addWidget(chip)
        hidden = len(entries) - MAX_CHIPS
        if hidden > 0:
            more = QLabel(f"ще {hidden}")
            more.setObjectName("MonthMore")
            column.addWidget(more)
        column.addStretch(1)

        count = len(entries)
        if count:
            self.setToolTip(f"{count} {pairs_word(count)}")

    def enterEvent(self, event):  # noqa: N802 - Qt naming
        self.add_button.show()
        super().enterEvent(event)

    def leaveEvent(self, event):  # noqa: N802 - Qt naming
        self.add_button.hide()
        super().leaveEvent(event)

    # ------------------------------------------------------------- drag and drop

    def chip_at(self, point: QPoint) -> MonthChip | None:
        """The class chip under ``point``. Chips let mouse events through, so the tile asks."""
        return next((chip for chip in self.chips if chip.geometry().contains(point)), None)

    def mousePressEvent(self, event):  # noqa: N802 - Qt naming
        if event.button() == Qt.LeftButton:
            self._press_at = event.position().toPoint()
            self._pressed_chip = self.chip_at(self._press_at)
            self._dragging = False
        super().mousePressEvent(event)

    def mouseMoveEvent(self, event):  # noqa: N802 - Qt naming
        if (
            self._press_at is None
            or self._pressed_chip is None
            or not (event.buttons() & Qt.LeftButton)
        ):
            return super().mouseMoveEvent(event)
        travelled = (event.position().toPoint() - self._press_at).manhattanLength()
        if travelled < QApplication.startDragDistance():
            return super().mouseMoveEvent(event)

        self._dragging = True
        chip = self._pressed_chip
        data = QMimeData()
        data.setData(LESSON_MIME, drag_payload(chip.lesson.id, self.day))
        drag = QDrag(self)
        drag.setMimeData(data)
        drag.setPixmap(chip.grab())
        drag.setHotSpot(self._press_at - chip.pos())
        drag.exec(Qt.MoveAction)
        return None

    def _set_dropping(self, active: bool) -> None:
        wanted = "true" if active else "false"
        if self.property("dropping") == wanted:
            return
        self.setProperty("dropping", wanted)
        self.style().unpolish(self)
        self.style().polish(self)

    def dragEnterEvent(self, event):  # noqa: N802 - Qt naming
        if event.mimeData().hasFormat(LESSON_MIME):
            self._set_dropping(True)
            event.acceptProposedAction()

    def dragMoveEvent(self, event):  # noqa: N802 - Qt naming
        if event.mimeData().hasFormat(LESSON_MIME):
            event.acceptProposedAction()

    def dragLeaveEvent(self, event):  # noqa: N802 - Qt naming
        self._set_dropping(False)
        super().dragLeaveEvent(event)

    def dropEvent(self, event):  # noqa: N802 - Qt naming
        self._set_dropping(False)
        if not event.mimeData().hasFormat(LESSON_MIME):
            return
        try:
            lesson_id, source_day = read_drag_payload(event.mimeData().data(LESSON_MIME))
        except ValueError:
            return
        event.acceptProposedAction()
        self.lesson_dropped.emit(lesson_id, self.day, source_day)

    def mouseReleaseEvent(self, event):  # noqa: N802 - Qt naming
        was_drag = self._dragging
        self._press_at, self._pressed_chip, self._dragging = None, None, False
        if (
            not was_drag
            and event.button() == Qt.LeftButton
            and self.rect().contains(event.position().toPoint())
        ):
            self.selected.emit(self.day)
        super().mouseReleaseEvent(event)


def pairs_word(count: int) -> str:
    """``1 пара``, ``2 пари``, ``5 пар``."""
    if count % 10 == 1 and count % 100 != 11:
        return "пара"
    if count % 10 in (2, 3, 4) and count % 100 not in (12, 13, 14):
        return "пари"
    return "пар"


class MonthView(QWidget):
    """Six weeks of day tiles inside one rounded card."""

    day_selected = Signal(object)  # the date
    add_requested = Signal(object)  # the date
    lesson_dropped = Signal(int, object, object)  # lesson id, the date dropped on, date dragged from

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("MonthCard")
        # A plain QWidget ignores the stylesheet's background and border unless told otherwise.
        self.setAttribute(Qt.WA_StyledBackground, True)
        outer = QVBoxLayout(self)
        outer.setContentsMargins(12, 12, 12, 12)
        outer.setSpacing(4)

        names = QHBoxLayout()
        names.setContentsMargins(0, 0, 0, 0)
        names.setSpacing(0)
        for index in range(7):
            label = QLabel(DAY_SHORT[index])
            label.setObjectName("MonthWeekday")
            label.setAlignment(Qt.AlignCenter)
            names.addWidget(label, 1)
        outer.addLayout(names)

        self._body = QWidget()
        self._body.setObjectName("MonthBody")
        self._grid = QGridLayout(self._body)
        self._grid.setContentsMargins(0, 0, 0, 0)
        self._grid.setSpacing(0)
        for column in range(7):
            self._grid.setColumnStretch(column, 1)
        for row in range(WEEKS):
            self._grid.setRowStretch(row, 1)
        outer.addWidget(self._body, 1)
        self.tiles: list[DayTile] = []

    def morph_elements(self) -> list[tuple[QWidget, int, date]]:
        """The class chips on screen -- what a morph carries from one calendar to the next."""
        return [
            (chip, chip.lesson.id, tile.day)
            for tile in self.tiles
            for chip in tile.chips
            if chip.isVisible()
        ]

    def morph_first_day(self) -> date | None:
        return self.tiles[0].day if self.tiles else None

    @staticmethod
    def visible_days(first_of_month: date) -> list[date]:
        """The dates a month view draws, so the caller can ask storage for exactly those."""
        return month_grid(first_of_month)

    def render_month(
        self,
        first_of_month: date,
        lessons: list[Lesson],
        statuses: dict[tuple[int, str], str] | None = None,
        today: date | None = None,
    ) -> None:
        """Draw the month that contains ``first_of_month``.

        ``statuses`` is keyed by ``(lesson id, ISO date)``, the same as the week grid's, because a
        class that repeats is one lesson with a verdict per date.
        """
        statuses = statuses or {}
        today = today or date.today()
        self._clear()
        for position, day in enumerate(month_grid(first_of_month)):
            entries = [
                (lesson, statuses.get((lesson.id, day.isoformat())))
                for lesson in sorted(lessons, key=lambda item: item.start_time)
                if lesson.occurs_on(day)
            ]
            tile = DayTile(
                day,
                in_month=day.month == first_of_month.month,
                is_today=day == today,
                entries=entries,
            )
            tile.selected.connect(self.day_selected.emit)
            tile.add_requested.connect(self.add_requested.emit)
            tile.lesson_dropped.connect(self.lesson_dropped.emit)
            self._grid.addWidget(tile, *divmod(position, 7))
            self.tiles.append(tile)

    def _clear(self) -> None:
        """Empty the month without ever detaching a widget from its parent (see the module note)."""
        self.tiles = []
        while self._grid.count():
            widget = self._grid.takeAt(0).widget()
            if widget is not None:
                widget.hide()
                widget.deleteLater()
