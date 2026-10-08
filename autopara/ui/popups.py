"""Rounded tooltips and menus.

Qt will not round the corners of a popup from a stylesheet: the popup is a window, and a window's
corners are square unless it is translucent. So the two popups the interface uses constantly are
made translucent here and draw their own rounded shape.

* **Tooltips.** The native one ignores ``border-radius``. A tooltip request is intercepted before
  Qt shows it and answered with ``RoundedTip`` instead, which is frameless and translucent and paints
  its own plate in the palette's tooltip colours.
* **Menus.** Every ``QMenu`` is made frameless and translucent just before it is first shown, and
  the stylesheet's ``border-radius`` then has something to round.

Menus, dialogs and tooltips also **grow into place** (``transitions.morph_in``) as they appear: out of
the pointer for a menu or a tip, out of their middle for a dialog.

Both are installed once, as an application-wide event filter, so no widget needs to know.
"""

from __future__ import annotations

from PySide6.QtCore import QEvent, QObject, QPoint, QRectF, Qt, QTimer
from PySide6.QtGui import QColor, QCursor, QGuiApplication, QPainter
from PySide6.QtWidgets import QApplication, QDialog, QLabel, QMenu, QVBoxLayout, QWidget

from ..core import theme
from . import transitions

TIP_RADIUS = 10
TIP_PADDING = (10, 7)       # horizontal, vertical
TIP_MAX_WIDTH = 360
TIP_OFFSET = QPoint(14, 20)  # from the pointer, so the tip does not sit under it
TIP_LIFETIME_MS = 12_000

# Events after which a tooltip has no business being on screen.
_DISMISSING = {
    QEvent.Leave,
    QEvent.MouseButtonPress,
    QEvent.MouseButtonDblClick,
    QEvent.Wheel,
    QEvent.KeyPress,
    QEvent.WindowDeactivate,
    QEvent.ApplicationDeactivate,
    QEvent.FocusOut,
}


class RoundedTip(QWidget):
    """A tooltip that paints its own rounded plate onto a translucent window."""

    def __init__(self):
        super().__init__(
            None,
            Qt.ToolTip | Qt.FramelessWindowHint | Qt.NoDropShadowWindowHint,
        )
        self.setAttribute(Qt.WA_TranslucentBackground, True)
        self.setAttribute(Qt.WA_ShowWithoutActivating, True)
        self.setAttribute(Qt.WA_TransparentForMouseEvents, True)
        column = QVBoxLayout(self)
        column.setContentsMargins(*TIP_PADDING, *TIP_PADDING)
        self.label = QLabel("")
        self.label.setWordWrap(True)
        column.addWidget(self.label)
        self._expiry = QTimer(self)
        self._expiry.setSingleShot(True)
        self._expiry.timeout.connect(self.hide)

    def show_text(self, text: str, near: QPoint) -> None:
        palette = theme.PALETTES[theme.active()]
        self.label.setStyleSheet(
            f"background: transparent; color: {palette['tooltip_fg']}; "
            f"font-size: {9 * theme.point_scale():.1f}pt;"
        )
        self.label.setText(text)
        self.label.setMaximumWidth(TIP_MAX_WIDTH)
        self.label.ensurePolished()
        self.layout().activate()
        self.resize(self.sizeHint())

        position = near + TIP_OFFSET
        screen = QGuiApplication.screenAt(near) or QGuiApplication.primaryScreen()
        if screen is not None:
            area = screen.availableGeometry()
            position.setX(max(area.left(), min(position.x(), area.right() - self.width())))
            if position.y() + self.height() > area.bottom():
                position.setY(near.y() - self.height() - 8)  # flip above the pointer
        self.move(position)
        if not self.isVisible():  # moving from one tooltip to the next should not blink
            transitions.morph_in(self, near)
        self.show()
        self.raise_()
        self._expiry.start(TIP_LIFETIME_MS)

    def paintEvent(self, event):  # noqa: N802 - Qt naming
        painter = QPainter(self)
        painter.setRenderHint(QPainter.Antialiasing)
        painter.setPen(Qt.NoPen)
        painter.setBrush(QColor(theme.PALETTES[theme.active()]["tooltip_bg"]))
        painter.drawRoundedRect(QRectF(self.rect()), TIP_RADIUS, TIP_RADIUS)


class PopupStyler(QObject):
    """The application-wide filter that swaps in the rounded tooltip and rounds every menu."""

    def __init__(self, parent: QObject | None = None):
        super().__init__(parent)
        self.tip = RoundedTip()
        self._busy = False

    def eventFilter(self, obj, event):  # noqa: N802 - Qt naming
        # Only ever one pass at a time: showing a tip or fading a window sends events of its own,
        # which come back through this same filter. And while the application is being torn down
        # the tip may already be gone, which must not turn into an error per event.
        if self._busy:
            return False
        self._busy = True
        try:
            return self._filter(obj, event)
        except RuntimeError:  # a Qt object is already deleted: the process is shutting down
            return False
        finally:
            self._busy = False

    def _filter(self, obj, event) -> bool:
        if isinstance(obj, transitions.Ghost):
            return False  # the picture a popup grows from is not a window anyone is using
        kind = event.type()
        if kind == QEvent.ToolTip and isinstance(obj, QWidget):
            text = obj.toolTip()
            if not text:
                return False  # Qt will offer the event to the parent, which may have one
            self.tip.show_text(text, event.globalPos())
            return True
        if kind in _DISMISSING and self.tip.isVisible():
            self.tip.hide()
        elif kind == QEvent.Polish and isinstance(obj, QMenu):
            self._round_menu(obj)
        elif kind == QEvent.Show and isinstance(obj, (QMenu, QDialog)):
            # A window growing out of where it belongs, not appearing in one frame: a menu from the
            # pointer that opened it, a dialog from its middle.
            transitions.morph_in(obj, QCursor.pos() if isinstance(obj, QMenu) else None)
        return False

    @staticmethod
    def _round_menu(menu: QMenu) -> None:
        if menu.property("rounded"):
            return
        menu.setProperty("rounded", True)
        menu.setWindowFlag(Qt.FramelessWindowHint, True)
        menu.setAttribute(Qt.WA_TranslucentBackground, True)


_styler: PopupStyler | None = None


def install(app: QApplication) -> PopupStyler:
    """Install the filter once per process and return it."""
    global _styler
    if _styler is None:
        _styler = PopupStyler(app)
        app.installEventFilter(_styler)
    return _styler
