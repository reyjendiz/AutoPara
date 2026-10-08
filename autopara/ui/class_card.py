"""A single lesson rendered inside the week grid.

The card is both a click target (it opens the actions menu) and a drag source: dragging it onto
another grid cell moves the class to that day and slot. The two must not fight each other, so a
press only counts as a click when the pointer never travelled far enough to start a drag.
"""

from __future__ import annotations

import hashlib
from datetime import date

from urllib.parse import urlparse

from PySide6.QtCore import QMimeData, QPoint, QSize, Qt, Signal
from PySide6.QtGui import QDrag, QFontMetrics
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QLabel,
    QSizePolicy,
    QVBoxLayout,
)

from ..core import theme
from ..core.models import (
    PROVIDER_MEET,
    PROVIDER_ZOOM,
    STATUS_MANUAL,
    STATUS_MISSED,
    STATUS_OPENED,
    STATUS_SKIPPED,
    Lesson,
)

PROVIDER_LABEL = {PROVIDER_ZOOM: "Zoom", PROVIDER_MEET: "Meet", "unknown": "Посилання"}

# The drag payload is just the lesson id; the grid looks the lesson up in storage on drop, so a
# stale card can never carry stale lesson data across.
LESSON_MIME = "application/x-autopara-lesson"


def drag_payload(lesson_id: int, day: date | None) -> bytes:
    """What a dragged card carries: its lesson and, when known, the date it was dragged from."""
    return f"{lesson_id}|{day.isoformat()}".encode("ascii") if day else str(lesson_id).encode("ascii")


def read_drag_payload(raw: bytes) -> tuple[int, date | None]:
    """The inverse of :func:`drag_payload`; the date is None for a bare lesson id."""
    text = bytes(raw).decode("ascii")
    ident, _, stamp = text.partition("|")
    try:
        return int(ident), (date.fromisoformat(stamp) if stamp else None)
    except ValueError:
        raise ValueError(f"not a lesson drag: {text!r}") from None

# The card's own padding. Named because the height budget in ``_fit`` has to agree with the
# layout exactly -- a card that thinks it is 2 px smaller than it is clips a line for nothing.
CARD_MARGIN_H = 10
CARD_MARGIN_LEFT = 17  # leaves room for the subject bar
CARD_MARGIN_V = 4
CARD_SPACING = 2
CHIP_SPACING = 5

# The bar down the card's left edge: set in from the edge so it never follows the corner's curve.
BAR_INSET_X = 6
BAR_INSET_Y = 9
BAR_WIDTH = 3

STATE_TEXT = {
    "opened": "✓ відкрито",
    "missed": "не відкрито",
    "skipped": "пропущено",
    "nolink": "без посилання",
}

# A subject always hashes to the same entry, so its colour is stable across sessions and re-imports
# (docs/FRONTEND.md). The seven pastels of the design language -- coral, lemon, sky, violet, mint,
# pink and dusty blue -- read the same on a white card and on a dark one, so a subject keeps its
# colour between themes. The colour is only ever a bar and a chip, never behind text, which is what
# lets a pale yellow be one of them.
SUBJECT_COLORS = [
    "#ec6b66", "#f1f36a", "#86cdf7", "#8566e6", "#8fe8a8", "#f26c9c", "#a8bdd6",
]


def subject_color(subject: str) -> str:
    digest = hashlib.md5(subject.strip().casefold().encode("utf-8")).hexdigest()
    return SUBJECT_COLORS[int(digest[:8], 16) % len(SUBJECT_COLORS)]


def clamp_lines(text: str, metrics: QFontMetrics, width: int, max_lines: int) -> str:
    """``text`` wrapped to ``width`` and cut to ``max_lines`` lines, the last ending in an ellipsis.

    A card is exactly as tall as its class is long, so a long subject cannot be shown in full. What
    it must not do is be *cut*: a wrapped label whose height is capped shows the middle of its text
    with the top and bottom lines sliced through. This keeps whole lines and says, with "…", that
    there is more -- the tooltip has the rest.
    """
    if width <= 0 or max_lines <= 0:
        return text
    lines: list[str] = []
    current = ""
    for word in text.split():
        trial = f"{current} {word}".strip()
        if metrics.horizontalAdvance(trial) <= width:
            current = trial
            continue
        if current:
            lines.append(current)
        current = word
        # A word wider than the whole line is broken where it runs out of room.
        while metrics.horizontalAdvance(current) > width and len(current) > 1:
            cut = len(current) - 1
            while cut > 1 and metrics.horizontalAdvance(current[:cut]) > width:
                cut -= 1
            lines.append(current[:cut])
            current = current[cut:]
    if current:
        lines.append(current)
    if len(lines) <= max_lines:
        return "\n".join(lines)
    kept = lines[:max_lines]
    rest = " ".join(lines[max_lines - 1 :])
    kept[-1] = metrics.elidedText(rest, Qt.ElideRight, width)
    return "\n".join(kept)


