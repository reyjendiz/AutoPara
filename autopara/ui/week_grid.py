"""The calendar surface: day columns by hourly rows.

Rows are **hours**, 08:00 to 18:00, and a class is positioned by its real start and end rather
than dropped into a slot. A 09:30-10:50 pair therefore covers the bottom half of the 09:00 row and
most of the 10:00 one, exactly as it would in a calendar. The old slot table gave every class a
whole cell whatever its time, which is what made the times down the side look arbitrary.

The trick that keeps this simple: every row is exactly ``HOUR_HEIGHT`` px, so a minute is a fixed
number of pixels and a card's offset inside its span is just its start minute, scaled. No
fractional layout, no sub-rows, no custom paint.

Columns are **dates**. ``render_days`` draws any list of dates -- a week, or a single day -- and
``render_week`` is the week's way in: Mon..Sat are always shown because that is the teaching week,
and Sunday appears only if a lesson actually lands on it (docs/FRONTEND.md).

The grid is interactive in three ways, all of which report upwards rather than touching storage:
clicking a card, clicking an empty hour, and dropping a card onto another hour.
"""

from __future__ import annotations

from datetime import date, datetime, timedelta

from PySide6.QtCore import Qt, QTimer, Signal
from PySide6.QtWidgets import (
    QFrame,
    QGraphicsOpacityEffect,
    QGridLayout,
    QLabel,
    QScrollArea,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

from ..core.models import Lesson
from ..importer.normalize import DAY_NAMES, DAY_SHORT, MONTH_GENITIVE, week_start
from .class_card import LESSON_MIME, ClassCard, read_drag_payload

# The visible day: 08:00 up to and including the 18:00 row. The teaching day ends well before
# that -- the latest class in the source document finishes at 17:30 -- and every hour past it was a
# band of empty grid that the week had to scroll through. A class outside the window is not lost:
# ``span_for`` clamps it to the last row, and the edit dialog takes any time at all.
FIRST_HOUR = 8
LAST_HOUR = 18
HOURS = list(range(FIRST_HOUR, LAST_HOUR + 1))

DAY_START_MINUTES = FIRST_HOUR * 60
DAY_END_MINUTES = (LAST_HOUR + 1) * 60

# Every row is exactly this tall, and a minute is that divided by sixty. Sixty would be tidier --
# a minute would be a pixel -- but it is not enough room: the standard 80-minute pair needs 87 px
# to show its subject, teacher and time, and at 60 px an hour it would get 80. Eleven rows at 66
# still fit the default window without scrolling, which is the whole reason the day now ends at
# 18:00. The placement maths relies on rows being pinned to this exactly; change them together.
HOUR_HEIGHT = 66


def minutes_to_pixels(minutes: int) -> int:
    """A duration in the grid's own units. Rounded once, here, so no caller carries a float."""
    return round(minutes * HOUR_HEIGHT / 60)

# Anything shorter is still drawn this tall, so a ten-minute entry stays readable and clickable.
MIN_CARD_MINUTES = 26

GUTTER_WIDTH = 74
COLUMN_MIN_WIDTH = 150
HEADER_HEIGHT = 72
DATE_CIRCLE = 32  # the date's circle; today's is the black pill

# How often the "now" line moves. Half a minute keeps it within a pixel of the clock.
NOW_REFRESH_MS = 30_000
# How strongly the "now" line and its time badge show: 50 %.
NOW_OPACITY = 0.5


def to_minutes(hhmm: str) -> int:
    hour, minute = (int(part) for part in hhmm.split(":"))
    return hour * 60 + minute


def hour_label(hour: int) -> str:
    return f"{hour:02d}:00"


class GridCell(QFrame):
    """One empty hour. Clicking it offers to create a class then (FRONTEND.md 'Empty slots')."""

    clicked = Signal(object, str)  # the date, "HH:00"

    def __init__(self, day: date, hour: int, parent=None):
        super().__init__(parent)
        self.day = day
        self.hour = hour
        self.setObjectName("GridCell")
        self.setMinimumHeight(HOUR_HEIGHT)
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Ignored)
        self.setCursor(Qt.PointingHandCursor)

    @property
    def day_index(self) -> int:
        return self.day.weekday()

    @property
    def start_time(self) -> str:
        return hour_label(self.hour)

    def set_dropping(self, active: bool) -> None:
        if self.property("dropping") == ("true" if active else "false"):
            return
        self.setProperty("dropping", "true" if active else "false")
        self.style().unpolish(self)
        self.style().polish(self)

    def mouseReleaseEvent(self, event):  # noqa: N802 - Qt naming
        if event.button() == Qt.LeftButton:
            self.clicked.emit(self.day, self.start_time)
        super().mouseReleaseEvent(event)


