"""Створення та редагування пари.

Час пари задається вручну -- початок і кінець. Сітка тижня розкладає пару за реальним часом, тож
заняття не зобов'язане збігатися ані з годиною, ані зі стандартним слотом.

Пара буває трьох видів (docs/BACKEND.md, «Lessons that belong to a date»): разова -- на одну дату;
щотижнева до дати -- серія з кінцем; і щотижнева без кінця -- рядок розкладу, як усі імпортовані.
Імпортована пара лишається рядком розкладу: вид повтору для неї не пропонується.
"""

from __future__ import annotations

from datetime import date, timedelta

from PySide6.QtCore import QDate, QLocale, Qt, QTime
from PySide6.QtGui import QColor, QTextCharFormat
from PySide6.QtWidgets import (
    QComboBox,
    QDateEdit,
    QDialog,
    QFormLayout,
    QHBoxLayout,
    QLabel,
    QPushButton,
    QLineEdit,
    QMessageBox,
    QTimeEdit,
    QVBoxLayout,
)

from ..core import theme
from ..core.launcher import is_openable
from ..core.models import Lesson
from ..core.storage import Storage
from ..importer.normalize import (
    DAY_ACCUSATIVE,
    DAY_NAMES,
    MONTH_GENITIVE,
    add_minutes,
    detect_provider,
    minutes_between,
    pair_slot,
)
from .metrics import INPUT_HEIGHT, settle

PROVIDER_LABEL = {"zoom": "Zoom", "google_meet": "Google Meet"}

REPEAT_ONCE = "once"
REPEAT_UNTIL = "until"
REPEAT_WEEKLY = "weekly"
REPEAT_CHOICES = (
    (REPEAT_ONCE, "Один раз"),
    (REPEAT_UNTIL, "Щотижня до дати"),
    (REPEAT_WEEKLY, "Щотижня, без кінця"),
)

# A series suggested for a new "weekly until" lesson: four more weeks, about a month.
DEFAULT_SERIES_DAYS = 28
UKRAINIAN = QLocale(QLocale.Ukrainian)


def _to_qtime(hhmm: str) -> QTime:
    hour, minute = (int(part) for part in hhmm.split(":"))
    return QTime(hour, minute)


def _from_qtime(value: QTime) -> str:
    return f"{value.hour():02d}:{value.minute():02d}"


def _to_qdate(day: date) -> QDate:
    return QDate(day.year, day.month, day.day)


def _from_qdate(value: QDate) -> date:
    return date(value.year(), value.month(), value.day())


def _next_weekday(day_index: int, today: date | None = None) -> date:
    """The next date (today included) that falls on ``day_index``."""
    today = today or date.today()
    return today + timedelta(days=(day_index - today.weekday()) % 7)


def times_word(count: int) -> str:
    """Ukrainian plural for 'раз': 1 раз, 2-4 рази, otherwise разів."""
    if count % 10 == 1 and count % 100 != 11:
        return "раз"
    if count % 10 in (2, 3, 4) and count % 100 not in (12, 13, 14):
        return "рази"
    return "разів"


def _long_date(day: date) -> str:
    return f"{day.day} {MONTH_GENITIVE[day.month - 1]}"


def _date_edit() -> QDateEdit:
    """A date field with a calendar that speaks Ukrainian and starts its week on Monday.

    The locale is set on the field *and* its popup: the popup is a separate widget and would
    otherwise name its months and weekdays in whatever language the system is set to.
    """
    edit = QDateEdit()
    edit.setMinimumHeight(INPUT_HEIGHT)
    edit.setLocale(UKRAINIAN)
    edit.setDisplayFormat("dd.MM.yyyy")
    edit.setCalendarPopup(True)
    calendar = edit.calendarWidget()
    calendar.setLocale(UKRAINIAN)
    calendar.setFirstDayOfWeek(Qt.Monday)
    calendar.setGridVisible(False)
    # Its size hint is measured before the stylesheet gives the navigation bar its height, so the
    # popup opened a few pixels short and cut off the last row of the month.
    calendar.setMinimumSize(280, 240)
    calendar.setVerticalHeaderFormat(calendar.VerticalHeaderFormat.NoVerticalHeader)

    # Qt paints weekends red by default -- a colour the interface keeps for "not opened". Weekends
    # are just quieter days here, and the weekday names are a heading, not data.
    quiet = QTextCharFormat()
    quiet.setForeground(QColor(theme.token("text_muted")))
    for weekday in (Qt.Saturday, Qt.Sunday):
        calendar.setWeekdayTextFormat(weekday, quiet)
    heading = QTextCharFormat()
    heading.setForeground(QColor(theme.token("text_muted")))
    heading.setFontWeight(600)
    calendar.setHeaderTextFormat(heading)
    return edit


