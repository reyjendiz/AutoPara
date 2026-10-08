"""Light / dark colour schemes.

``ui/styles.qss`` is a template rather than a finished stylesheet: it names colours as ``$tokens``
which this module substitutes from one of two palettes. One file means a rule can never exist in
the light theme and be forgotten in the dark one.

The stored ``theme`` setting is ``system`` | ``light`` | ``dark``. ``system`` -- what a fresh
install uses -- reads Windows' own "app mode" from the registry, so AutoPara comes up matching the
desktop it was installed on.
"""

from __future__ import annotations

import logging
import re
import sys
from pathlib import Path
from string import Template

from .models import THEME_DARK, THEME_LIGHT, THEME_SYSTEM

log = logging.getLogger(__name__)

STYLESHEET = Path(__file__).resolve().parents[1] / "ui" / "styles.qss"

PERSONALIZE_KEY = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"

# Inter ships with the app (``ui/fonts``) under the SIL Open Font License, so it is redistributed
# like any other open font and nobody has to have it installed. The system faces after it are what
# each platform falls back to if the bundled files ever fail to load. Windows is the exception: it
# is set in its own Segoe UI, and Inter is only the fallback there. macOS (and anything else) keeps
# Inter first.
FONT_STACK = ("Inter", "Segoe UI Variable Text", "Segoe UI", "-apple-system", "Helvetica Neue", "Arial")
WINDOWS_FONT_STACK = ("Segoe UI", "Segoe UI Variable Text", "Inter", "Arial")


def font_stack() -> tuple[str, ...]:
    """The families to try, in order, on this platform."""
    return WINDOWS_FONT_STACK if sys.platform == "win32" else FONT_STACK


try:  # winreg exists only on Windows; keep the module importable elsewhere for tests.
    import winreg
except ImportError:  # pragma: no cover - non-Windows
    winreg = None


PALETTES: dict[str, dict[str, str]] = {
    # The look is "Education Hub": white cards on a soft grey canvas, one near-black anchor for
    # everything that is active or primary, and pastel colour only where it means something (a
    # subject, a status). So the accent is *ink*, not a hue: ``accent_fill`` is the black pill and
    # ``accent_text`` is what is written on it. The dark theme inverts that pair rather than
    # inventing a second accent. Values are opaque hex: Qt's stylesheet parser is inconsistent
    # about alpha, and the same translucent grey would composite differently on a card, on a
    # tinted cell and on the rail.
    THEME_LIGHT: {
        "canvas": "#f5f5f6",          # the window: the soft grey the cards sit on
        "bg": "#ffffff",
        "surface": "#ffffff",         # a card, a dialog, an input
        "sunken": "#f5f5f6",          # inner panels and recessed groups
        "text": "#0e0e10",
        "text_muted": "#6b6b75",
        "text_faint": "#7e7e88",
        "border": "#e6e6ea",
        "border_soft": "#ececef",
        "grid_line": "#e2e2e7",
        "accent": "#0e0e10",          # borders, indicators, focus rings
        "accent_fill": "#0e0e10",     # the black pill
        "accent_hover": "#2b2b30",
        "accent_ink": "#0e0e10",      # the accent used as text
        "accent_soft": "#efeff1",     # a selected row
        "accent_text": "#ffffff",     # what is written on the black pill
        "today_bg": "#fafafb",
        "now_bg": "#fafafb",
        "card_bg": "#ffffff",
        "opened_bg": "#fafafb",
        "missed_bg": "#fffafa",
        "nolink_bg": "#fffdf4",
        "skipped_bg": "#fafafb",
        "chip_bg": "#f0f0f2",
        "success_bg": "#dff4e5",
        "success_fg": "#1f7a47",
        "danger": "#e5534d",
        "danger_bg": "#fdecea",
        "danger_fg": "#c23a30",
        "warn_bg": "#fff3c9",
        "warn_fg": "#7d6200",
        "zoom_bg": "#e1f0fd",
        "zoom_fg": "#1f67ad",
        "meet_bg": "#dff4e5",
        "meet_fg": "#1f7a47",
        "tooltip_bg": "#0e0e10",
        "tooltip_fg": "#ffffff",
        "scroll": "#dcdce0",
        "scroll_hover": "#c4c4ca",
        "drop_bg": "#f0f0f2",
        "drop_border": "#cfcfd6",
        "drop_hover_bg": "#efeff1",
        "hero_bg": "#a8bdd6",         # the dusty blue of the cover
        "banner_bg": "#fff3c9",
        "banner_border": "#f0dc8a",
        "banner_text": "#5c4700",
        "rail_bg": "#0e0e10",         # the floating black pill that holds the actions
        "rail_icon": "#ffffff",
        "rail_hover": "#2b2b30",
        "rail_primary_bg": "#ffffff",
        "rail_primary_fg": "#0e0e10",
    },
    THEME_DARK: {
        "canvas": "#0a0a0b",
        "bg": "#151517",
        "surface": "#151517",
        "sunken": "#1d1d20",
        "text": "#f2f2f4",
        "text_muted": "#a0a0aa",
        "text_faint": "#8a8a94",
        "border": "#2a2a2e",
        "border_soft": "#232326",
        "grid_line": "#2a2a2e",
        "accent": "#f2f2f4",
        "accent_fill": "#f2f2f4",
        "accent_hover": "#d6d6da",
        "accent_ink": "#f2f2f4",
        "accent_soft": "#26262b",
        "accent_text": "#0e0e10",
        "today_bg": "#19191c",
        "now_bg": "#19191c",
        "card_bg": "#1d1d20",
        "opened_bg": "#18181a",
        "missed_bg": "#241a1a",
        "nolink_bg": "#22200f",
        "skipped_bg": "#18181a",
        "chip_bg": "#26262b",
        "success_bg": "#17301f",
        "success_fg": "#7edc9e",
        "danger": "#f0665c",
        "danger_bg": "#33191a",
        "danger_fg": "#ff8a84",
        "warn_bg": "#33290c",
        "warn_fg": "#f1d86b",
        "zoom_bg": "#142b40",
        "zoom_fg": "#8cc8f5",
        "meet_bg": "#17301f",
        "meet_fg": "#7edc9e",
        "tooltip_bg": "#f2f2f4",
        "tooltip_fg": "#0e0e10",
        "scroll": "#33333a",
        "scroll_hover": "#4a4a52",
        "drop_bg": "#1d1d20",
        "drop_border": "#3a3a40",
        "drop_hover_bg": "#26262b",
        "hero_bg": "#26262b",
        "banner_bg": "#33290c",
        "banner_border": "#5c4a12",
        "banner_text": "#f1d86b",
        "rail_bg": "#1d1d20",
        "rail_icon": "#f2f2f4",
        "rail_hover": "#2e2e33",
        "rail_primary_bg": "#f2f2f4",
        "rail_primary_fg": "#0e0e10",
    },
}