class ClampedLabel(QLabel):
    """A label that shows at most ``max_lines`` whole lines of its text, ending in "…" if cut.

    It asks for no width of its own (a column's width must not depend on its longest subject) and
    re-wraps whenever it is resized.
    """

    def __init__(self, text: str, max_lines: int, parent=None):
        super().__init__(parent)
        self._full = text
        self._max_lines = max_lines
        self.setAlignment(Qt.AlignLeft | Qt.AlignTop)
        self.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        self.setText(text)

    def minimumSizeHint(self) -> QSize:  # noqa: N802 - Qt naming
        return QSize(0, super().minimumSizeHint().height())

    def resizeEvent(self, event):  # noqa: N802 - Qt naming
        super().resizeEvent(event)
        text = clamp_lines(self._full, self.fontMetrics(), self.width(), self._max_lines)
        if text != self.text():
            self.setText(text)
            # A new height is only *scheduled* by setText; until it runs the second line of a
            # subject is laid out into a one-line box and cut off.
            parent = self.parentWidget()
            if parent is not None and parent.layout() is not None:
                parent.layout().activate()


def groups_word(count: int) -> str:
    """Ukrainian plural for 'group': 2-4 групи, otherwise груп."""
    if count % 10 in (2, 3, 4) and count % 100 not in (12, 13, 14):
        return "групи"
    return "груп"