class EditDialog(QDialog):
    """Створює пару або змінює наявну (зокрема додає відсутнє посилання)."""

    def __init__(
        self,
        storage: Storage,
        group_id: int,
        lesson: Lesson | None = None,
        parent=None,
        day: date | None = None,
        start_time: str | None = None,
    ):
        """``day`` is the date a new class was asked for (an empty hour, a month cell)."""
        super().__init__(parent)
        self.storage = storage
        self.group_id = group_id
        self.lesson = lesson
        self.setWindowTitle("Редагувати пару" if lesson else "Нова пара")
        self.setMinimumWidth(440)
        self._build()
        if lesson:
            self._populate(lesson)
        else:
            self._prefill(day, start_time)
        self._repeat_changed()
        settle(self)

    def _build(self) -> None:
        layout = QVBoxLayout(self)
        layout.setContentsMargins(24, 20, 24, 20)
        layout.setSpacing(14)

        title = QLabel("Редагувати пару" if self.lesson else "Створити пару")
        title.setObjectName("TitleLabel")
        layout.addWidget(title)

        form = QFormLayout()

        form.setLabelAlignment(Qt.AlignLeft | Qt.AlignVCenter)
        form.setSpacing(11)

        self.subject_edit = QLineEdit()
        self.subject_edit.setPlaceholderText("Назва предмета")
        form.addRow("Предмет", self.subject_edit)

        self.teacher_edit = QLineEdit()
        self.teacher_edit.setPlaceholderText("Викладач (необов'язково)")
        form.addRow("Викладач", self.teacher_edit)

        # How often the class happens. Only manual classes can change it: an imported class is a
        # row of the timetable and stays one.
        self.repeat_combo = QComboBox()
        for value, label in REPEAT_CHOICES:
            self.repeat_combo.addItem(label, value)
        self.repeat_combo.currentIndexChanged.connect(self._repeat_changed)
        form.addRow("Повторення", self.repeat_combo)

        self.date_edit = _date_edit()
        self.date_edit.dateChanged.connect(self._date_changed)
        form.addRow("Дата", self.date_edit)

        self.until_edit = _date_edit()
        self.until_edit.dateChanged.connect(lambda _: self._repeat_changed())
        form.addRow("Повторювати до", self.until_edit)

        self.day_combo = QComboBox()
        for index, name in enumerate(DAY_NAMES):
            self.day_combo.addItem(name, index)
        self.day_combo.currentIndexChanged.connect(lambda _: self._repeat_changed())
        form.addRow("День", self.day_combo)
        self._form = form

        # Час задається вручну: початок і кінець.
        times = QHBoxLayout()
        times.setSpacing(8)
        self.start_edit = QTimeEdit()
        self.start_edit.setMinimumHeight(INPUT_HEIGHT)
        self.start_edit.setDisplayFormat("HH:mm")
        self.start_edit.timeChanged.connect(self._start_changed)
        self.end_edit = QTimeEdit()
        self.end_edit.setMinimumHeight(INPUT_HEIGHT)
        self.end_edit.setDisplayFormat("HH:mm")
        self.end_edit.timeChanged.connect(lambda _: self._refresh_slot_hint())
        times.addWidget(self.start_edit, 1)
        dash = QLabel("–")
        dash.setObjectName("FormHint")
        times.addWidget(dash)
        times.addWidget(self.end_edit, 1)
        form.addRow("Час", times)

        self.url_edit = QLineEdit()
        self.url_edit.setPlaceholderText("https://… (Zoom або Google Meet)")
        self.url_edit.textChanged.connect(self._url_changed)
        form.addRow("Посилання", self.url_edit)

        layout.addLayout(form)

        self.repeat_hint = QLabel("")
        self.repeat_hint.setObjectName("FormHint")
        self.repeat_hint.setWordWrap(True)
        layout.addWidget(self.repeat_hint)

        self.slot_hint = QLabel("")
        self.slot_hint.setObjectName("FormHint")
        layout.addWidget(self.slot_hint)

        self.provider_hint = QLabel("")
        self.provider_hint.setObjectName("FormHint")
        layout.addWidget(self.provider_hint)

        # Кнопки шикуються вручну, а не через QDialogButtonBox: на Windows той ставить
        # головну дію ліворуч, а в цій мові інтерфейсу вона завжди крайня праворуч.
        footer = QHBoxLayout()
        footer.setSpacing(8)
        footer.addStretch(1)

        cancel = QPushButton("Скасувати")
        cancel.setObjectName("Plain")
        cancel.clicked.connect(self.reject)
        footer.addWidget(cancel)

        save = QPushButton("Зберегти")
        save.setObjectName("Primary")
        save.setDefault(True)
        save.clicked.connect(self._accept)
        footer.addWidget(save)
        layout.addLayout(footer)

        self._url_changed("")

    # -------------------------------------------------------------- reactions

    @property
    def repeat_mode(self) -> str:
        return self.repeat_combo.currentData()

    def _date_changed(self, value: QDate) -> None:
        """A series cannot end before it starts, so the end date's floor follows the start."""
        self.until_edit.setMinimumDate(value)
        self._repeat_changed()

    def _repeat_changed(self) -> None:
        """Show the fields the chosen kind of repeat needs, and say what it will do."""
        mode = self.repeat_mode
        form = self._form
        form.setRowVisible(self.date_edit, mode != REPEAT_WEEKLY)
        form.setRowVisible(self.until_edit, mode == REPEAT_UNTIL)
        form.setRowVisible(self.day_combo, mode == REPEAT_WEEKLY)

        if mode == REPEAT_ONCE:
            day = _from_qdate(self.date_edit.date())
            text = f"{DAY_NAMES[day.weekday()]}, {_long_date(day)} — лише цього дня."
        elif mode == REPEAT_UNTIL:
            first = _from_qdate(self.date_edit.date())
            last = _from_qdate(self.until_edit.date())
            count = max((last - first).days // 7 + 1, 1)
            text = (
                f"Щотижня, у {DAY_ACCUSATIVE[first.weekday()]}: з {_long_date(first)} "
                f"по {_long_date(last)} — {count} {times_word(count)}."
            )
        else:
            text = (
                f"Щотижня, у {DAY_ACCUSATIVE[self.day_combo.currentData()]}, "
                "доки пару не видалено."
            )
        self.repeat_hint.setText(text)
        # Rows that came or went change how tall the form needs to be.
        if self.isVisible():
            self.resize(self.width(), self.sizeHint().height())

    def _start_changed(self, value: QTime) -> None:
        """Кінець тягнеться за початком, доки користувач не задав його сам."""
        start = _from_qtime(value)
        duration = self.storage.settings().class_duration_minutes
        if _from_qtime(self.end_edit.time()) <= start:
            self.end_edit.setTime(_to_qtime(add_minutes(start, duration)))
        self._refresh_slot_hint()

    def _refresh_slot_hint(self) -> None:
        start = _from_qtime(self.start_edit.time())
        end = _from_qtime(self.end_edit.time())
        length = minutes_between(start, end)
        if length <= 0:
            self.slot_hint.setText("Кінець має бути пізніше за початок.")
            return
        hours, minutes = divmod(length, 60)
        parts = [f"{hours} год"] if hours else []
        if minutes:
            parts.append(f"{minutes} хв")
        self.slot_hint.setText("Тривалість: " + " ".join(parts))

    def _url_changed(self, text: str) -> None:
        text = text.strip()
        if not text:
            self.provider_hint.setText("Без посилання пара не відкриється автоматично.")
            return
        if not is_openable(text):
            self.provider_hint.setText("Відкриваються лише посилання http:// та https://.")
            return
        provider = detect_provider(text)
        label = PROVIDER_LABEL.get(provider, "Невідомий сервіс")
        self.provider_hint.setText(f"Розпізнано: {label}")

    def _populate(self, lesson: Lesson) -> None:
        self.subject_edit.setText(lesson.subject)
        self.teacher_edit.setText(lesson.teacher.strip("()"))
        self.day_combo.setCurrentIndex(lesson.day_index)
        self.start_edit.setTime(_to_qtime(lesson.start_time))
        self.end_edit.setTime(_to_qtime(lesson.end_time))
        self.url_edit.setText(lesson.url or "")
        self._refresh_slot_hint()

        first = lesson.on_date or _next_weekday(lesson.day_index)
        self.date_edit.setDate(_to_qdate(first))
        self.until_edit.setDate(
            _to_qdate(lesson.repeat_until or first + timedelta(days=DEFAULT_SERIES_DAYS))
        )
        if lesson.on_date is None:
            mode = REPEAT_WEEKLY
        elif lesson.repeat_until is None:
            mode = REPEAT_ONCE
        else:
            mode = REPEAT_UNTIL
        self.repeat_combo.setCurrentIndex(self.repeat_combo.findData(mode))
        # An imported class is a row of the timetable; it cannot be turned into a dated one here.
        if not lesson.is_manual:
            self._form.setRowVisible(self.repeat_combo, False)

    def _prefill(self, day: date | None, start_time: str | None) -> None:
        """Нова пара з порожньої клітинки вже знає свою дату й годину."""
        start = start_time or "08:00"
        duration = self.storage.settings().class_duration_minutes
        day = day or date.today()
        self.day_combo.setCurrentIndex(day.weekday())
        self.date_edit.setDate(_to_qdate(day))
        self.until_edit.setDate(_to_qdate(day + timedelta(days=DEFAULT_SERIES_DAYS)))
        self.repeat_combo.setCurrentIndex(self.repeat_combo.findData(REPEAT_ONCE))
        self.start_edit.setTime(_to_qtime(start))
        self.end_edit.setTime(_to_qtime(add_minutes(start, duration)))
        self._refresh_slot_hint()

    # ----------------------------------------------------------------- accept

    def _accept(self) -> None:
        subject = self.subject_edit.text().strip()
        if not subject:
            QMessageBox.warning(self, "Потрібна назва", "Вкажіть назву предмета.")
            return

        url = self.url_edit.text().strip() or None
        if url and not is_openable(url):
            QMessageBox.warning(
                self, "Хибне посилання", "Посилання має починатися з http:// або https://."
            )
            return

        start = _from_qtime(self.start_edit.time())
        end = _from_qtime(self.end_edit.time())
        if end <= start:
            QMessageBox.warning(
                self, "Хибний час", "Кінець пари має бути пізніше за її початок."
            )
            return

        pair = pair_slot(start)
        mode = self.repeat_mode
        on_date = repeat_until = None
        if mode == REPEAT_WEEKLY:
            day_index = self.day_combo.currentData()
        else:
            on_date = _from_qdate(self.date_edit.date())
            day_index = on_date.weekday()
            if mode == REPEAT_UNTIL:
                repeat_until = _from_qdate(self.until_edit.date())
                if repeat_until < on_date:
                    QMessageBox.warning(
                        self, "Хибна дата", "Серія не може закінчитися раніше, ніж починається."
                    )
                    return
        teacher = self.teacher_edit.text().strip()
        if teacher and not teacher.startswith("("):
            teacher = f"({teacher})"

        if self.lesson:
            self.lesson.subject = subject
            self.lesson.teacher = teacher
            self.lesson.day_index = day_index
            self.lesson.on_date = on_date
            self.lesson.repeat_until = repeat_until
            self.lesson.pair = pair
            self.lesson.start_time = start
            self.lesson.end_time = end
            self.lesson.url = url
            self.lesson.provider = detect_provider(url)
            self.lesson.needs_link = not url
            self.storage.update_lesson(self.lesson)
        else:
            settings = self.storage.settings()
            group = self.storage.group(self.group_id)
            new_lesson = Lesson(
                id=0,
                course_id=group.course_id if group else settings.selected_course_id or 0,
                day_index=day_index,
                pair=pair,
                start_time=start,
                end_time=end,
                subject=subject,
                teacher=teacher,
                url=url,
                provider=detect_provider(url),
                needs_link=not url,
                is_manual=True,
                on_date=on_date,
                repeat_until=repeat_until,
            )
            self.storage.add_lesson(new_lesson, [self.group_id])
        self.accept()