class GridCanvas(QWidget):
    """The drop surface.

    Drops are handled here rather than on each cell because a card sits *on top of* the hours it
    covers and would otherwise swallow the event. The canvas maps the drop position onto an hour
    cell instead.
    """

    # lesson id, the date it was dropped on, "HH:00", and the date it was dragged from (None when
    # the drag did not say -- then the whole lesson is moved, as it always was)
    lesson_dropped = Signal(int, object, str, object)
    resized = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("GridCanvas")
        self.setAcceptDrops(True)
        self._cells: list[GridCell] = []
        self._hover: GridCell | None = None

    def resizeEvent(self, event):  # noqa: N802 - Qt naming
        super().resizeEvent(event)
        self.resized.emit()

    def cells(self) -> list[GridCell]:
        return list(self._cells)

    def set_cells(self, cells: list[GridCell]) -> None:
        self._cells = cells
        self._hover = None

    def _cell_at(self, position) -> GridCell | None:
        point = position.toPoint() if hasattr(position, "toPoint") else position
        for cell in self._cells:
            if cell.geometry().contains(point):
                return cell
        return None

    def _highlight(self, cell: GridCell | None) -> None:
        if self._hover is cell:
            return
        if self._hover is not None:
            self._hover.set_dropping(False)
        self._hover = cell
        if cell is not None:
            cell.set_dropping(True)

    def dragEnterEvent(self, event):  # noqa: N802 - Qt naming
        if event.mimeData().hasFormat(LESSON_MIME):
            event.acceptProposedAction()

    def dragMoveEvent(self, event):  # noqa: N802 - Qt naming
        if not event.mimeData().hasFormat(LESSON_MIME):
            return
        self._highlight(self._cell_at(event.position()))
        event.acceptProposedAction()

    def dragLeaveEvent(self, event):  # noqa: N802 - Qt naming
        self._highlight(None)
        super().dragLeaveEvent(event)

    def dropEvent(self, event):  # noqa: N802 - Qt naming
        cell = self._cell_at(event.position())
        self._highlight(None)
        if cell is None or not event.mimeData().hasFormat(LESSON_MIME):
            return
        try:
            lesson_id, source_day = read_drag_payload(event.mimeData().data(LESSON_MIME))
        except ValueError:
            return
        event.acceptProposedAction()
        self.lesson_dropped.emit(lesson_id, cell.day, cell.start_time, source_day)


