"""Motion: sliding between days, weeks and months, and fading popups in.

Both are decoration, so both follow the same rules. They never delay or change what the window does
-- the state has already changed when the first frame of a transition is drawn -- they stop at once
when something newer happens, and they are skipped altogether where nobody could see them (a window
that is not on screen) or where they are switched off (``ENABLED``, which the test suite turns off
so that nothing waits on a clock).

* **Between periods** the calendar surface is photographed before the change and again after it,
  and an overlay slides one picture out while the other slides in, in the direction of travel. A
  picture is used rather than animating the live widgets because the live ones are rebuilt from the
  database on every change: there is nothing steady to move.
* **Popups** (menus, tooltips, dialogs) fade in by animating the window's own opacity.
"""

from __future__ import annotations

from PySide6.QtCore import QEasingCurve, QPropertyAnimation, QTimer, QVariantAnimation, Qt, Signal
from PySide6.QtGui import QColor, QPainter, QPixmap
from PySide6.QtWidgets import QWidget

from ..core import theme

ENABLED = True

SLIDE_MS = 240
SLIDE_PX = 36          # how far the pictures travel; they also fade, so a short way reads as motion
FADE_MS = 140          # a popup appearing


class SlideOverlay(QWidget):
    """Two pictures of the same surface, one leaving and one arriving, drawn over the real one.

    ``direction`` is +1 when time moves forward (the new picture comes from the right), -1 for
    backward, and 0 for a change of *view*, which has no direction and simply cross-fades.
    """

    finished = Signal()

    def __init__(self, parent: QWidget, old: QPixmap, direction: int):
        super().__init__(parent)
        self.setAttribute(Qt.WA_TransparentForMouseEvents, True)
        self._old = old
        self._new: QPixmap | None = None
        self._direction = direction
        self._progress = 0.0
        self._canvas = QColor(theme.token("canvas"))
        self._animation: QVariantAnimation | None = None

    # -------------------------------------------------------------- progress

    @property
    def progress(self) -> float:
        return self._progress

    def set_progress(self, value: float) -> None:
        self._progress = max(0.0, min(1.0, float(value)))
        self.update()

    def run(self, new: QPixmap) -> None:
        """Start moving. ``new`` is the picture of the surface after the change."""
        self._new = new
        animation = QVariantAnimation(self)
        animation.setStartValue(0.0)
        animation.setEndValue(1.0)
        animation.setDuration(SLIDE_MS)
        animation.setEasingCurve(QEasingCurve.OutCubic)
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

    # ----------------------------------------------------------------- paint

    def paintEvent(self, event):  # noqa: N802 - Qt naming
        painter = QPainter(self)
        painter.setRenderHint(QPainter.SmoothPixmapTransform)
        painter.fillRect(self.rect(), self._canvas)
        t = self._progress
        if self._new is None:
            painter.drawPixmap(0, 0, self._old)
            return
        shift = self._direction * SLIDE_PX
        if self._direction == 0:
            # A change of view dissolves one picture into the other: the old stays whole beneath.
            old_alpha, new_alpha = 1.0, t
        else:
            # A slide hands over rather than superimposes: the old picture is gone by the middle
            # and the new one starts a little before it, so two weeks of text are never both at
            # full strength on top of each other.
            old_alpha = max(0.0, 1.0 - t / 0.5)
            new_alpha = min(1.0, max(0.0, (t - 0.25) / 0.75))
        painter.setOpacity(old_alpha)
        painter.drawPixmap(round(-shift * t), 0, self._old)
        painter.setOpacity(new_alpha)
        painter.drawPixmap(round(shift * (1.0 - t)), 0, self._new)


def play(surface: QWidget, old: QPixmap, direction: int) -> SlideOverlay | None:
    """Cover ``surface`` with the picture it had before, then slide to what it shows now.

    The overlay appears at once with the old picture, which hides the already-rebuilt widgets
    beneath it; the new picture is taken one event-loop turn later, when the layout has settled.
    """
    if not ENABLED:
        return None
    parent = surface.parentWidget()
    if parent is None:
        return None
    overlay = SlideOverlay(parent, old, direction)
    overlay.setGeometry(surface.geometry())
    overlay.show()
    overlay.raise_()

    def start() -> None:
        try:
            overlay.run(surface.grab())
        except RuntimeError:  # the overlay was finished (and deleted) before the layout settled
            pass

    QTimer.singleShot(0, overlay, start)
    return overlay


def fade_in(widget: QWidget, duration: int = FADE_MS) -> QPropertyAnimation | None:
    """Raise a top-level window from transparent to opaque. Does nothing where motion is off."""
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
