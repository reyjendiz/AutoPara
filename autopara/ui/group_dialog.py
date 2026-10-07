"""Обрати інший курс і групу з уже імпортованого розкладу.

Імпорт зберігає *всі* курси документа, а вибір курсу й групи -- це лише налаштування. Тож коли
файл уже відкрито, але обрано не ту групу, документ перечитувати не треба: цей діалог лише
перемикає вибір. Повторний імпорт лишається для нового документа.
"""

from __future__ import annotations

from PySide6.QtCore import Qt
from PySide6.QtWidgets import (
    QComboBox,
    QDialog,
    QHBoxLayout,
    QLabel,
    QListWidget,
    QListWidgetItem,
    QPushButton,
    QVBoxLayout,
)

from ..core.storage import Storage
from .metrics import settle
from .month_view import pairs_word


class GroupDialog(QDialog):
    """Курс у списку, групи цього курсу під ним; «Обрати» запам'ятовує вибір."""

    def __init__(self, storage: Storage, parent=None):
        super().__init__(parent)
        self.storage = storage
        self.setWindowTitle("Курс і група")
        self.setMinimumWidth(460)
        self._build()
        self._load()
        settle(self)

    def _build(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(24, 20, 24, 20)
        layout.setSpacing(14)

        title = QLabel("Курс і група")
        title.setObjectName("TitleLabel")
        layout.addWidget(title)

        hint = QLabel(
            "Оберіть іншу групу з уже імпортованого розкладу. Файл перечитувати не потрібно."
        )
        hint.setObjectName("FormHint")
        hint.setWordWrap(True)
        layout.addWidget(hint)

        course_label = QLabel("Курс")
        course_label.setObjectName("SectionLabel")
        layout.addWidget(course_label)
        self.course_combo = QComboBox()
        self.course_combo.currentIndexChanged.connect(self._course_changed)
        layout.addWidget(self.course_combo)

        group_label = QLabel("Група")
        group_label.setObjectName("SectionLabel")
        layout.addWidget(group_label)
        self.group_list = QListWidget()
        self.group_list.setMinimumHeight(160)
        self.group_list.currentRowChanged.connect(self._row_changed)
        self.group_list.itemDoubleClicked.connect(lambda _item: self._accept())
        layout.addWidget(self.group_list)

        # Кнопки шикуються вручну, а не через QDialogButtonBox: на Windows той ставить
        # головну дію ліворуч, а в цій мові інтерфейсу вона завжди крайня праворуч.
        footer = QHBoxLayout()
        footer.setSpacing(8)
        footer.addStretch(1)
        cancel = QPushButton("Скасувати")
        cancel.setObjectName("Plain")
        cancel.clicked.connect(self.reject)
        footer.addWidget(cancel)
        self.choose_button = QPushButton("Обрати")
        self.choose_button.setObjectName("Primary")
        self.choose_button.setDefault(True)
        self.choose_button.clicked.connect(self._accept)
        footer.addWidget(self.choose_button)
        layout.addLayout(footer)

    # ------------------------------------------------------------------- load

    def _load(self) -> None:
        settings = self.storage.settings()
        for course in self.storage.courses():
            self.course_combo.addItem(course.name, course.id)
        index = self.course_combo.findData(settings.selected_course_id)
        self.course_combo.setCurrentIndex(max(index, 0))
        self._course_changed()
        for row in range(self.group_list.count()):
            if self.group_list.item(row).data(Qt.UserRole) == settings.selected_group_id:
                self.group_list.setCurrentRow(row)
                break

    def _course_changed(self) -> None:
        self.group_list.clear()
        course_id = self.course_combo.currentData()
        if course_id is None:
            self.choose_button.setEnabled(False)
            return
        for group in self.storage.groups(course_id):
            count = len(self.storage.lessons_for_group(group.id))
            suffix = "немає пар" if count == 0 else f"{count} {pairs_word(count)}"
            label = group.name + (f"  ·  {group.specialty}" if group.specialty else "")
            item = QListWidgetItem(f"{label}  ·  {suffix}")
            item.setData(Qt.UserRole, group.id)
            self.group_list.addItem(item)
        if self.group_list.count():
            self.group_list.setCurrentRow(0)
        self._row_changed(self.group_list.currentRow())

    def _row_changed(self, row: int) -> None:
        self.choose_button.setEnabled(row >= 0)

    # ----------------------------------------------------------------- accept

    def _accept(self) -> None:
        item = self.group_list.currentItem()
        course_id = self.course_combo.currentData()
        if item is None or course_id is None:
            return
        self.storage.set_setting("selected_course_id", course_id)
        self.storage.set_setting("selected_group_id", item.data(Qt.UserRole))
        self.accept()