class WeekGrid(QScrollArea):
    """Renders one group's week. Owns no state -- it is rebuilt from storage on every change."""

    # Every signal carries the date it happened on: a class that repeats is the same lesson id on
    # every one of its dates, so the id alone no longer says which occurrence was meant.
    lesson_clicked = Signal(int, object)         # lesson id, date -- left click: join the class
    lesson_menu_requested = Signal(int, object)  # lesson id, date -- right click: the menu
    slot_clicked = Signal(object, str)           # date, "HH:00"
    lesson_dropped = Signal(int, object, str, object)  # lesson id, date, "HH:00", source date

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("GridScroll")
        self.setWidgetResizable(True)
        self.setHorizontalScrollBarPolicy(Qt.ScrollBarAsNeeded)
        self._canvas = GridCanvas()
        self._canvas.lesson_dropped.connect(self.lesson_dropped.emit)
        self._layout = QGridLayout(self._canvas)
        self._layout.setContentsMargins(0, 0, 0, 0)
        self._layout.setSpacing(0)
        self.setWidget(self._canvas)
        self._dates: list[date] = []
        self._days: list[int] = []  # the weekday of each column

        # The "now" line is not in the layout: it floats over the canvas at the pixel the clock
        # says, so ``_clear`` (which empties the layout) never has to know it exists.
        self._now_line = QFrame(self._canvas)
        self._now_line.setObjectName("NowLine")
        self._now_line.setFixedHeight(2)
        self._now_badge = QLabel(self._canvas)
        self._now_badge.setObjectName("NowBadge")
        # Half strength: the marker is a guide, and must not drown the card it crosses. An opacity
        # effect dims fill and text together (a separate effect each: one cannot be shared).
        for marker in (self._now_line, self._now_badge):
            effect = QGraphicsOpacityEffect(marker)
            effect.setOpacity(NOW_OPACITY)
            marker.setGraphicsEffect(effect)
        self._now_line.hide()
        self._now_badge.hide()
        self._canvas.resized.connect(self._place_now)
        self._now_timer = QTimer(self)
        self._now_timer.setInterval(NOW_REFRESH_MS)
        self._now_timer.timeout.connect(self._place_now)
        self._now_timer.start()

    # ------------------------------------------------------------------ render

    def render_week(
        self,
        lessons: list[Lesson],
        statuses: dict[int, str] | None = None,
        next_lesson_id: int | None = None,
        today: date | None = None,
        monday: date | None = None,
    ) -> None:
        """Draw the week that starts on ``monday`` (the one containing ``today`` by default).

        ``statuses`` is keyed by lesson id: inside one week a weekly class happens once, so the id
        names the occurrence. ``render_days`` is keyed by (id, date) for the views where it does
        not.
        """
        today = today or date.today()
        monday = monday or week_start(today)
        days = [monday + timedelta(days=offset) for offset in range(6)]
        sunday = monday + timedelta(days=6)
        if any(lesson.occurs_on(sunday) for lesson in lessons):  # only when something is on it
            days.append(sunday)

        by_id = {lesson.id: lesson for lesson in lessons}
        dated = {
            (lesson_id, (monday + timedelta(days=by_id[lesson_id].day_index)).isoformat()): status
            for lesson_id, status in (statuses or {}).items()
            if lesson_id in by_id
        }
        self.render_days(
            days,
            lessons,
            dated,
            (next_lesson_id, today.isoformat()) if next_lesson_id is not None else None,
            today,
        )

    def render_days(
        self,
        days: list[date],
        lessons: list[Lesson],
        statuses: dict[tuple[int, str], str] | None = None,
        next_key: tuple[int, str] | None = None,
        today: date | None = None,
    ) -> None:
        """Draw one column per date in ``days``; each lesson lands on every date it occurs on.

        ``statuses`` and ``next_key`` are keyed by ``(lesson id, ISO date)`` because a repeating
        class is one lesson on many dates and each date has its own verdict.
        """
        statuses = statuses or {}
        today = today or date.today()
        self._clear()

        self._dates = list(days)
        self._days = [day.weekday() for day in self._dates]

        self._build_headers(today)
        self._build_gutter()
        self._build_cells(today)
        self._place_lessons(lessons, statuses, next_key)
        self._pin_canvas_size()
        # The layout has not been given its rectangles yet, so the line is placed once it has.
        QTimer.singleShot(0, self, self._place_now)

        # A grid layout remembers a column's stretch and minimum width after the column's widgets
        # are gone. Going from a week to a day left columns two to six still claiming their share
        # of the width, and the one real column was a sixth of the screen wide.
        for column in range(self._layout.columnCount()):
            self._layout.setColumnStretch(column, 0)
            self._layout.setColumnMinimumWidth(column, 0)
        for column in range(1, len(self._days) + 1):
            self._layout.setColumnStretch(column, 1)
            self._layout.setColumnMinimumWidth(column, COLUMN_MIN_WIDTH)
        self._layout.setColumnMinimumWidth(0, GUTTER_WIDTH)
        # Spare vertical space goes below the last hour, so every row stays exactly HOUR_HEIGHT
        # and the one-minute-per-pixel identity holds.
        self._layout.setRowStretch(len(HOURS) + 1, 1)

    def _pin_canvas_size(self) -> None:
        """Tell the canvas how big the grid needs to be, rather than leave it to the layout.

        A scroll area sizes its widget from the widget's minimum size *when something resizes it*.
        The layout's own minimum is not reliable at that moment: while a week is being built the
        layout is briefly empty, answers "726 px, no width" (just the rows), and the scroll area
        keeps that -- so every hour came out 60 px, the header was cut through and the cards were
        a pixel too narrow for their chips. The size is not a mystery anyway: a header and one
        pinned row per hour, a gutter and a minimum width per column.
        """
        self._canvas.setMinimumSize(
            GUTTER_WIDTH + COLUMN_MIN_WIDTH * len(self._dates),
            HEADER_HEIGHT + len(HOURS) * HOUR_HEIGHT,
        )

    def _clear(self) -> None:
        """Empty the grid without ever detaching a widget from its parent.

        ``setParent(None)`` on a live widget makes it a **top-level window** for the moment
        between the rebuild and the event loop running ``deleteLater``. Rebuilding a full week
        that way threw dozens of stray top-levels at the window manager, which is what flashed
        small empty windows across the screen whenever the grid reloaded -- most visibly right
        after clicking a class, because opening the link triggers a reload. Hiding and deleting
        keeps every widget a child of the canvas until it is gone.
        """
        self._canvas.set_cells([])
        while self._layout.count():
            item = self._layout.takeAt(0)
            widget = item.widget()
            if widget is not None:
                widget.hide()
                widget.deleteLater()

    def _build_headers(self, today: date) -> None:
        """A header over the gutter and one over each column.

        A week names each day over its own column, centred. A single day has one very wide column,
        and a name floating in the middle of it is a long way from the hours it belongs to; there
        the name and the date sit in the corner over the time gutter, the way a day is drawn in a
        calendar app, and the column's own header is left empty.
        """
        single = len(self._dates) == 1
        corner = self._header(self._dates[0] if single else None, today, short=True)
        self._layout.addWidget(corner, 0, 0)
        for column, day in enumerate(self._dates, start=1):
            self._layout.addWidget(self._header(None if single else day, today), 0, column)

    def _header(self, day: date | None, today: date, short: bool = False) -> QFrame:
        """One header cell: the weekday over the date in a circle, or empty when ``day`` is None."""
        header = QFrame()
        header.setObjectName("DayHeader")
        header.setFixedHeight(HEADER_HEIGHT)
        if day is None:
            return header
        is_today = day == today
        header.setProperty("today", "true" if is_today else "false")

        box = QVBoxLayout(header)
        box.setContentsMargins(8, 10, 8, 6)
        box.setSpacing(2)

        name = QLabel(DAY_SHORT[day.weekday()] if short else DAY_NAMES[day.weekday()])
        name.setObjectName("DayName")
        name.setProperty("today", "true" if is_today else "false")
        name.setAlignment(Qt.AlignCenter)

        number = QLabel(str(day.day))
        number.setObjectName("DayDate")
        number.setProperty("today", "true" if is_today else "false")
        number.setAlignment(Qt.AlignCenter)
        number.setFixedSize(DATE_CIRCLE, DATE_CIRCLE)
        number.setToolTip(f"{day.day} {MONTH_GENITIVE[day.month - 1]} {day.year}")

        box.addWidget(name)
        box.addWidget(number, 0, Qt.AlignHCenter)
        return header

    def _build_gutter(self) -> None:
        for row, hour in enumerate(HOURS, start=1):
            cell = QFrame()
            cell.setObjectName("TimeGutter")
            cell.setFixedWidth(GUTTER_WIDTH)
            cell.setMinimumHeight(HOUR_HEIGHT)
            cell.setSizePolicy(QSizePolicy.Fixed, QSizePolicy.Ignored)
            box = QVBoxLayout(cell)
            box.setContentsMargins(8, 4, 10, 4)
            box.setSpacing(0)
            # The label sits on the hour line, the way a calendar reads.
            label = QLabel(hour_label(hour))
            label.setObjectName("HourLabel")
            label.setAlignment(Qt.AlignRight | Qt.AlignTop)
            box.addWidget(label)
            box.addStretch(1)
            self._layout.addWidget(cell, row, 0)
            self._layout.setRowMinimumHeight(row, HOUR_HEIGHT)
            self._layout.setRowStretch(row, 0)

    def _build_cells(self, today: date) -> None:
        cells: list[GridCell] = []
        for row, hour in enumerate(HOURS, start=1):
            for column, day in enumerate(self._dates, start=1):
                cell = GridCell(day, hour)
                is_today = day == today
                cell.setProperty("today", "true" if is_today else "false")
                cell.setProperty("dropping", "false")
                cell.setToolTip(
                    f"{DAY_NAMES[day.weekday()]}, {day.day} {MONTH_GENITIVE[day.month - 1]} · "
                    f"{hour_label(hour)}"
                    "\nКлацніть, щоб створити пару"
                )
                cell.clicked.connect(self.slot_clicked.emit)
                self._layout.addWidget(cell, row, column)
                cells.append(cell)
        self._canvas.set_cells(cells)

    # -------------------------------------------------------------- placement

    @staticmethod
    def span_for(start_time: str, end_time: str) -> tuple[int, int, int, int]:
        """Where a class sits: ``(first_row, row_span, top_margin_min, bottom_margin_min)``.

        Rows are 1-based to match the layout, whose row 0 is the header. The margins are the
        **minutes** the class does not use at either end of the hours it spans -- which is what
        lets a card cover half of one cell and half of the next. They are minutes rather than
        pixels so this stays a pure function of the timetable; ``minutes_to_pixels`` turns them
        into a margin at the one place that draws.
        """
        start = max(DAY_START_MINUTES, min(to_minutes(start_time), DAY_END_MINUTES - 1))
        end = min(DAY_END_MINUTES, max(to_minutes(end_time), start + MIN_CARD_MINUTES))

        first_hour = (start - DAY_START_MINUTES) // 60
        last_hour = (end - DAY_START_MINUTES - 1) // 60
        span = last_hour - first_hour + 1

        top = start - DAY_START_MINUTES - first_hour * 60
        bottom = (last_hour + 1) * 60 - (end - DAY_START_MINUTES)
        return first_hour + 1, span, top, max(bottom, 1)

    def _place_lessons(
        self,
        lessons: list[Lesson],
        statuses: dict[tuple[int, str], str],
        next_key: tuple[int, str] | None,
    ) -> None:
        for column, day in enumerate(self._dates, start=1):
            for lesson in lessons:
                if lesson.occurs_on(day):
                    self._place(lesson, day, column, statuses, next_key)

    def _place(
        self,
        lesson: Lesson,
        day: date,
        column: int,
        statuses: dict[tuple[int, str], str],
        next_key: tuple[int, str] | None,
    ) -> None:
        key = (lesson.id, day.isoformat())
        row, span, top, bottom = self.span_for(lesson.start_time, lesson.end_time)

        top_px, bottom_px = minutes_to_pixels(top), minutes_to_pixels(bottom)
        card = ClassCard(
            lesson,
            status=statuses.get(key),
            is_next=key == next_key,
            height=span * HOUR_HEIGHT - top_px - bottom_px,
        )
        card.day = day
        card.clicked.connect(lambda lesson_id, day=day: self.lesson_clicked.emit(lesson_id, day))
        card.menu_requested.connect(
            lambda lesson_id, day=day: self.lesson_menu_requested.emit(lesson_id, day)
        )
        container = QWidget()
        container.setObjectName("CardSlot")
        box = QVBoxLayout(container)
        box.setContentsMargins(4, top_px, 4, bottom_px)
        box.setSpacing(0)
        box.addWidget(card)
        # The card must not be allowed to argue with the clock. An Ignored vertical policy is
        # not enough on its own: QGridLayout still honours a *spanning* item's
        # minimumSizeHint, so a card whose text needed more room than its class lasts pushed
        # the rows it covered apart -- 60 px hours quietly became 106 px ones, and every card
        # below them drifted off the time it was supposed to sit on. A fixed height is the
        # only answer the layout cannot argue with: the container is exactly as tall as the
        # class is long, and text that does not fit is clipped, as it is in any calendar.
        # Ignored in both directions: a card's text must not decide how wide its column is, or the
        # week comes out in columns of different widths depending on which subject is longest.
        container.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Ignored)
        container.setFixedHeight(span * HOUR_HEIGHT)
        self._layout.addWidget(container, row, column, span, 1)

    # --------------------------------------------------------------- now line

    def _place_now(self, now: datetime | None = None) -> None:
        """Lay the "now" line across today's column, at the minute the clock says.

        Hidden whenever there is nothing honest to show: today is not one of the columns, or the
        time is outside the hours the grid draws. ``now`` exists for tests; the timer passes none.
        """
        now = now or datetime.now()
        minutes = now.hour * 60 + now.minute
        today = now.date()
        if today not in self._dates or not (DAY_START_MINUTES <= minutes < DAY_END_MINUTES):
            self._now_line.hide()
            self._now_badge.hide()
            return
        # Anchored on the real cell for today's first hour: its geometry is what is on screen,
        # where asking the layout for a cell rectangle answered before the rows had been sized.
        anchor = next(
            (
                cell
                for cell in self._canvas.cells()
                if cell.day == today and cell.hour == FIRST_HOUR
            ),
            None,
        )
        if anchor is None or anchor.width() <= 0:  # not laid out yet
            return
        y = anchor.geometry().top() + minutes_to_pixels(minutes - DAY_START_MINUTES)

        self._now_line.setGeometry(anchor.geometry().left(), y - 1, anchor.width(), 2)
        self._now_badge.setText(f"{now.hour:02d}:{now.minute:02d}")
        self._now_badge.adjustSize()
        self._now_badge.move(
            GUTTER_WIDTH - self._now_badge.width() - 4, y - self._now_badge.height() // 2
        )
        self._now_line.show()
        self._now_badge.show()
        self._now_line.raise_()
        self._now_badge.raise_()
