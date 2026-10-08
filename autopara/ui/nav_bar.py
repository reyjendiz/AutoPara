"""The strip above the calendar: which period is on screen, and the way to move it.

It owns no dates. The window tells it what to say (``set_title``) and what the arrows do
(``set_tooltips``); it only reports presses. Keeping it that dumb is what lets the same bar serve
a day, a week and a month.
"""

from __future__ import annotations

from datetime import date, timedelta

from PySide6.QtCore import QEasingCurve, QPropertyAnimation, QSize, Qt, QTimer, Signal
from PySide6.QtWidgets import (
    QButtonGroup,
    QFrame,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QWidget,
)

from ..core import theme
from ..core.models import VIEW_DAY, VIEW_MONTH, VIEW_WEEK
from ..importer.normalize import MONTH_GENITIVE, MONTH_NAMES
from . import icons, transitions


def week_parts(monday: date) -> tuple[str, str]:
    """A week as ``(what, year)``: ``("5–11 жовтня", "2026")``.

    The year is returned apart because the bar sets it quieter than the rest. A week that spans
    New Year names both years, so there the year is part of the first half and the second year is
    the quiet one.
    """
    sunday = monday + timedelta(days=6)
    first, last = MONTH_GENITIVE[monday.month - 1], MONTH_GENITIVE[sunday.month - 1]
    if monday.year != sunday.year:
        return f"{monday.day} {first} {monday.year} – {sunday.day} {last}", str(sunday.year)
    if monday.month != sunday.month:
        return f"{monday.day} {first} – {sunday.day} {last}", str(sunday.year)
    return f"{monday.day}–{sunday.day} {last}", str(sunday.year)


def week_title(monday: date) -> str:
    """``5–11 жовтня 2026``, ``28 вересня – 4 жовтня 2026`` or across a new year."""
    return " ".join(part for part in week_parts(monday) if part)


def month_title(day: date) -> str:
    return f"{MONTH_NAMES[day.month - 1]} {day.year}"


VIEW_LABELS = ((VIEW_DAY, "День"), (VIEW_WEEK, "Тиждень"), (VIEW_MONTH, "Місяць"))

THUMB_MS = 220  # how long the selected pill takes to travel to the next segment
NEXT_UP_MIN_WIDTH = 1000  # narrower than this the bar has no room to say what is next


class SegmentedFrame(QFrame):
    """The pill that holds the view segments; says when its size (and so theirs) has settled."""

    settled = Signal()

    def resizeEvent(self, event):  # noqa: N802 - Qt naming
        super().resizeEvent(event)
        self.settled.emit()

    def showEvent(self, event):  # noqa: N802 - Qt naming
        super().showEvent(event)
        self.settled.emit()