# The theme currently applied to the QApplication. Widgets that paint themselves (the card shadow,
# the tray glyph) read this rather than parsing the stylesheet back out.
_active = THEME_LIGHT


def system_theme() -> str:
    """Read the OS appearance: dark mode vs light mode."""
    if sys.platform == "darwin":
        try:
            import subprocess

            res = subprocess.run(
                ["defaults", "read", "-g", "AppleInterfaceStyle"],
                capture_output=True,
                text=True,
                timeout=1,
            )
            if res.returncode == 0 and res.stdout.strip() == "Dark":
                return THEME_DARK
            return THEME_LIGHT
        except Exception:
            return THEME_LIGHT

    if winreg is None:
        return THEME_LIGHT
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, PERSONALIZE_KEY) as key:
            value, _ = winreg.QueryValueEx(key, "AppsUseLightTheme")
            return THEME_LIGHT if value else THEME_DARK
    except (FileNotFoundError, OSError):
        return THEME_LIGHT



def resolve(theme: str) -> str:
    """Turn the stored setting (which may be ``system``) into ``light`` or ``dark``."""
    if theme == THEME_DARK:
        return THEME_DARK
    if theme == THEME_LIGHT:
        return THEME_LIGHT
    return system_theme()


def active() -> str:
    return _active


def is_dark() -> bool:
    return _active == THEME_DARK


def token(name: str) -> str:
    """One colour of the active palette, for the widgets that paint instead of being styled."""
    return PALETTES[_active][name]


def interface_font() -> str:
    """The first family of ``font_stack()`` that Qt actually has.

    Resolved here rather than written into the stylesheet as a comma-separated list: Qt honours
    only the first family in such a list, so a missing Inter would not fall through to Segoe
    UI -- it would fall through to a default with no glyphs at all, and the whole interface would
    render as empty boxes.
    """
    stack = font_stack()
    try:
        from PySide6.QtGui import QFontDatabase, QGuiApplication
    except ImportError:  # pragma: no cover - Qt is a hard dependency of the app itself
        return stack[-1]
    if QGuiApplication.instance() is None:
        return stack[-1]
    available = set(QFontDatabase.families())
    for family in stack:
        if family in available:
            return family
    return stack[-1]


def _asset_dir() -> Path:
    """Where generated stylesheet assets live, beside the database."""
    from .storage import default_db_path

    directory = Path(default_db_path()).parent / "assets"
    directory.mkdir(parents=True, exist_ok=True)
    return directory


