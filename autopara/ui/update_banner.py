"""The strip that says a newer version is out, downloads it, and offers to install it.

One strip, three states, in the same place as the missed-class prompt (above the calendar):

* **available** -- "Доступна нова версія": download it, skip it, or not now;
* **downloading** -- a progress bar, no buttons (nothing to decide until it is done);
* **ready** -- "Оновлення завантажено": install and restart, or not now.

It owns no network and runs nothing; it reports what the user chose, and the window and the app
act on it.
"""

from __future__ import annotations

from PySide6.QtCore import Qt, Signal
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QLabel,
    QProgressBar,
    QPushButton,
    QVBoxLayout,
)

from ..core import theme
from ..core.updater import Release
from . import icons


class UpdateBanner(QFrame):
    download_requested = Signal(object)  # Release
    install_requested = Signal(str)      # path of the verified download
    page_requested = Signal(str)         # the release's page on GitHub
    skipped = Signal(object)             # Release -- "not this version"
    dismissed = Signal()                 # "later": hide it for now

    def __init__(self, current: str, parent=None):
        super().__init__(parent)
        self.setObjectName("UpdateBanner")
        self.current = current
        self.release: Release | None = None
        self.path: str = ""
        self._build()
        self.hide()

    def _build(self) -> None:
        row = QHBoxLayout(self)
        row.setContentsMargins(20, 12, 20, 12)
        row.setSpacing(14)

        self.glyph = QLabel()
        row.addWidget(self.glyph, 0)
        self.repaint_glyph()

        texts = QVBoxLayout()
        texts.setSpacing(1)
        self.title = QLabel("")
        self.title.setObjectName("UpdateText")
        self.detail = QLabel("")
        self.detail.setObjectName("UpdateDetail")
        self.detail.setWordWrap(True)
        self.bar = QProgressBar()
        self.bar.setObjectName("UpdateProgress")
        self.bar.setRange(0, 100)
        self.bar.setTextVisible(False)
        self.bar.setFixedHeight(8)
        texts.addWidget(self.title)
        texts.addWidget(self.detail)
        texts.addWidget(self.bar)
        row.addLayout(texts, 1)

        self.page_button = QPushButton("Що нового")
        self.page_button.setObjectName("Plain")
        self.page_button.clicked.connect(self._page)
        row.addWidget(self.page_button)

        self.skip_button = QPushButton("Пропустити версію")
        self.skip_button.setObjectName("Plain")
        self.skip_button.clicked.connect(self._skip)
        row.addWidget(self.skip_button)

        self.later_button = QPushButton("Пізніше")
        self.later_button.setObjectName("Plain")
        self.later_button.clicked.connect(self._later)
        row.addWidget(self.later_button)

        self.main_button = QPushButton("")
        self.main_button.setObjectName("Primary")
        self.main_button.clicked.connect(self._main)
        row.addWidget(self.main_button)

        self._action = ""

    def repaint_glyph(self) -> None:
        """Значок намальований кодом, тож після зміни теми його треба перефарбувати."""
        self.glyph.setPixmap(icons.pixmap("download", theme.token("text"), 22))

    # ------------------------------------------------------------------ states

    def _show_buttons(self, main: str, action: str, page=True, skip=False, later=True) -> None:
        self._action = action
        self.main_button.setText(main)
        self.main_button.setVisible(bool(main))
        self.page_button.setVisible(page and bool(self.release and self.release.page_url))
        self.skip_button.setVisible(skip)
        self.later_button.setVisible(later)

    def show_available(self, release: Release, installable: bool) -> None:
        self.release = release
        self.bar.hide()
        self.title.setText(f"Доступна нова версія AutoPara {release.version}")
        self.detail.setText(self._notes(release) or f"Зараз у вас {self.current}.")
        if installable and release.asset:
            self._show_buttons("Завантажити", "download", skip=True)
        elif release.page_url:
            self._show_buttons("Відкрити сторінку", "page", page=False, skip=True)
        else:
            self._show_buttons("", "", page=False, skip=True)
        self.show()

    def show_progress(self, release: Release, percent: int) -> None:
        self.release = release
        self.title.setText(f"Завантажую AutoPara {release.version}…")
        self.detail.setText(f"{percent}%")
        self.bar.setValue(percent)
        self.bar.show()
        self._show_buttons("", "", page=False, later=False)
        self.show()

    def show_ready(self, release: Release, path: str, restarts: bool) -> None:
        self.release = release
        self.path = path
        self.bar.hide()
        self.title.setText(f"Оновлення {release.version} завантажено")
        if restarts:
            self.detail.setText("Встановлення закриє AutoPara і запустить нову версію.")
            self._show_buttons("Встановити й перезапустити", "install", page=False)
        else:
            self.detail.setText("Відкрийте образ і перетягніть AutoPara у «Програми», замінивши стару.")
            self._show_buttons("Відкрити образ", "install", page=False)
        self.show()

    @staticmethod
    def _notes(release: Release) -> str:
        """The first line of the release notes, as the detail line -- no more than one."""
        for line in release.notes.splitlines():
            line = line.strip().lstrip("#*- ").strip()
            if line:
                return line[:140]
        return ""

    # ----------------------------------------------------------------- clicks

    def _main(self) -> None:
        if self.release is None:
            return
        if self._action == "download":
            self.download_requested.emit(self.release)
        elif self._action == "page":
            self.page_requested.emit(self.release.page_url)
        elif self._action == "install":
            self.install_requested.emit(self.path)

    def _page(self) -> None:
        if self.release is not None and self.release.page_url:
            self.page_requested.emit(self.release.page_url)

    def _skip(self) -> None:
        if self.release is not None:
            self.skipped.emit(self.release)
        self.hide()

    def _later(self) -> None:
        self.dismissed.emit()
        self.hide()
