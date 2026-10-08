"""Motion: everything that changes shape does so by morphing, not by cutting or sliding.

All of it is decoration, so all of it follows the same rules. It never delays or changes what the
window does -- the state has already changed when the first frame of a transition is drawn -- it
stops at once when something newer happens, and it is skipped altogether where nobody could see it
(a window that is not on screen) or where it is switched off (``ENABLED``, which the test suite
turns off so that nothing waits on a clock).

* **Between periods and views** the calendar surface is photographed before the change and again
  after it, *in two layers*: the empty calendar (grid, headers, dates) and every class on it, each
  with where it sat. An overlay then morphs one into the other. A class that is on both sides -- the
  same lesson in the same place of the next week, or the same lesson as a chip in the month and a
  card in the week -- is one element: it glides and resizes from where it was to where it is, its
  picture dissolving into the new one on the way. A class on one side only grows in or shrinks out.
  The empty calendar cross-fades, drifting a few pixels in the direction time is moving. A picture
  is used rather than animating the live widgets because the live ones are rebuilt from the
  database on every change: there is nothing steady to move.
* **Popups** (menus, dialogs, tooltips) grow out of the point they belong to -- the pointer for a
  menu or a tip, the middle for a dialog -- from a slightly smaller picture of themselves while
  becoming opaque.
* **The selected pill** of the view control travels between segments (see ``nav_bar``) and **a round
  "+" button** eases its hover fill in and out (``FadeButton``).
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import date

from PySide6.QtCore import (
    QEasingCurve,
    QPoint,
    QPointF,
    QPropertyAnimation,
    QRect,
    QRectF,
    QTimer,
    QVariantAnimation,
    Qt,
    Signal,
)
from PySide6.QtGui import QColor, QPainter, QPainterPath, QPixmap
from PySide6.QtWidgets import QPushButton, QWidget

from ..core import theme

ENABLED = True

MORPH_MS = 320         # a calendar changing period or view
DRIFT_PX = 14          # how far the new calendar drifts in; it also fades, so a short way reads
FADE_MS = 140          # a popup appearing (it also grows: see ``morph_in``)
POPUP_FROM = 0.90      # a popup starts at this fraction of its size
SCALE_OUT = 0.90       # a class that is only on one side shrinks to / grows from this fraction


@dataclass
class Element:
    """One class on the calendar: which lesson, on which date, where it sat and what it looked like."""

    lesson_id: int
    day: date | None
    rect: QRect
    picture: QPixmap


@dataclass
class Snapshot:
    """The calendar in two layers: the empty calendar, and the classes on it."""

    background: QPixmap
    elements: list[Element] = field(default_factory=list)
    first_day: date | None = None  # the first date on screen: what "the same place" is measured from


def snapshot(surface: QWidget) -> Snapshot:
    """Photograph ``surface`` as an empty calendar plus the classes that sit on it.

    The surface says which of its children are classes (``morph_elements``); everything else is
    background. The classes are photographed one by one, then hidden for a moment while the rest is
    photographed, and shown again before anything can be painted -- the hide and the show happen in
    one call, and a hidden widget only asks its layout to look again later, so nothing moves.
    """
    ask = getattr(surface, "morph_elements", None)
    found = list(ask()) if ask else []
    elements: list[Element] = []
    hidden: list[QWidget] = []
    for widget, lesson_id, day in found:
        if not widget.isVisible():
            continue
        rect = QRect(widget.mapTo(surface, QPoint(0, 0)), widget.size())
        hidden.append(widget)
        if rect.intersects(surface.rect()):
            elements.append(Element(lesson_id, day, rect, widget.grab()))
    for widget in hidden:
        widget.setVisible(False)
    try:
        background = surface.grab()
    finally:
        for widget in hidden:
            widget.setVisible(True)
    first = getattr(surface, "morph_first_day", None)
    return Snapshot(background, elements, first() if first else None)


def _lerp(a: float, b: float, t: float) -> float:
    return a + (b - a) * t


def _lerp_rect(a: QRect, b: QRect, t: float) -> QRectF:
    return QRectF(
        _lerp(a.x(), b.x(), t), _lerp(a.y(), b.y(), t),
        _lerp(a.width(), b.width(), t), _lerp(a.height(), b.height(), t),
    )


def _scaled(rect: QRect, factor: float) -> QRectF:
    """``rect`` scaled about its own centre."""
    width, height = rect.width() * factor, rect.height() * factor
    centre = QPointF(rect.center()) + QPointF(0.5, 0.5)
    return QRectF(centre.x() - width / 2, centre.y() - height / 2, width, height)


class MorphOverlay(QWidget):
    """Two snapshots of the same surface, one becoming the other, drawn over the real one.

    ``direction`` is +1 when time moves forward (the new calendar drifts in from the right), -1 for
    backward, and 0 for a change of *view*, which has no direction and does not drift.
    ``same_view`` says the two snapshots are of the same kind of view, one period apart: a class
    then stays itself if it holds the same *place* on screen (the same lesson on the same weekday of
    the next week). Across views the same lesson on the same *date* is the same class instead.
    """

    finished = Signal()

    def __init__(self, parent: QWidget, old: Snapshot, direction: int, same_view: bool = True):
        super().__init__(parent)
        self.setAttribute(Qt.WA_TransparentForMouseEvents, True)
        self._old = old
        self._new: Snapshot | None = None
        self._direction = direction
        self._same_view = same_view
        self._progress = 0.0
        self._canvas = QColor(theme.token("canvas"))
        self._animation: QVariantAnimation | None = None
        self._pairs: list[tuple[Element, Element]] = []
        self._leaving: list[Element] = []
        self._arriving: list[Element] = []

    # -------------------------------------------------------------- progress

    @property
    def progress(self) -> float:
        return self._progress

    def set_progress(self, value: float) -> None:
        self._progress = max(0.0, min(1.0, float(value)))
        self.update()

    def run(self, new: Snapshot) -> None:
        """Start morphing. ``new`` is the snapshot of the surface after the change."""
        self._new = new
        self._match()
        animation = QVariantAnimation(self)
        animation.setStartValue(0.0)
        animation.setEndValue(1.0)
        animation.setDuration(MORPH_MS)
        animation.setEasingCurve(QEasingCurve.InOutCubic)
        animation.valueChanged.connect(self.set_progress)
        animation.finished.connect(self.finish)
        self._animation = animation
        animation.start()

    def finish(self) -> None:
        """Stop where we are and go away: the real surface underneath is already the new one."""
        if self._animation is not None:
            self._animation.stop()
            self._animation = None
        self.hide()
        self.deleteLater()
        self.finished.emit()

    # -------------------------------------------------------------- matching

    def _key(self, element: Element, snap: Snapshot):
        if self._same_view and snap.first_day is not None and element.day is not None:
            return element.lesson_id, (element.day - snap.first_day).days
        return element.lesson_id, element.day

    def _match(self) -> None:
        """Pair each class on the old calendar with its twin on the new one, if it has one."""
        assert self._new is not None
        arriving = {self._key(e, self._new): e for e in self._new.elements}
        self._pairs, self._leaving = [], []
        for element in self._old.elements:
            twin = arriving.pop(self._key(element, self._old), None)
            if twin is None:
                self._leaving.append(element)
            else:
                self._pairs.append((element, twin))
        self._arriving = list(arriving.values())

    # ----------------------------------------------------------------- paint

    @staticmethod
    def _draw(painter: QPainter, picture: QPixmap, rect: QRectF, opacity: float) -> None:
        if opacity <= 0.0 or rect.width() < 1 or rect.height() < 1:
            return
        painter.setOpacity(opacity)
        painter.drawPixmap(rect, picture, QRectF(picture.rect()))

    @staticmethod
    def _body_colour(element: Element) -> QColor:
        """The colour of a class's card, read from beside its middle where there is no text."""
        image = element.picture.toImage()
        x = max(0, image.width() - 6)
        y = min(max(0, image.height() // 2), max(0, image.height() - 1))
        return image.pixelColor(min(x, max(0, image.width() - 1)), y)

    def _draw_pair(self, painter: QPainter, before: Element, after: Element, e: float) -> None:
        """A container transform: the box grows from one shape to the other, filled with the card's
        colour, while the two pictures stay at their own size inside it and dissolve into each other.

        Scaling a picture of text to a different shape smears it, and the two shapes of one class can
        be very different (a month's chip and a day's card), so nothing is stretched: the box moves
        and the words fade, which is also what reads as one thing becoming another.
        """
        box = _lerp_rect(before.rect, after.rect, e)
        if box.width() < 1 or box.height() < 1:
            return
        fill = QColor(self._body_colour(before))
        target = self._body_colour(after)
        fill = QColor(
            round(_lerp(fill.red(), target.red(), e)),
            round(_lerp(fill.green(), target.green(), e)),
            round(_lerp(fill.blue(), target.blue(), e)),
        )
        path = QPainterPath()
        radius = min(10.0, box.width() / 2, box.height() / 2)
        path.addRoundedRect(box, radius, radius)
        painter.save()
        painter.setOpacity(1.0)
        painter.setClipPath(path)
        painter.fillRect(box, fill)
        for picture, opacity in ((before.picture, 1.0 - e), (after.picture, e)):
            if opacity > 0.0:
                painter.setOpacity(opacity)
                painter.drawPixmap(box.topLeft(), picture)
        painter.restore()

    def paintEvent(self, event):  # noqa: N802 - Qt naming
        painter = QPainter(self)
        painter.setRenderHint(QPainter.SmoothPixmapTransform)
        painter.setRenderHint(QPainter.Antialiasing)
        painter.fillRect(self.rect(), self._canvas)
        e = self._progress
        painter.drawPixmap(0, 0, self._old.background)
        if self._new is None:
            for element in self._old.elements:
                self._draw(painter, element.picture, QRectF(element.rect), 1.0)
            return

        # The empty calendar: the old one stays whole beneath and the new one dissolves in over it,
        # drifting from the side time is coming from, so two calendars of text are never both
        # struck through each other at half strength.
        painter.setOpacity(e)
        painter.drawPixmap(round(self._direction * DRIFT_PX * (1.0 - e)), 0, self._new.background)

        for element in self._leaving:  # goes quickly: it is already gone by the middle
            self._draw(
                painter, element.picture, _scaled(element.rect, _lerp(1.0, SCALE_OUT, e)),
                max(0.0, 1.0 - e / 0.5),
            )
        for before, after in self._pairs:  # the same class: one box that changes shape
            self._draw_pair(painter, before, after, e)
        for element in self._arriving:  # starts a little before the middle
            self._draw(
                painter, element.picture, _scaled(element.rect, _lerp(SCALE_OUT, 1.0, e)),
                min(1.0, max(0.0, (e - 0.35) / 0.65)),
            )


def play(
    surface: QWidget, old: Snapshot, direction: int, same_view: bool = True
) -> MorphOverlay | None:
    """Cover ``surface`` with the picture it had before, then morph to what it shows now.

    The overlay appears at once with the old picture, which hides the already-rebuilt widgets
    beneath it; the new snapshot is taken one event-loop turn later, when the layout has settled.
    """
    if not ENABLED:
        return None
    parent = surface.parentWidget()
    if parent is None:
        return None
    overlay = MorphOverlay(parent, old, direction, same_view)
    overlay.setGeometry(surface.geometry())
    overlay.show()
    overlay.raise_()

    def start() -> None:
        try:
            overlay.run(snapshot(surface))
        except RuntimeError:  # the overlay was finished (and deleted) before the layout settled
            pass

    QTimer.singleShot(0, overlay, start)
    return overlay


def fade_in(widget: QWidget, duration: int = FADE_MS) -> QPropertyAnimation | None:
    """Raise a top-level window from transparent to opaque. Does nothing where motion is off.

    The plain fallback of :func:`morph_in`, and what a window gets when a picture of it cannot be
    taken.
    """
    if not ENABLED or not widget.isWindow():
        return None
    previous = getattr(widget, "_fade", None)
    if previous is not None:
        previous.stop()
    widget.setWindowOpacity(0.0)
    animation = QPropertyAnimation(widget, b"windowOpacity", widget)
    animation.setStartValue(0.0)
    animation.setEndValue(1.0)
    animation.setDuration(duration)
    animation.setEasingCurve(QEasingCurve.OutCubic)
    widget._fade = animation
    animation.start()
    return animation


class Ghost(QWidget):
    """A picture of a popup that grows into place while the popup itself is still invisible.

    A top-level window cannot be scaled, and resizing a real one re-lays out its contents on every
    frame. So the popup is photographed, a click-through translucent window of its own paints that
    photograph growing from ``origin`` to full size, and only when it is done is the real popup made
    opaque in the same spot -- the two are the same picture, so the hand-over cannot be seen.
    """

    def __init__(self, picture: QPixmap, rect: QRect, origin: QPointF, widget: QWidget):
        flags = (
            Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
            | Qt.WindowTransparentForInput | Qt.NoDropShadowWindowHint
        )
        super().__init__(None, flags)
        self.setAttribute(Qt.WA_TranslucentBackground, True)
        self.setAttribute(Qt.WA_ShowWithoutActivating, True)
        self.setAttribute(Qt.WA_DeleteOnClose, True)
        self._picture = picture
        self._origin = origin  # in this window's own coordinates
        self._progress = 0.0
        self._widget = widget
        self.setGeometry(rect)

    @property
    def progress(self) -> float:
        return self._progress

    def set_progress(self, value: float) -> None:
        self._progress = max(0.0, min(1.0, float(value)))
        self.update()

    def current_rect(self) -> QRectF:
        """Where the picture is now: scaled about ``origin`` from ``POPUP_FROM`` up to its own size."""
        scale = _lerp(POPUP_FROM, 1.0, self._progress)
        full = QRectF(self.rect())
        return QRectF(
            self._origin.x() + (full.x() - self._origin.x()) * scale,
            self._origin.y() + (full.y() - self._origin.y()) * scale,
            full.width() * scale,
            full.height() * scale,
        )

    def paintEvent(self, event):  # noqa: N802 - Qt naming
        painter = QPainter(self)
        painter.setRenderHint(QPainter.SmoothPixmapTransform)
        painter.setOpacity(self._progress)
        painter.drawPixmap(self.current_rect(), self._picture, QRectF(self._picture.rect()))


def _is_frameless(widget: QWidget) -> bool:
    """True for a window that is all content: a menu, a tip, anything without a title bar.

    A photograph of a window holds its contents and not its frame, so only these can be grown from
    a picture: with a native frame the title bar would pop in at the very end.
    """
    return bool(widget.windowFlags() & Qt.FramelessWindowHint) or widget.windowType() in (
        Qt.Popup, Qt.ToolTip,
    )


def _close_ghost(widget: QWidget) -> None:
    ghost = getattr(widget, "_ghost", None)
    widget._ghost = None
    if ghost is not None:
        try:
            ghost.close()
        except RuntimeError:  # it closed itself already
            pass


def morph_in(widget: QWidget, origin: QPoint | None = None) -> QVariantAnimation | None:
    """Bring a popup in by growing it out of ``origin`` (global coordinates; default: its middle).

    Does nothing where motion is off. A window with a native frame (a dialog) only fades in, since
    its frame cannot be photographed, and so does a popup that cannot be photographed (it has no size
    yet) -- a popup that will not appear is a far worse bug than one that appears without ceremony.
    """
    if not ENABLED or not widget.isWindow():
        return None
    previous = getattr(widget, "_morph", None)
    if previous is not None:
        previous.stop()
    _close_ghost(widget)
    if not _is_frameless(widget):
        return fade_in(widget)
    rect = widget.geometry()
    if rect.width() < 2 or rect.height() < 2:
        return fade_in(widget)
    picture = widget.grab()
    if picture.isNull():
        return fade_in(widget)

    if origin is None:
        point = QPointF(rect.center())
    else:  # a pointer outside the popup (it was flipped to fit the screen): grow from its nearest edge
        point = QPointF(
            max(rect.left(), min(origin.x(), rect.right())),
            max(rect.top(), min(origin.y(), rect.bottom())),
        )
    ghost = Ghost(picture, rect, point - QPointF(rect.topLeft()), widget)
    widget.setWindowOpacity(0.0)

    animation = QVariantAnimation(widget)
    animation.setStartValue(0.0)
    animation.setEndValue(1.0)
    animation.setDuration(FADE_MS + 60)
    animation.setEasingCurve(QEasingCurve.OutCubic)
    animation.valueChanged.connect(ghost.set_progress)

    def arrived() -> None:
        try:
            widget.setWindowOpacity(1.0)  # the real popup first, then the picture of it goes
            _close_ghost(widget)
        except RuntimeError:  # the popup was destroyed while it was appearing
            try:
                ghost.close()
            except RuntimeError:
                pass

    animation.finished.connect(arrived)
    widget._morph = animation
    widget._ghost = ghost
    ghost.show()
    ghost.raise_()
    animation.start()
    return animation


HOVER_MS = 220         # a round button's fill easing in or out under the pointer


class FadeButton(QPushButton):
    """A round button whose hover fill fades in and out instead of switching.

    QSS has no ``transition``, so the fill is painted here: the stylesheet leaves the button
    transparent and ``paintEvent`` draws a disc in a colour mixed between the resting and the
    hovered token, ``HOVER_MS`` long, ease-in-out. The two colours are close in tone on purpose,
    so what moves is a hint, not a jump. Tokens are read at paint time, so a theme change needs
    nothing from here. ``rest=None`` is a button with no fill at rest.
    """

    def __init__(self, text: str = "", rest: str | None = None, hover: str = "chip_bg",
                 pressed: str = "border", parent: QWidget | None = None):
        super().__init__(text, parent)
        self._rest, self._hover, self._pressed = rest, hover, pressed
        self._level = 0.0
        self._fade: QVariantAnimation | None = None

    def _glide_to(self, goal: float) -> None:
        if self._fade is not None:
            self._fade.stop()
            self._fade = None
        if not ENABLED or not self.isVisible():
            self._level = goal
            self.update()
            return
        fade = QVariantAnimation(self)
        fade.setStartValue(self._level)
        fade.setEndValue(goal)
        fade.setDuration(HOVER_MS)
        fade.setEasingCurve(QEasingCurve.InOutQuad)
        fade.valueChanged.connect(self._set_level)
        self._fade = fade
        fade.start()

    def _set_level(self, value) -> None:
        self._level = float(value)
        self.update()

    def _fill(self) -> QColor:
        if self.isDown():
            return QColor(theme.token(self._pressed))
        hover = QColor(theme.token(self._hover))
        if self._rest is None:
            hover.setAlphaF(self._level)  # fade the tint in rather than blending through black
            return hover
        rest, t = QColor(theme.token(self._rest)), self._level
        return QColor(
            round(rest.red() + (hover.red() - rest.red()) * t),
            round(rest.green() + (hover.green() - rest.green()) * t),
            round(rest.blue() + (hover.blue() - rest.blue()) * t),
        )

    def enterEvent(self, event):  # noqa: N802 - Qt naming
        self._glide_to(1.0)
        super().enterEvent(event)

    def leaveEvent(self, event):  # noqa: N802 - Qt naming
        self._glide_to(0.0)
        super().leaveEvent(event)

    def hideEvent(self, event):  # noqa: N802 - Qt naming
        # A hidden button never hears the pointer leave; come back at rest.
        self._glide_to(0.0)
        super().hideEvent(event)

    def paintEvent(self, event):  # noqa: N802 - Qt naming
        fill = self._fill()
        if fill.alpha():
            painter = QPainter(self)
            painter.setRenderHint(QPainter.Antialiasing)
            painter.setPen(Qt.NoPen)
            painter.setBrush(fill)
            radius = min(self.width(), self.height()) / 2
            painter.drawRoundedRect(self.rect(), radius, radius)
            painter.end()
        super().paintEvent(event)