def checkmark_icon(colour: str, name: str) -> str:
    """Draw a tick and return a path Qt stylesheets can reference.

    A checked QCheckBox has to *look* checked, and QSS cannot draw a shape -- ``image:`` wants a
    file. Filling the box with the accent colour was the alternative and read as a coloured
    square rather than a tick. So the tick is painted once per theme into a small PNG next to the
    database, and the stylesheet points at it.
    """
    from PySide6.QtCore import QPointF, Qt
    from PySide6.QtGui import QColor, QGuiApplication, QPainter, QPen, QPixmap

    if QGuiApplication.instance() is None:
        # No GUI yet (import-time or a headless check): the stylesheet falls back to no image.
        return ""

    path = _asset_dir() / f"check-{name}.png"
    size = 15
    pixmap = QPixmap(size, size)
    pixmap.fill(QColor(0, 0, 0, 0))
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.Antialiasing)
    pen = QPen(QColor(colour))
    pen.setWidthF(2.2)
    pen.setCapStyle(Qt.RoundCap)
    pen.setJoinStyle(Qt.RoundJoin)
    painter.setPen(pen)
    painter.drawPolyline(
        [QPointF(3.0, 7.8), QPointF(6.2, 11.0), QPointF(12.0, 4.2)]
    )
    painter.end()
    if not pixmap.save(str(path), "PNG"):
        log.warning("could not write the checkmark asset to %s", path)
        return ""
    # QSS wants forward slashes even on Windows.
    return str(path).replace("\\", "/")


def chevron_icon(colour: str, name: str, pointing_down: bool) -> str:
    """Draw the little arrow a combo box and a spin box need.

    Same reasoning as :func:`checkmark_icon`: QSS cannot draw a shape, and styling any part of a
    sub-control makes Qt stop drawing the native one. Without this the combo boxes came up as
    empty rounded fields with nothing to say they open.
    """
    from PySide6.QtCore import QPointF, Qt
    from PySide6.QtGui import QColor, QGuiApplication, QPainter, QPen, QPixmap

    if QGuiApplication.instance() is None:
        return ""

    direction = "down" if pointing_down else "up"
    path = _asset_dir() / f"chevron-{direction}-{name}.png"
    width, height = 14, 9
    pixmap = QPixmap(width, height)
    pixmap.fill(QColor(0, 0, 0, 0))
    painter = QPainter(pixmap)
    painter.setRenderHint(QPainter.Antialiasing)
    pen = QPen(QColor(colour))
    pen.setWidthF(1.8)
    pen.setCapStyle(Qt.RoundCap)
    pen.setJoinStyle(Qt.RoundJoin)
    painter.setPen(pen)
    top, bottom = 3.0, 6.6
    first, last = (top, bottom) if pointing_down else (bottom, top)
    painter.drawPolyline([QPointF(3.4, first), QPointF(7.0, last), QPointF(10.6, first)])
    painter.end()
    if not pixmap.save(str(path), "PNG"):
        log.warning("could not write the chevron asset to %s", path)
        return ""
    return str(path).replace("\\", "/")


# Qt reckons a point as one pixel on macOS (72 dpi) and as 4/3 of a pixel on Windows (96 dpi), so
# the same ``10pt`` that reads comfortably on Windows is a 10 px font on a Mac, and the interface
# looked shrunken there. The full correction (96/72 = 1.33) turned out too large for a window this
# dense, so a Mac gets a step between the two: 10pt becomes 11.5 px.
MAC_POINT_SCALE = 1.15


def point_scale() -> float:
    """How much to enlarge the stylesheet's point sizes on this platform."""
    return MAC_POINT_SCALE if sys.platform == "darwin" else 1.0


def scaled_points(text: str) -> str:
    """``text`` with every ``<n>pt`` multiplied by ``point_scale()`` (a no-op where it is 1)."""
    scale = point_scale()
    if scale == 1.0:
        return text
    return re.sub(
        r"(\d+(?:\.\d+)?)pt",
        lambda match: f"{float(match.group(1)) * scale:.1f}pt",
        text,
    )


def stylesheet(theme: str) -> str:
    """Render ``styles.qss`` with the palette for ``theme`` (which may be ``system``)."""
    resolved = resolve(theme)
    try:
        template = Template(STYLESHEET.read_text(encoding="utf-8"))
    except OSError:
        log.warning("stylesheet not found at %s", STYLESHEET)
        return ""
    palette = dict(PALETTES[resolved])
    palette["font_family"] = f'"{interface_font()}"'
    tick = checkmark_icon(palette["accent"], resolved)
    palette["check_icon"] = f'url("{tick}")' if tick else "none"
    for pointing_down in (True, False):
        key = "chevron_down" if pointing_down else "chevron_up"
        arrow = chevron_icon(palette["text_muted"], resolved, pointing_down)
        palette[key] = f'url("{arrow}")' if arrow else "none"
    return scaled_points(template.safe_substitute(palette))


def apply(app, theme: str) -> str:
    """Apply ``theme`` to a QApplication and return the resolved (``light``/``dark``) name."""
    global _active
    _active = resolve(theme)
    app.setStyleSheet(stylesheet(theme))
    return _active


def next_theme(current: str) -> str:
    """What the toolbar toggle switches to: whatever is not showing right now."""
    return THEME_LIGHT if resolve(current) == THEME_DARK else THEME_DARK


__all__ = [
    "PALETTES",
    "THEME_DARK",
    "THEME_LIGHT",
    "THEME_SYSTEM",
    "WINDOWS_FONT_STACK",
    "active",
    "apply",
    "chevron_icon",
    "font_stack",
    "interface_font",
    "is_dark",
    "next_theme",
    "point_scale",
    "scaled_points",
    "resolve",
    "stylesheet",
    "system_theme",
    "token",
]
