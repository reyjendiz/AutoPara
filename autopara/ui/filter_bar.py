"""The strip under the navigation bar: a search box, a subject filter and a group filter.

Like the navigation bar it owns no data. The window tells it what to offer (``set_subjects``,
``set_groups``) and what was found (``set_summary``); it only reports that something changed or
that the user asked to jump to the next or previous match.
"""

from __future__ import annotations

from PySide6.QtCore import QSize, Qt, Signal
from PySide6.QtGui import QKeyEvent
from PySide6.QtWidgets import (
    QComboBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QWidget,
)

from ..core import theme
from ..core.filters import LessonFilter
from . import icons

ALL_SUBJECTS = "Усі предмети"


class SearchField(QLineEdit):
    """A line edit that clears on Escape and jumps to a match on Enter / Shift+Enter."""

    jump = Signal(int)  # +1 next match, -1 previous

    def keyPressEvent(self, event: QKeyEvent):  # noqa: N802 - Qt naming
        if event.key() == Qt.Key_Escape and self.text():
            self.clear()
            return
        if event.key() in (Qt.Key_Return, Qt.Key_Enter):
            self.jump.emit(-1 if event.modifiers() & Qt.ShiftModifier else 1)
            return
        super().keyPressEvent(event)


class FilterBar(QWidget):
    changed = Signal()
    jump_requested = Signal(int)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("FilterBar")
        row = QHBoxLayout(self)
        row.setContentsMargins(0, 0, 0, 0)
        row.setSpacing(8)

        self.search = SearchField()
        self.search.setObjectName("FilterSearch")
        self.search.setPlaceholderText("Пошук: предмет або викладач")
        self.search.setToolTip(
            "Знайти пару за предметом чи викладачем\n"
            "Enter — до наступного збігу · Shift+Enter — до попереднього · Esc — очистити"
        )
        self._search_icon = self.search.addAction(
            icons.icon("search", theme.token("text_muted"), 16), QLineEdit.LeadingPosition
        )
        self._clear_icon = self.search.addAction(
            icons.icon("close", theme.token("text_muted"), 14), QLineEdit.TrailingPosition
        )
        self._clear_icon.setVisible(False)
        self._clear_icon.triggered.connect(self.search.clear)
        self.search.textChanged.connect(self._text_changed)
        self.search.jump.connect(self.jump_requested.emit)
        row.addWidget(self.search, 1)

        self.subject = QComboBox()
        self.subject.setObjectName("FilterCombo")
        self.subject.setToolTip("Показати лише один предмет")
        self.subject.setMinimumWidth(180)
        self.subject.addItem(ALL_SUBJECTS, "")
        self.subject.currentIndexChanged.connect(self._combo_changed)
        row.addWidget(self.subject)

        self.group = QComboBox()
        self.group.setObjectName("FilterCombo")
        self.group.setToolTip("Показати розклад іншої групи цього курсу")
        self.group.setMinimumWidth(200)
        self.group.currentIndexChanged.connect(self._combo_changed)
        row.addWidget(self.group)

        self.summary = QLabel("")
        self.summary.setObjectName("FilterSummary")
        row.addWidget(self.summary)

        self.reset_button = QPushButton("Скинути")
        self.reset_button.setObjectName("Plain")
        self.reset_button.setToolTip("Прибрати пошук і фільтри")
        self.reset_button.clicked.connect(self.reset)
        self.reset_button.hide()
        row.addWidget(self.reset_button)

        self._quiet = False  # set while the window refills the drop-downs

    # ------------------------------------------------------------------- state

    def current_filter(self) -> LessonFilter:
        return LessonFilter(text=self.search.text(), subject=self.subject.currentData() or "")

    def group_id(self) -> int | None:
        return self.group.currentData()

    def focus_search(self) -> None:
        self.search.setFocus()
        self.search.selectAll()

    def reset(self) -> None:
        """Back to everything: no text, every subject (the group choice stays where it is)."""
        self._quiet = True
        self.search.clear()
        self.subject.setCurrentIndex(0)
        self._quiet = False
        self._refresh_reset()
        self.changed.emit()

    # ------------------------------------------------------ what the window offers

    def set_subjects(self, subjects: list[str]) -> None:
        """Offer ``subjects``; keep the chosen one if it is still there, otherwise go back to all."""
        chosen = self.subject.currentData() or ""
        self._quiet = True
        self.subject.clear()
        self.subject.addItem(ALL_SUBJECTS, "")
        for name in subjects:
            self.subject.addItem(name, name)
        index = self.subject.findData(chosen)
        self.subject.setCurrentIndex(index if index >= 0 else 0)
        self._quiet = False
        self._refresh_reset()

    def set_groups(self, groups: list[tuple[int, str]], current: int | None) -> None:
        """Offer the course's groups; ``current`` is the one on screen."""
        self._quiet = True
        self.group.clear()
        for group_id, name in groups:
            self.group.addItem(name, group_id)
        index = self.group.findData(current)
        self.group.setCurrentIndex(max(index, 0))
        self._quiet = False
        self.group.setVisible(len(groups) > 1)

    def set_summary(self, text: str) -> None:
        self.summary.setText(text)
        self.summary.setVisible(bool(text))

    def repaint_glyphs(self) -> None:
        self._search_icon.setIcon(icons.icon("search", theme.token("text_muted"), 16))
        self._clear_icon.setIcon(icons.icon("close", theme.token("text_muted"), 14))

    # ----------------------------------------------------------------- reactions

    def _text_changed(self, text: str) -> None:
        self._clear_icon.setVisible(bool(text))
        self._refresh_reset()
        if not self._quiet:
            self.changed.emit()

    def _combo_changed(self, _index: int) -> None:
        self._refresh_reset()
        if not self._quiet:
            self.changed.emit()

    def _refresh_reset(self) -> None:
        self.reset_button.setVisible(self.current_filter().active)

    def sizeHint(self) -> QSize:  # noqa: N802 - Qt naming
        return QSize(super().sizeHint().width(), 36)
