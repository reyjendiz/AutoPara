"""Іконка в системному треї, меню та сповіщення."""

from __future__ import annotations

from pathlib import Path

from PySide6.QtCore import QObject, Qt, Signal
from PySide6.QtGui import QAction, QIcon, QPixmap
from PySide6.QtWidgets import QMenu, QSystemTrayIcon


# The app mark is a shipped picture, not a drawing: ``assets/logo.png`` is one square, 1024 px, with
# transparent corners. It is fixed rather than themed -- it appears on the taskbar, on the desktop
# and at the top of the rail, and none of those follows AutoPara's own light/dark setting.
LOGO_PATH = Path(__file__).resolve().parent / "assets" / "logo.png"

# What goes into the .ico Windows reads for the executable, the desktop shortcut and the taskbar.
# Explorer picks whichever of these fits the slot it is filling, so a missing size is a blurry
# icon somewhere rather than no icon at all.
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)


def build_icon() -> QIcon:
    """Значок застосунку: одне зображення, масштаб якого добирає сама система."""
    return QIcon(icon_pixmap(64))


_logo: QPixmap | None = None


def _source() -> QPixmap:
    """The 1024 px original, read once (it needs the GUI application to exist)."""
    global _logo
    if _logo is None or _logo.isNull():
        _logo = QPixmap(str(LOGO_PATH))
    return _logo


def icon_pixmap(size: int) -> QPixmap:
    """The app mark at ``size`` logical pixels, rendered at 2x so it stays sharp when scaled up.

    One picture serves the tray, the rail, the first screen and the .ico, so none of them can drift
    from the others. It is scaled down from the 1024 px original every time rather than cached per
    size: the sizes are few and the smooth scaling of the original is what keeps 16 px legible.
    """
    scale = 2
    pixmap = _source().scaled(
        size * scale, size * scale, Qt.KeepAspectRatio, Qt.SmoothTransformation
    )
    pixmap.setDevicePixelRatio(scale)
    return pixmap


def write_ico(path) -> Path:
    """Write the mark to a Windows ``.ico`` and return where it went.

    Windows will not take a painted pixmap: the icon of an executable, and therefore of every
    shortcut and taskbar button that points at it, has to be a real file compiled into the binary.
    Without one the build carried PyInstaller's stock icon, which is Python's -- so AutoPara
    appeared on the desktop as a Python program.

    The file is generated from the same drawing as everything else rather than committed, so the
    mark cannot drift between the tray and the desktop. Each entry is a PNG, which Windows has
    read inside an ``.ico`` since Vista and which keeps the 256 px size from costing 256 KB.
    """
    import struct

    from PySide6.QtCore import QBuffer, QByteArray

    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)

    frames: list[bytes] = []
    for size in ICO_SIZES:
        image = icon_pixmap(size).toImage().scaled(
            size, size, Qt.KeepAspectRatio, Qt.SmoothTransformation
        )
        payload = QByteArray()
        buffer = QBuffer(payload)
        buffer.open(QBuffer.WriteOnly)
        image.save(buffer, "PNG")
        buffer.close()
        frames.append(bytes(payload))

    # ICONDIR, then one 16-byte ICONDIRENTRY per size, then the images back to back.
    header = struct.pack("<HHH", 0, 1, len(frames))
    offset = len(header) + 16 * len(frames)
    directory = b""
    for size, frame in zip(ICO_SIZES, frames):
        # 256 is written as 0: the field is one byte and 256 does not fit in it.
        directory += struct.pack(
            "<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(frame), offset
        )
        offset += len(frame)

    path.write_bytes(header + directory + b"".join(frames))
    return path


def write_icns(path) -> Path:
    """Write the mark to an Apple .icns file and return where it went.

    Generates all standard Retina and non-Retina icon sizes (16, 32, 64, 128, 256, 512, 1024)
    into a temporary .iconset and compiles them using macOS's built-in iconutil.
    """
    import shutil
    import subprocess
    import tempfile

    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)

    sizes = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ]

    with tempfile.TemporaryDirectory() as temp_dir:
        iconset_dir = Path(temp_dir) / "AutoPara.iconset"
        iconset_dir.mkdir()

        for px, file_name in sizes:
            pixmap = icon_pixmap(px)
            pixmap.save(str(iconset_dir / file_name), "PNG")

        temp_icns = Path(temp_dir) / "AutoPara.icns"
        subprocess.run(
            ["iconutil", "-c", "icns", str(iconset_dir), "-o", str(temp_icns)],
            check=True,
            capture_output=True,
        )
        shutil.copyfile(temp_icns, path)

    return path



class Tray(QObject):
    """Володіє іконкою трею. Повідомляє про намір — рішення ухвалює застосунок."""

    show_requested = Signal()
    settings_requested = Signal()
    import_requested = Signal()
    check_updates_requested = Signal()
    quit_requested = Signal()
    message_clicked = Signal()

    def __init__(self, parent: QObject | None = None):
        super().__init__(parent)
        self.icon = QSystemTrayIcon(build_icon(), self)
        self.icon.setToolTip("AutoPara — автозапуск пар")
        self._build_menu()
        self.icon.activated.connect(self._activated)
        self.icon.messageClicked.connect(self.message_clicked.emit)

    def _build_menu(self) -> None:
        menu = QMenu()
        show = QAction("Показати розклад", menu)
        show.triggered.connect(self.show_requested.emit)
        menu.addAction(show)

        import_action = QAction("Імпортувати розклад…", menu)
        import_action.triggered.connect(self.import_requested.emit)
        menu.addAction(import_action)

        settings = QAction("Налаштування…", menu)
        settings.triggered.connect(self.settings_requested.emit)
        menu.addAction(settings)

        updates = QAction("Перевірити оновлення", menu)
        updates.triggered.connect(self.check_updates_requested.emit)
        menu.addAction(updates)

        menu.addSeparator()
        quit_action = QAction("Вийти з AutoPara", menu)
        quit_action.triggered.connect(self.quit_requested.emit)
        menu.addAction(quit_action)

        self.menu = menu
        self.icon.setContextMenu(menu)

    def _activated(self, reason) -> None:
        if reason in (QSystemTrayIcon.Trigger, QSystemTrayIcon.DoubleClick):
            self.show_requested.emit()

    def show(self) -> None:
        self.icon.show()

    def hide(self) -> None:
        self.icon.hide()

    def notify(self, title: str, message: str, seconds: int = 12) -> None:
        self.icon.showMessage(title, message, build_icon(), seconds * 1000)

    def set_tooltip(self, text: str) -> None:
        self.icon.setToolTip(text)