class ClassCard(QFrame):
    """Shows subject, teacher, time, provider and state. Emits on click, drags to move."""

    clicked = Signal(int)        # lesson id -- left click: join the class
    menu_requested = Signal(int)  # lesson id -- right click: the actions menu

    def __init__(
        self,
        lesson: Lesson,
        status: str | None = None,
        is_next: bool = False,
        parent=None,
        height: int | None = None,
    ):
        """``height`` is how many pixels the grid will actually give this card.

        The card is exactly as tall as its class is long, so a long subject cannot be shown in
        full and something has to give. Told the height, the card decides *what* to leave out --
        and leaves out whole lines. Left to the layout it overflowed instead, and a card cut
        through the middle of a word looks like a rendering fault rather than a calendar.
        """
        super().__init__(parent)
        self.lesson = lesson
        self.day: date | None = None  # the date this card is drawn on; the grid sets it
        self.status = status
        self._height = height
        self.setObjectName("ClassCard")
        self.setCursor(Qt.PointingHandCursor)
        self.setSizePolicy(QSizePolicy.Expanding, QSizePolicy.Expanding)
        self._press_at: QPoint | None = None
        self._dragging = False
        self._apply_state(is_next)
        self._build()
        self._build_bar()

    # ------------------------------------------------------------------ state

    def _apply_state(self, is_next: bool) -> None:
        if self.status in (STATUS_OPENED, STATUS_MANUAL):
            state = "opened"
        elif self.status == STATUS_SKIPPED:
            state = "skipped"
        elif self.status == STATUS_MISSED:
            state = "missed"
        elif self.lesson.needs_link:
            state = "nolink"
        elif is_next:
            state = "next"
        else:
            state = "normal"
        self.state = state
        self.setProperty("state", state)

    def _build_bar(self) -> None:
        """The subject's colour as a short bar set in from the card's left edge.

        A bar of its own rather than a coloured ``border-left``: that follows the card's rounded
        corner and reads as a bent stripe, and on a dashed card it came out dashed too.
        """
        colour = theme.token("danger") if self.state == "missed" else subject_color(
            self.lesson.subject
        )
        self._bar = QFrame(self)
        self._bar.setObjectName("CardBar")
        self._bar.setStyleSheet(f"background: {colour};")
        self._bar.setAttribute(Qt.WA_TransparentForMouseEvents)
        self._place_bar()

    def _place_bar(self) -> None:
        self._bar.setGeometry(
            BAR_INSET_X, BAR_INSET_Y, BAR_WIDTH, max(self.height() - 2 * BAR_INSET_Y, 0)
        )
        self._bar.raise_()

    def resizeEvent(self, event):  # noqa: N802 - Qt naming
        super().resizeEvent(event)
        self._place_bar()
        self._fit_chips()

    def _fit_chips(self) -> None:
        """Show as many chips as the card is wide enough for, most important first.

        A column is about 150 px, and a chip squeezed below its text reads "Zoon" and "✓від" -- a
        wrong word is worse than a missing one. The first chip always stays; the tooltip carries
        everything a hidden chip would have said.
        """
        room = self.width() - CARD_MARGIN_LEFT - CARD_MARGIN_H
        used = 0
        for position, chip in enumerate(self._chips):
            need = chip.sizeHint().width() + (CHIP_SPACING if used else 0)
            fits = position == 0 or used + need <= room
            chip.setVisible(fits)
            if fits:
                used += need
        # Hiding a chip only *schedules* a new layout, and until it runs the chips that stay are
        # still as narrow as when all of them were competing for the room.
        self.layout().activate()

    # ------------------------------------------------------------------ layout

    # Ranked by what a timetable is for: the subject first, then who teaches it, and last the
    # time -- which the card's own position on the grid already says, and the tooltip repeats.
    # The first arrangement that fits the card's height is the one it gets.
    #
    # (subject lines, teacher, time, detail). The detail line -- who shares the class, or where the
    # link leads -- only fits on a tall card, i.e. in the day view where an hour is given room.
    LAYOUTS = (
        (3, True, True, True),
        (3, True, True, False),
        (2, True, True, False),
        (2, True, False, False),
        (2, False, True, False),
        (2, False, False, False),
        (1, True, True, False),
        (1, True, False, False),
        (1, False, True, False),
        (1, False, False, False),
    )

    def _fit(
        self, line_heights: tuple[int, int, int, int]
    ) -> tuple[int, bool, bool, bool]:
        """Pick the richest of ``LAYOUTS`` that fits, or the barest one if none does."""
        top_height, subject_line, teacher_line, time_height = line_heights
        if self._height is None:
            return self.LAYOUTS[2]
        for subject_lines, with_teacher, with_time, with_detail in self.LAYOUTS:
            rows = 2 + int(with_teacher) + int(with_time) + int(with_detail)
            needed = (
                CARD_MARGIN_V * 2
                + top_height
                + subject_lines * subject_line
                + (teacher_line if with_teacher else 0)
                + (time_height if with_time else 0)
                + (teacher_line if with_detail else 0)
                + (rows - 1) * CARD_SPACING
            )
            if needed <= self._height:
                return subject_lines, with_teacher, with_time, with_detail
        return self.LAYOUTS[-1]

    def _detail_text(self) -> str:
        """The extra line of a tall card: the groups that share the class, else where it is held."""
        if len(self.lesson.group_names) > 1:
            return "Групи: " + ", ".join(self.lesson.group_names)
        if self.lesson.url:
            return urlparse(self.lesson.url).netloc or self.lesson.url
        return ""

    def _build(self) -> None:
        layout = QVBoxLayout(self)
        # Tight, because the card only ever gets as many pixels as its class lasts: the standard
        # 80-minute pair is 88 px, and every pixel spent on padding is a line of the subject that
        # does not fit. See docs/FRONTEND.md, "One minute, a fixed number of pixels".
        layout.setContentsMargins(CARD_MARGIN_LEFT, CARD_MARGIN_V, CARD_MARGIN_H, CARD_MARGIN_V)
        layout.setSpacing(CARD_SPACING)

        top = QHBoxLayout()
        top.setSpacing(CHIP_SPACING)
        # A lesson with no link has no provider to name; its state chip already says so.
        badge = None
        if self.lesson.url:
            badge = QLabel(PROVIDER_LABEL.get(self.lesson.provider, PROVIDER_LABEL["unknown"]))
            badge.setObjectName("CardBadge")
            badge.setProperty("provider", self.lesson.provider)

        state_badge = None
        state_text = STATE_TEXT.get(self.state)
        if state_text:
            state_badge = QLabel(state_text)
            state_badge.setObjectName("StateBadge")
            state_badge.setProperty("state", self.state)

        # The group chip rides on the badge row rather than beside the time, so a card too short
        # for a time row still says that the class is shared.
        group_chip = None
        count = len(self.lesson.group_names)
        if count > 1:
            group_chip = QLabel(f"{count} {groups_word(count)}")
            group_chip.setObjectName("GroupChip")
            group_chip.setToolTip("Спільна пара: " + ", ".join(self.lesson.group_names))

        # In the order they are given up when the card is too narrow for all of them: what became
        # of the class matters most, then where it is held, and last how many groups share it.
        self._chips = [chip for chip in (state_badge, badge, group_chip) if chip is not None]
        # Every widget on the badge row, so its height is measured rather than guessed from the
        # provider pill: a state badge or a group chip can be the tallest thing on it, and a row
        # two pixels taller than the budget expected clips the line at the bottom of the card.
        top_widgets = list(self._chips)
        for chip in (badge, state_badge, group_chip):
            if chip is not None:
                top.addWidget(chip)
        top.addStretch(1)
        layout.addLayout(top)

        subject = ClampedLabel(self.lesson.subject, 1)
        subject.setObjectName("CardSubject")
        subject.setProperty("muted", "true" if self.state in ("opened", "skipped") else "false")

        teacher = None
        if self.lesson.teacher:
            teacher = ClampedLabel(self.lesson.teacher.strip("()"), 1)
            teacher.setObjectName("CardTeacher")

        time_label = QLabel(f"{self.lesson.start_time}–{self.lesson.end_time}")
        time_label.setObjectName("CardTime")

        detail = None
        if self._detail_text():
            detail = ClampedLabel(self._detail_text(), 1)
            detail.setObjectName("CardTeacher")

        # Полірування -- це те, що застосовує таблицю стилів, а без неї шрифт іще не той, яким
        # напис справді малюватиметься, і всі виміри були б від іншого розміру.
        for widget in (*top_widgets, subject, teacher, time_label):
            if widget is not None:
                widget.ensurePolished()

        subject_lines, with_teacher, with_time, with_detail = self._fit(
            (
                max((widget.sizeHint().height() for widget in top_widgets), default=0),
                subject.fontMetrics().lineSpacing(),
                teacher.fontMetrics().lineSpacing() if teacher else 0,
                time_label.sizeHint().height(),
            )
        )

        # Whole lines only, the last ending in "…" when the subject runs on (ClampedLabel).
        subject._max_lines = subject_lines
        layout.addWidget(subject)

        if teacher is not None and with_teacher:
            layout.addWidget(teacher)
        if detail is not None and with_detail:
            layout.addWidget(detail)
        elif detail is not None:
            detail.deleteLater()

        layout.addStretch(1)

        if with_time:
            layout.addWidget(time_label)
        else:
            time_label.deleteLater()

        tooltip = [self.lesson.subject]
        if self.lesson.teacher:
            tooltip.append(self.lesson.teacher.strip("()"))
        tooltip.append(f"{self.lesson.start_time}–{self.lesson.end_time}")
        if self.lesson.group_names:
            tooltip.append("Групи: " + ", ".join(self.lesson.group_names))
        tooltip.append(self.lesson.url if self.lesson.url else "Без посилання")
        # The card is trimmed to the length of its class, so the tooltip is where the whole of a
        # long subject, a full teacher's name and the exact times stay reachable.
        tooltip.append("Клац — підключитися · правий клац — меню · перетягніть, щоб перенести")
        self.setToolTip("\n".join(tooltip))

    # ------------------------------------------------------------------ events

    def mousePressEvent(self, event):  # noqa: N802 - Qt naming
        if event.button() == Qt.LeftButton:
            self._press_at = event.position().toPoint()
            self._dragging = False
        super().mousePressEvent(event)

    def mouseMoveEvent(self, event):  # noqa: N802 - Qt naming
        if self._press_at is None or not (event.buttons() & Qt.LeftButton):
            return super().mouseMoveEvent(event)
        travelled = (event.position().toPoint() - self._press_at).manhattanLength()
        if travelled < self.startDragDistance():
            return super().mouseMoveEvent(event)

        self._dragging = True
        data = QMimeData()
        data.setData(LESSON_MIME, drag_payload(self.lesson.id, self.day))
        drag = QDrag(self)
        drag.setMimeData(data)
        drag.setPixmap(self.grab())
        drag.setHotSpot(self._press_at)
        drag.exec(Qt.MoveAction)
        return None

    def mouseReleaseEvent(self, event):  # noqa: N802 - Qt naming
        if event.button() == Qt.LeftButton and not self._dragging:
            self.clicked.emit(self.lesson.id)
        self._press_at = None
        self._dragging = False
        super().mouseReleaseEvent(event)

    def contextMenuEvent(self, event):  # noqa: N802 - Qt naming
        """Right click asks for the actions menu.

        Handled here rather than in mouseReleaseEvent so the keyboard's Menu key works too, and so
        Qt does not also deliver the event to the grid underneath.
        """
        event.accept()
        self.menu_requested.emit(self.lesson.id)

    @staticmethod
    def startDragDistance() -> int:  # noqa: N802 - mirrors Qt's own naming
        from PySide6.QtWidgets import QApplication

        return QApplication.startDragDistance()