class NavBar(QWidget):
    previous_requested = Signal()
    next_requested = Signal()
    today_requested = Signal()
    view_requested = Signal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("NavBar")
        row = QHBoxLayout(self)
        row.setContentsMargins(0, 0, 0, 0)
        row.setSpacing(8)
        row.setAlignment(Qt.AlignVCenter)

        self.title = QLabel("")
        self.title.setObjectName("NavTitle")
        row.addWidget(self.title)
        self.sub = QLabel("")
        self.sub.setObjectName("NavSub")
        row.addWidget(self.sub)

        # What is on, or next, today -- a quiet pill after the title; it goes first when space runs out.
        self.next_up = QLabel("")
        self.next_up.setObjectName("NextUp")
        self.next_up.setFixedHeight(28)
        self.next_up.hide()
        self._next_text = ""
        row.addSpacing(6)
        row.addWidget(self.next_up)
        row.addStretch(1)

        # The segmented control: the selected segment is the black pill, like every active thing.
        segmented = SegmentedFrame()
        segmented.setObjectName("Segmented")
        # 32 px segments, 3 px of air and a 1 px border on each side: exactly 40, so the radius of
        # 20 in the stylesheet is exactly half of it.
        segmented.setFixedHeight(40)
        segments = QHBoxLayout(segmented)
        segments.setContentsMargins(3, 3, 3, 3)
        segments.setSpacing(2)
        self._views = QButtonGroup(self)
        self._views.setExclusive(True)
        self.view_buttons: dict[str, QPushButton] = {}
        for mode, label in VIEW_LABELS:
            button = QPushButton(label)
            button.setObjectName("SegButton")
            button.setCheckable(True)
            button.clicked.connect(lambda _checked=False, mode=mode: self.view_requested.emit(mode))
            self._views.addButton(button)
            segments.addWidget(button)
            self.view_buttons[mode] = button

        # The selected pill is a widget of its own, behind the segments, so that it can *travel*
        # from one to the next instead of the fill jumping between two buttons.
        self._thumb = QFrame(segmented)
        self._thumb.setObjectName("SegThumb")
        self._thumb.lower()
        self._thumb.hide()
        self._glide: QPropertyAnimation | None = None
        segmented.settled.connect(self._park_thumb)
        self._views.buttonToggled.connect(self._toggled)
        row.addWidget(segmented)
        row.addSpacing(8)

        self.today_button = QPushButton("Сьогодні")
        self.today_button.setToolTip("Повернутися до сьогодні")
        self.today_button.clicked.connect(self.today_requested.emit)
        row.addWidget(self.today_button)

        self.previous_button = self._arrow(self.previous_requested.emit)
        self.next_button = self._arrow(self.next_requested.emit)
        row.addWidget(self.previous_button)
        row.addWidget(self.next_button)

        self.repaint_glyphs()

    def _arrow(self, handler) -> QPushButton:
        """A round icon button; its meaning is in the tooltip, like every other icon here."""
        button = QPushButton("")
        button.setObjectName("NavButton")
        button.setFixedSize(36, 36)
        button.setIconSize(QSize(18, 18))
        button.clicked.connect(handler)
        return button

    def set_next_up(self, text: str, tooltip: str = "") -> None:
        """``text`` is what to say about the class that is on or next; empty hides the pill."""
        self._next_text = text
        self.next_up.setText(text)
        self.next_up.setToolTip(tooltip)
        self._fit_next_up()

    def _fit_next_up(self) -> None:
        self.next_up.setVisible(bool(self._next_text) and self.width() >= NEXT_UP_MIN_WIDTH)

    def resizeEvent(self, event):  # noqa: N802 - Qt naming
        super().resizeEvent(event)
        self._fit_next_up()

    def showEvent(self, event):  # noqa: N802 - Qt naming
        super().showEvent(event)
        self._fit_next_up()

    def set_title(self, text: str, quiet: str = "") -> None:
        """``text`` is the period; ``quiet`` (usually the year) follows it in a lighter weight."""
        self.title.setText(text)
        self.sub.setText(quiet)
        self.sub.setVisible(bool(quiet))

    # ------------------------------------------------------------ the sliding pill

    def _checked(self) -> QPushButton | None:
        return next((b for b in self.view_buttons.values() if b.isChecked()), None)

    def _mark(self, current: QPushButton | None) -> None:
        """Light the text of the segment the pill has reached (and only that one)."""
        for button in self.view_buttons.values():
            wanted = "true" if button is current else "false"
            if button.property("on") != wanted:
                button.setProperty("on", wanted)
                button.style().unpolish(button)
                button.style().polish(button)

    def _park_thumb(self) -> None:
        """Put the pill exactly behind the chosen segment, with no travel (a resize, first show)."""
        if self._glide is not None:
            self._glide.stop()
            self._glide = None
        button = self._checked()
        if button is None or button.width() <= 0:
            self._thumb.hide()
            return
        self._thumb.setGeometry(button.geometry())
        self._thumb.show()
        self._thumb.lower()
        self._mark(button)

    def _toggled(self, button: QPushButton, checked: bool) -> None:
        if not checked:
            return
        travels = transitions.ENABLED and self.isVisible() and self._thumb.isVisible()
        if not travels:
            QTimer.singleShot(0, self, self._park_thumb)  # after the layout has placed the segment
            self._park_thumb()
            return
        if self._glide is not None:
            self._glide.stop()
        # The text over the pill has to stay readable all the way: the segment it left keeps its
        # lit text until the pill is nearer the new one, and then the new one takes over. Pale text
        # on the arriving pill (or dark text on the bare bar) would be unreadable for a moment.
        previous = self._thumb.geometry().center().x()
        goal = button.geometry().center().x()
        glide = QPropertyAnimation(self._thumb, b"geometry", self)
        glide.setStartValue(self._thumb.geometry())
        glide.setEndValue(button.geometry())
        glide.setDuration(THUMB_MS)
        glide.setEasingCurve(QEasingCurve.OutCubic)

        def moved(rect) -> None:
            if abs(rect.center().x() - goal) < abs(rect.center().x() - previous):
                self._mark(button)

        glide.valueChanged.connect(moved)
        glide.finished.connect(lambda: self._mark(self._checked()))
        self._glide = glide
        glide.start()

    def set_view(self, mode: str) -> None:
        """Mark the segment for ``mode`` -- without announcing it, since the window asked."""
        button = self.view_buttons.get(mode)
        if button is not None:
            button.setChecked(True)

    def set_tooltips(self, previous: str, following: str) -> None:
        self.previous_button.setToolTip(previous)
        self.next_button.setToolTip(following)

    def repaint_glyphs(self) -> None:
        ink = theme.token("text")
        self.previous_button.setIcon(icons.icon("chevron_left", ink, 18))
        self.next_button.setIcon(icons.icon("chevron_right", ink, 18))
