"""Головне вікно: панель інструментів, банер пропущеної пари, тижнева сітка та рядок стану.

Режиму редагування немає: сітка редагована завжди. Лівий клік по парі підключає до неї, права
кнопка відкриває меню дій, клік по порожній клітинці пропонує створити пару, перетягування
переносить пару в інший день або слот.
"""

from __future__ import annotations

from datetime import date, datetime, timedelta

from PySide6.QtCore import QSize, Qt, Signal
from PySide6.QtGui import QKeySequence, QShortcut
from PySide6.QtWidgets import (
    QApplication,
    QFrame,
    QHBoxLayout,
    QLabel,
    QMainWindow,
    QMenu,
    QMessageBox,
    QPushButton,
    QVBoxLayout,
    QWidget,
)

from .. import __version__
from ..core import filters, theme
from ..core.launcher import open_url
from ..core.models import (
    STATUS_MISSED,
    STATUS_OPENED,
    STATUS_SKIPPED,
    VIEW_DAY,
    VIEW_MODES,
    VIEW_MONTH,
    Lesson,
)
from ..core.scheduler import Scheduler
from ..core.storage import Storage
from ..importer.normalize import (
    DAY_NAMES,
    MONTH_GENITIVE,
    MONTH_NAMES,
    add_minutes,
    minutes_between,
    pair_slot,
    week_start,
)
from . import icons, transitions
from .catchup_banner import CatchupBanner
from .clock_bar import ClockBar
from .edit_dialog import EditDialog
from .filter_bar import FilterBar
from .group_dialog import GroupDialog
from .import_landing import ImportLanding
from .month_view import MonthView, first_of, shift_month
from .nav_bar import NavBar, week_parts
from .settings_dialog import SettingsDialog
from .update_banner import UpdateBanner
from .setup_dialog import SetupDialog
from .tray import icon_pixmap
from .week_grid import WeekGrid


# A rail, not a panel: with no text on it, its width is the width of one button plus air.
# The pill is centred in the rail and the content starts right where the rail ends, so the air on
# either side of the pill is the same: (SIDEBAR_WIDTH - RAIL_PILL_WIDTH) / 2 = 14 px.
SIDEBAR_WIDTH = 80
RAIL_PILL_WIDTH = 52


class MainWindow(QMainWindow):
    """Постійно редагована сітка тижня з навігацією по тижнях."""

    closed_to_tray = Signal()
    install_requested = Signal(str)  # a verified download the user chose to install

    def __init__(self, storage: Storage, scheduler: Scheduler):
        super().__init__()
        self.storage = storage
        self.scheduler = scheduler
        # What the window is looking at. This is the only state it keeps: the lessons themselves are
        # read from storage on every reload, so there is nothing here that can go stale.
        self.anchor = date.today()
        self._overlay: transitions.SlideOverlay | None = None
        self.updates = None  # the UpdateService, once the app attaches one
        self._view_group_id: int | None = None  # a group of the course other than the chosen one
        self._last_chosen_group_id: int | None = None
        self.setWindowTitle("AutoPara — автозапуск пар")
        # Wide enough that the sidebar does not cost the grid a day column.
        self.resize(1240, 800)
        self._build()

        self.scheduler.lesson_opened.connect(self._lesson_opened)
        self.scheduler.lesson_missed.connect(lambda _: self.reload())

    # ------------------------------------------------------------------ build

    def _build(self) -> None:
        central = QWidget()
        central.setObjectName("Canvas")
        columns = QHBoxLayout(central)
        columns.setContentsMargins(0, 0, 0, 0)
        columns.setSpacing(0)

        columns.addWidget(self._build_sidebar())

        content = QWidget()
        content.setObjectName("Canvas")
        layout = QVBoxLayout(content)
        # Cards float on the canvas, so the content has a margin of its own and the cards have
        # room between them; the rail is its own column and keeps its own air.
        layout.setContentsMargins(0, 20, 24, 24)
        layout.setSpacing(16)

        self.nav = NavBar()
        self.nav.previous_requested.connect(lambda: self._step(-1))
        self.nav.next_requested.connect(lambda: self._step(1))
        self.nav.today_requested.connect(self.go_today)
        self.nav.view_requested.connect(self.set_view_mode)
        layout.addWidget(self.nav)

        self.filterbar = FilterBar()
        self.filterbar.changed.connect(self._filters_changed)
        self.filterbar.jump_requested.connect(self._jump_to_match)
        layout.addWidget(self.filterbar)
        QShortcut(QKeySequence.Find, self, activated=self.filterbar.focus_search)

        self.update_banner = UpdateBanner(__version__)
        self.update_banner.page_requested.connect(open_url)
        layout.addWidget(self.update_banner)

        self.banner = CatchupBanner()
        self.banner.connect_requested.connect(self._catchup_connect)
        self.banner.dismissed.connect(self._catchup_dismiss)
        layout.addWidget(self.banner)

        self.grid = WeekGrid()
        self.grid.lesson_clicked.connect(self._lesson_clicked)
        self.grid.lesson_menu_requested.connect(self._lesson_menu_requested)
        self.grid.slot_clicked.connect(self._slot_clicked)
        self.grid.lesson_dropped.connect(self._lesson_dropped)
        layout.addWidget(self.grid, 1)

        self.month = MonthView()
        self.month.day_selected.connect(self.open_day)
        self.month.add_requested.connect(lambda day: self._edit_lesson(None, day=day))
        self.month.lesson_dropped.connect(self._month_dropped)
        self.month.hide()
        layout.addWidget(self.month, 1)

        self.landing = ImportLanding()
        self.landing.file_dropped.connect(lambda path: self.open_import(path))
        self.landing.browse_requested.connect(self.open_import)
        self.landing.hide()
        layout.addWidget(self.landing, 1)
        # Решта вікна зверталася просто до порожнього напису; він і далі тут, усередині екрана
        # імпорту, тож ніщо навколо не мусить знати, що він переїхав.
        self.empty_label = self.landing.empty_label

        columns.addWidget(content, 1)
        self.setCentralWidget(central)
        # Після бічної панелі та екрана імпорту: значки малюються в кольорі теми, для обох одразу.
        self._refresh_icons()

    def _build_sidebar(self) -> QWidget:
        """Дії живуть у вузькій колонці ліворуч, а не в смузі згори.

        Сітка тижня -- широка й невисока, тож горизонтальна панель забирала саме ту висоту, якої
        календарю бракує. У колонці немає жодного слова: сам застосунок не мусить називати себе у
        власному вікні, а що робить кожна кнопка -- каже підказка під курсором. Кнопки зібрані в
        чорну «пігулку», що висить над полотном: це єдиний темний якір на екрані.
        """
        bar = QFrame()
        bar.setObjectName("Sidebar")
        bar.setFixedWidth(SIDEBAR_WIDTH)
        column = QVBoxLayout(bar)
        column.setContentsMargins(0, 24, 0, 24)
        column.setSpacing(12)

        # Значок замість слова «AutoPara». Обраний курс і група більше не написані на панелі --
        # вони не змінюються тижнями, а місце займали постійно; тепер це підказка на значку.
        self.brand = QLabel()
        self.brand.setObjectName("Brand")
        self.brand.setPixmap(icon_pixmap(30))
        self.brand.setAlignment(Qt.AlignCenter)
        column.addWidget(self.brand, 0, Qt.AlignHCenter)

        column.addStretch(1)

        pill = QFrame()
        pill.setObjectName("RailPill")
        pill.setFixedWidth(RAIL_PILL_WIDTH)
        stack = QVBoxLayout(pill)
        stack.setContentsMargins(0, 8, 0, 8)
        stack.setSpacing(2)

        self.add_button = self._icon_button(
            "Додати пару",
            lambda: self._edit_lesson(None, day=self.anchor),
            button=transitions.FadeButton("", rest="rail_primary_bg"),
        )
        self.add_button.setObjectName("PrimaryRound")
        self.add_button.setFixedSize(40, 40)
        stack.addWidget(self.add_button, 0, Qt.AlignHCenter)
        stack.addSpacing(6)

        # Три значки стоять щільно, щоб читалися однією групою дрібних дій, а не трьома
        # знаками, що розбрелися під кнопкою над ними.
        self.import_button = self._icon_button("Імпортувати розклад…", self.open_import)
        stack.addWidget(self.import_button, 0, Qt.AlignHCenter)

        self.group_button = self._icon_button("Обрати інший курс і групу…", self.choose_group)
        stack.addWidget(self.group_button, 0, Qt.AlignHCenter)

        self.settings_button = self._icon_button("Налаштування", self.open_settings)
        stack.addWidget(self.settings_button, 0, Qt.AlignHCenter)

        self.theme_button = self._icon_button("", self.toggle_theme)
        stack.addWidget(self.theme_button, 0, Qt.AlignHCenter)

        column.addWidget(pill, 0, Qt.AlignHCenter)

        self.clock = ClockBar()
        column.addWidget(self.clock, 0, Qt.AlignHCenter)
        return bar

    def _icon_button(self, tooltip: str, handler, button: QPushButton | None = None) -> QPushButton:
        """Кнопка-значок 36x36 -- зручна для миші й не перетворює панель на суцільні кнопки.

        Текст лишається порожнім назавжди: значок ставить ``_refresh_icons`` після кожної зміни
        теми, бо значки малюються кодом і мають перефарбовуватися разом із нею.
        """
        button = button or QPushButton("")
        button.setObjectName("IconButton")
        button.setFixedSize(36, 36)
        button.setIconSize(QSize(20, 20))
        button.setToolTip(tooltip)
        button.clicked.connect(handler)
        return button

    # ------------------------------------------------------------------- week

    def monday(self) -> date:
        """Понеділок тижня, який зараз на екрані (за замовчуванням -- поточного)."""
        return week_start(self.anchor)

    def date_of(self, lesson: Lesson) -> date:
        """Календарна дата цієї пари на тижні, що на екрані -- до неї прив'язуються позначки."""
        return self.monday() + timedelta(days=lesson.day_index)

    def _surface(self) -> QWidget | None:
        """The calendar surface on screen -- the week grid or the month -- or None."""
        for candidate in (self.grid, self.month):
            if candidate.isVisible():
                return candidate
        return None

    def _animated(self, direction: int, change, same_view: bool = True) -> None:
        """Run ``change`` (which moves the calendar) and morph from the old picture to the new.

        ``direction`` is +1 for forward in time, -1 for back and 0 for a change of view;
        ``same_view`` is False when the change swaps day / week / month for another, so a class is
        followed by its date rather than by its place on screen. The change is made first and in
        full: the animation only dresses it, so nothing about the window depends on it finishing --
        or on it being seen, which is why it is skipped for a window that is not on screen.
        """
        surface = self._surface() if transitions.ENABLED and self.isVisible() else None
        if surface is None:
            change()
            return
        if self._overlay is not None:
            self._overlay.finish()  # a newer move wins; the surface underneath is already current
        old = transitions.snapshot(surface)
        change()
        current = self._surface()
        if current is None:
            return
        self.centralWidget().layout().activate()
        self._overlay = transitions.play(current, old, direction, same_view)
        if self._overlay is not None:
            self._overlay.finished.connect(self._overlay_gone)

    def _overlay_gone(self) -> None:
        self._overlay = None

    def go_today(self) -> None:
        today = date.today()
        direction = (today > self.anchor) - (today < self.anchor)

        def change() -> None:
            self.anchor = today
            self.reload()

        self._animated(direction, change)

    def view_mode(self) -> str:
        """Day, week or month -- read from storage each time, like everything else."""
        return self.storage.settings().view_mode

    def set_view_mode(self, mode: str) -> None:
        if mode not in VIEW_MODES:
            return

        def change() -> None:
            self.storage.set_setting("view_mode", mode)
            self.reload()

        if mode == self.view_mode():
            change()  # the view that is already showing: just reload, nothing to slide between
        else:
            self._animated(0, change, same_view=False)

    def open_day(self, day: date) -> None:
        """Go to a single day -- what clicking a day in the month does."""
        self.anchor = day
        self.set_view_mode(VIEW_DAY)

    def _step(self, direction: int) -> None:
        """A day, a week or a month back or forward, according to the view."""

        def change() -> None:
            mode = self.view_mode()
            if mode == VIEW_DAY:
                self.anchor += timedelta(days=direction)
            elif mode == VIEW_MONTH:
                self.anchor = shift_month(self.anchor, direction)
            else:
                self.anchor += timedelta(weeks=direction)
            self.reload()

        self._animated(direction, change)

    # ----------------------------------------------------------------- render

    def _shown_group_id(self) -> int | None:
        """The group whose classes are on screen: the chosen one, or another of its course."""
        chosen = self.storage.settings().selected_group_id
        if chosen and self._view_group_id and self._view_group_id != chosen:
            picked, mine = self.storage.group(self._view_group_id), self.storage.group(chosen)
            if picked is not None and mine is not None and picked.course_id == mine.course_id:
                return picked.id
        return chosen

    def group_lessons(self) -> list[Lesson]:
        """Усі пари групи на екрані, незалежно від того, який тиждень на екрані й що знайдено."""
        group_id = self._shown_group_id()
        if not group_id:
            return []
        return self.storage.lessons_for_group(group_id)

    def shown_lessons(self) -> list[Lesson]:
        """Пари групи, що проходять пошук і фільтр, -- це й малюють усі три вигляди."""
        return filters.apply(self.group_lessons(), self.filterbar.current_filter())

    def current_lessons(self) -> list[Lesson]:
        """Пари, що трапляються на тижні, який на екрані: датована пара належить не кожному."""
        return [lesson for lesson in self.shown_lessons() if lesson.occurs_on(self.date_of(lesson))]

    def week_statuses(self, lessons: list[Lesson]) -> dict[int, str]:
        """Стан кожної пари цього тижня (пара трапляється в тижні один раз)."""
        monday = self.monday()
        occurrences = self.storage.occurrences_between(monday, monday + timedelta(days=6))
        statuses: dict[int, str] = {}
        for lesson in lessons:
            key = (lesson.id, self.date_of(lesson).isoformat())
            occurrence = occurrences.get(key)
            if occurrence is not None:
                statuses[lesson.id] = occurrence.status
        return statuses

    def _keyed_statuses(self, first: date, last: date) -> dict[tuple[int, str], str]:
        """Every verdict between two dates, keyed by (lesson id, ISO date)."""
        return {
            key: occurrence.status
            for key, occurrence in self.storage.occurrences_between(first, last).items()
        }

    def _title(self, mode: str) -> tuple[str, str]:
        """What the navigation bar says about the period on screen: ``(the period, the year)``."""
        if mode == VIEW_DAY:
            day = self.anchor
            text = f"{DAY_NAMES[day.weekday()]}, {day.day} {MONTH_GENITIVE[day.month - 1]}"
            return text, str(day.year)
        if mode == VIEW_MONTH:
            return MONTH_NAMES[self.anchor.month - 1], str(self.anchor.year)
        return week_parts(self.monday())

    def _show_only(self, widget: QWidget | None) -> None:
        """Show the calendar surface the view needs and hide the others."""
        for candidate in (self.grid, self.month):
            candidate.setVisible(candidate is widget)

    def reload(self) -> None:
        settings = self.storage.settings()
        group = (
            self.storage.group(settings.selected_group_id)
            if settings.selected_group_id
            else None
        )
        if settings.selected_group_id != self._last_chosen_group_id:
            # Обрали іншу групу (чи імпортували заново): "інша група курсу" більше не діє.
            self._last_chosen_group_id = settings.selected_group_id
            self._view_group_id = None
        everything = self.group_lessons()
        flt = self.filterbar.current_filter()
        shown = filters.apply(everything, flt)
        # Нема з чого обирати, доки нічого не імпортовано.
        self.group_button.setEnabled(bool(self.storage.courses()))

        if group is None:
            self._show_only(None)
            self.nav.hide()
            self.filterbar.hide()
            self.landing.show_no_schedule()
            self.landing.show()
            self.brand.setToolTip("Розклад ще не імпортовано")
            return
        if not everything and self._shown_group_id() == settings.selected_group_id:
            # Порожня група -- це порожній екран; а порожній *тиждень* лишається сіткою, бо
            # інакше з нього не було б як піти до тижня, де пари є.
            self._show_only(None)
            self.nav.hide()
            self.filterbar.hide()
            self.landing.set_state(
                "Тут поки порожньо",
                f"У групі {group.name} немає пар в імпортованому розкладі. "
                "Натисніть «Додати пару» або імпортуйте інший файл.",
            )
            self.landing.show()
            self._set_subtitle(group)
            return

        mode = settings.view_mode
        self.landing.hide()
        self.nav.show()
        self.filterbar.show()
        self._refresh_filterbar(group, everything, shown, flt)
        self.nav.set_view(mode)
        self.nav.set_title(*self._title(mode))
        unit = {VIEW_DAY: "день", VIEW_MONTH: "місяць"}.get(mode, "тиждень")
        self.nav.set_tooltips(f"Попередній {unit}", f"Наступний {unit}")
        today = date.today()

        if mode == VIEW_MONTH:
            self._show_only(self.month)
            first = first_of(self.anchor)
            days = self.month.visible_days(first)
            self.month.render_month(
                first, shown, self._keyed_statuses(days[0], days[-1]), today
            )
        elif mode == VIEW_DAY:
            self._show_only(self.grid)
            keyed = self._keyed_statuses(self.anchor, self.anchor)
            self.grid.render_days(
                [self.anchor],
                shown,
                keyed,
                self._next_key(shown, keyed) if self.anchor == today else None,
                today,
            )
        else:
            self._show_only(self.grid)
            lessons = self.current_lessons()
            statuses = self.week_statuses(lessons)
            self.grid.render_week(
                lessons,
                statuses=statuses,
                next_lesson_id=self._next_lesson_id(lessons, statuses),
                today=today,
                monday=self.monday(),
            )
        self._set_subtitle(group)

    # ------------------------------------------------------------ search and filters

    def _refresh_filterbar(self, group, everything, shown, flt) -> None:
        """Offer this group's subjects and this course's groups, and say how much was found."""
        self.filterbar.set_subjects(filters.subjects(everything))
        self.filterbar.set_groups(
            [(g.id, g.name) for g in self.storage.groups(group.course_id)],
            self._shown_group_id(),
        )
        if not flt.active:
            self.filterbar.set_summary("")
        elif not shown:
            self.filterbar.set_summary("Нічого не знайдено")
        else:
            self.filterbar.set_summary(f"Знайдено {len(shown)} з {len(everything)}")

    def _filters_changed(self) -> None:
        picked = self.filterbar.group_id()
        chosen = self.storage.settings().selected_group_id
        self._view_group_id = picked if picked and picked != chosen else None
        self.reload()

    def _visible_range(self) -> tuple[date, date]:
        mode = self.view_mode()
        if mode == VIEW_DAY:
            return self.anchor, self.anchor
        if mode == VIEW_MONTH:
            days = self.month.visible_days(first_of(self.anchor))
            return days[0], days[-1]
        return self.monday(), self.monday() + timedelta(days=6)

    def _jump_to_match(self, direction: int) -> None:
        """Enter у пошуку: перейти до найближчої дати з парою, що підходить (за межі екрана)."""
        flt = self.filterbar.current_filter()
        if not flt.active:
            return
        first, last = self._visible_range()
        found = filters.find_match(
            self.group_lessons(), flt, last if direction >= 0 else first, direction
        )
        if found is None:
            self.filterbar.set_summary("Збігів більше немає")
            return

        def change() -> None:
            self.anchor = found
            self.reload()

        self._animated(1 if direction >= 0 else -1, change)

    def _next_key(
        self, lessons: list[Lesson], keyed: dict[tuple[int, str], str]
    ) -> tuple[int, str] | None:
        """The class to outline in the day view: today's soonest one that has not been settled."""
        now = datetime.now()
        today = now.date()
        upcoming = [
            lesson
            for lesson in lessons
            if lesson.occurs_on(today)
            and lesson.ends_on(today) > now
            and (lesson.id, today.isoformat()) not in keyed
        ]
        if not upcoming:
            return None
        return min(upcoming, key=lambda lesson: lesson.start_time).id, today.isoformat()

    def _set_subtitle(self, group) -> None:
        """Курс, група і час відкриття -- підказка на значку, а не напис на панелі.

        Ці чотири рядки не змінюються тижнями, але місце на екрані займали постійно. Підказка
        показує їх тоді, коли про них справді питають.
        """
        settings = self.storage.settings()
        course = self.storage.course(group.course_id)
        parts = [group.name]
        if group.specialty:
            parts.append(group.specialty)
        if course:
            parts.insert(0, course.name)
        parts.append(f"відкриття за {settings.lead_minutes} хв")
        self.brand.setToolTip("\n".join(parts))

    def _next_lesson_id(self, lessons: list[Lesson], statuses: dict[int, str]) -> int | None:
        now = datetime.now()
        today = now.date()
        upcoming = [
            lesson
            for lesson in lessons
            if lesson.occurs_on(today)
            and lesson.ends_on(today) > now
            and lesson.id not in statuses
        ]
        if not upcoming:
            return None
        return min(upcoming, key=lambda lesson: lesson.start_time).id

    # ------------------------------------------------------------------ theme

    def _refresh_icons(self) -> None:
        """Перемальовує значки в кольорі активної теми. Підписів на цих кнопках немає."""
        dark = theme.is_dark()
        ink = theme.token("rail_icon")
        self.add_button.setIcon(icons.icon("add", theme.token("rail_primary_fg"), 22))
        self.import_button.setIcon(icons.icon("import", ink, 20))
        self.group_button.setIcon(icons.icon("group", ink, 20))
        self.settings_button.setIcon(icons.icon("settings", ink, 20))
        self.theme_button.setIcon(icons.icon("sun" if dark else "moon", ink, 20))
        self.theme_button.setToolTip(
            "Перемкнути на світлу тему" if dark else "Перемкнути на темну тему"
        )
        self.landing.repaint_glyphs()
        self.banner.repaint_glyph()
        self.update_banner.repaint_glyph()
        self.nav.repaint_glyphs()
        self.filterbar.repaint_glyphs()

    def apply_theme(self) -> None:
        app = QApplication.instance()
        if app is not None:
            theme.apply(app, self.storage.settings().theme)
        self._refresh_icons()
        self.reload()

    def toggle_theme(self) -> None:
        """Явно закріплює світлу або темну тему замість «як у системі»."""
        wanted = theme.next_theme(self.storage.settings().theme)
        self.storage.set_setting("theme", wanted)
        self.apply_theme()

    # ------------------------------------------------------- catch-up (банер)

    def offer_catchup(self, lesson_id: int) -> None:
        lesson = self.storage.lesson(lesson_id)
        if lesson is not None:
            self.banner.offer(lesson)

    def _catchup_connect(self, lesson_id: int) -> None:
        # A successful open reloads through ``lesson_opened``; reloading again here would rebuild
        # the whole grid a second time while the browser is starting.
        if not self.scheduler.open_now(lesson_id):
            QMessageBox.warning(
                self, "Не вдалося відкрити", "Посилання не вдалося відкрити у браузері."
            )
            self.reload()

    def _catchup_dismiss(self, lesson_id: int) -> None:
        """«Закрити» -- пара лишається пропущеною й більше не питає про себе сьогодні."""
        self.scheduler.mark(lesson_id, STATUS_MISSED)
        self.reload()

    def _lesson_opened(self, lesson_id: int) -> None:
        self.banner.resolve(lesson_id)
        self.reload()

    # ---------------------------------------------------------------- actions

    def _open_lesson(self, lesson: Lesson, day: date) -> bool:
        """Відкриває посилання пари на дату ``day``.

        Дата, якої ще не настало, не позначається відкритою: інакше планувальник вважав би, що ця
        пара вже була, і не відкрив би її, коли той день прийде.
        """
        return self.scheduler.open_now(lesson.id, day, claim=day <= date.today())

    def _lesson_clicked(self, lesson_id: int, day: date | None = None) -> None:
        """Лівий клік — підключитися до пари.

        Це те, заради чого програму відкривають, тож воно коштує один клік, а не клік плюс вибір
        у меню. Решта дій — на правій кнопці.
        """
        lesson = self.storage.lesson(lesson_id)
        if lesson is None:
            return
        day = day or self.date_of(lesson)
        if not lesson.url:
            # Відкривати нічого: показуємо меню, де перший пункт — додати посилання.
            self._show_lesson_menu(lesson, day)
            return

        # ``lesson_opened`` перезавантажує сітку сам, тож тут другого перемальовування немає.
        if not self._open_lesson(lesson, day):
            QMessageBox.warning(
                self, "Не вдалося відкрити", "Посилання не вдалося відкрити у браузері."
            )

    def _lesson_menu_requested(self, lesson_id: int, day: date | None = None) -> None:
        """Права кнопка — повне меню дій для пари."""
        lesson = self.storage.lesson(lesson_id)
        if lesson is not None:
            self._show_lesson_menu(lesson, day or self.date_of(lesson))

    def _show_lesson_menu(self, lesson: Lesson, day: date | None = None) -> None:
        """Одне меню дій для пари: відкрити / редагувати / позначити / видалити."""
        day = day or self.date_of(lesson)
        occurrence = self.storage.occurrence(lesson.id, day)

        menu = QMenu(self)
        if lesson.url:
            open_action = menu.addAction("Відкрити посилання")
            add_link_action = None
        else:
            open_action = None
            add_link_action = menu.addAction("Додати посилання…")
        edit_action = menu.addAction("Редагувати…")

        menu.addSeparator()
        mark_opened_action = menu.addAction("Позначити як відкриту")
        mark_skipped_action = menu.addAction("Позначити як пропущену")
        clear_action = menu.addAction("Зняти позначку") if occurrence else None

        menu.addSeparator()
        delete_action = menu.addAction("Видалити пару")

        chosen = menu.exec(self.cursor().pos())
        if chosen is None:
            return
        if chosen is open_action:
            # ``lesson_opened`` already reloads on success, so this branch returns rather than
            # rebuilding the grid a second time behind the opening browser.
            if self._open_lesson(lesson, day):
                return
            QMessageBox.warning(
                self, "Не вдалося відкрити", "Посилання не вдалося відкрити у браузері."
            )
        elif chosen is add_link_action or chosen is edit_action:
            self._edit_lesson(lesson)
        elif chosen is mark_opened_action:
            self.scheduler.mark(lesson.id, STATUS_OPENED, day)
        elif chosen is mark_skipped_action:
            self.scheduler.mark(lesson.id, STATUS_SKIPPED, day)
        elif clear_action is not None and chosen is clear_action:
            self.scheduler.clear_mark(lesson.id, day)
        elif chosen is delete_action:
            self._delete_lesson(lesson)
        self.reload()

    def _delete_lesson(self, lesson: Lesson) -> None:
        if lesson.repeat_until is not None:
            # A series is one row in the database, so deleting it removes every date. Say so, and
            # point at the way to drop a single date.
            question = (
                f"«{lesson.subject}» повторюється щотижня до {lesson.repeat_until.day} "
                f"{MONTH_GENITIVE[lesson.repeat_until.month - 1]}. Видалити всю серію?\n\n"
                "Щоб пропустити лише одну дату, оберіть «Позначити як пропущену»."
            )
        else:
            question = f"Видалити «{lesson.subject}» з розкладу?"
        confirm = QMessageBox.question(
            self,
            "Видалити пару",
            question,
            QMessageBox.Yes | QMessageBox.No,
        )
        if confirm == QMessageBox.Yes:
            self.storage.delete_lesson(lesson.id)

    def _slot_clicked(self, day: date, start: str) -> None:
        """Клік по вільній годині — як у Google Calendar: спливна пропозиція створити пару."""
        settings = self.storage.settings()
        if not settings.selected_group_id:
            QMessageBox.information(
                self, "Спершу імпорт", "Імпортуйте розклад, перш ніж додавати пари."
            )
            return
        menu = QMenu(self)
        create = menu.addAction(
            f"➕ Створити пару · {DAY_NAMES[day.weekday()]}, "
            f"{day.day} {MONTH_GENITIVE[day.month - 1]}, {start}"
        )
        menu.addSeparator()
        menu.addAction("Скасувати")
        if menu.exec(self.cursor().pos()) is create:
            self._edit_lesson(None, day=day, start_time=start)

    def _ask_move_scope(self, lesson: Lesson, from_day: date) -> str | None:
        """Пара повторюється: перенести лише цю дату чи всю серію? ``None`` -- передумали."""
        box = QMessageBox(self)
        box.setIcon(QMessageBox.Question)
        box.setWindowTitle("Перенести пару")
        box.setText(
            f"«{lesson.subject}» повторюється щотижня.\n\n"
            f"Перенести лише {from_day.day} {MONTH_GENITIVE[from_day.month - 1]} "
            "чи всю серію?"
        )
        one = box.addButton("Лише цю дату", QMessageBox.AcceptRole)
        series = box.addButton("Всю серію", QMessageBox.ActionRole)
        box.addButton("Скасувати", QMessageBox.RejectRole)
        box.setDefaultButton(one)
        box.exec()
        if box.clickedButton() is one:
            return "one"
        if box.clickedButton() is series:
            return "all"
        return None

    def _lesson_dropped(
        self, lesson_id: int, day: date, start: str, from_day: date | None = None
    ) -> None:
        """Перетягування переносить пару на іншу годину, зберігаючи її тривалість.

        ``from_day`` -- дата, з якої тягнули. Якщо пара повторюється, питаємо, що переносити:
        лише цю дату (решта серії лишається на місці) чи всю серію.
        """
        lesson = self.storage.lesson(lesson_id)
        if lesson is None:
            return
        duration = minutes_between(lesson.start_time, lesson.end_time) or (
            self.storage.settings().class_duration_minutes
        )
        if from_day is not None and lesson.repeats:
            if from_day == day and lesson.start_time == start:
                return  # dropped back where it was
            scope = self._ask_move_scope(lesson, from_day)
            if scope is None:
                return
            if scope == "one":
                self.storage.move_occurrence(
                    lesson.id,
                    from_day,
                    day,
                    pair_slot(start),
                    start,
                    add_minutes(start, duration),
                )
                self.reload()
                return
        if lesson.day_index == day.weekday() and lesson.start_time == start and not lesson.is_dated:
            return  # the whole series would not change
        if lesson.is_dated and lesson.on_date == day and lesson.start_time == start:
            return
        self.storage.move_lesson(
            lesson.id,
            day.weekday(),
            pair_slot(start),
            start,
            add_minutes(start, duration),
            on_date=day if lesson.is_dated else None,
        )
        self.reload()

    def _month_dropped(self, lesson_id: int, day: date, from_day: date | None) -> None:
        """Клас кинули на день у місяці: час лишається, змінюється лише дата."""
        lesson = self.storage.lesson(lesson_id)
        if lesson is not None:
            self._lesson_dropped(lesson_id, day, lesson.start_time, from_day)

    def _edit_lesson(
        self, lesson: Lesson | None, day: date | None = None, start_time: str | None = None
    ) -> None:
        settings = self.storage.settings()
        if not settings.selected_group_id:
            QMessageBox.information(
                self, "Спершу імпорт", "Імпортуйте розклад, перш ніж додавати пари."
            )
            return
        dialog = EditDialog(
            self.storage,
            settings.selected_group_id,
            lesson,
            self,
            day=day,
            start_time=start_time,
        )
        if dialog.exec():
            self.reload()

    def open_import(self, path: str | None = None) -> None:
        """``path`` -- файл, який щойно кинули на екран імпорту; діалог одразу його читає."""
        dialog = SetupDialog(self.storage, self, path=path)
        if dialog.exec():
            self.reload()

    # ----------------------------------------------------------------- updates

    def attach_updates(self, service) -> None:
        """Connect the update service: its news goes to the strip, the strip's choices come back."""
        self.updates = service
        service.available.connect(self._update_available)
        service.downloading.connect(self.update_banner.show_progress)
        service.ready.connect(self._update_ready)
        service.up_to_date.connect(self._update_current)
        service.failed.connect(self._update_failed)
        self.update_banner.download_requested.connect(service.download_release)
        self.update_banner.skipped.connect(service.skip)
        self.update_banner.install_requested.connect(self.install_requested.emit)

    def check_updates_manually(self) -> None:
        if self.updates is not None:
            self.updates.check_now(manual=True)

    def _update_available(self, release) -> None:
        self.update_banner.show_available(release, self.updates.installable)

    def _update_ready(self, release, path: str) -> None:
        self.update_banner.show_ready(release, path, restarts=self.updates.platform == "win32")

    def _update_current(self, manual: bool) -> None:
        if manual:
            QMessageBox.information(
                self, "Оновлень немає", f"У вас остання версія AutoPara — {__version__}."
            )

    def _update_failed(self, message: str, manual: bool) -> None:
        """A failed check is silent unless the user asked; a failed download always says so."""
        release = self.update_banner.release
        if release is not None and self.update_banner.bar.isVisible():
            self.update_banner.show_available(release, self.updates.installable)
        if manual:
            QMessageBox.warning(
                self,
                "Не вдалося оновити",
                "Не вдалося зв'язатися з GitHub або завантажити файл. "
                "Перевірте з'єднання з інтернетом і спробуйте ще раз.",
            )

    def choose_group(self) -> None:
        """Перемикає курс і групу серед уже імпортованих, не читаючи файл удруге."""
        dialog = GroupDialog(self.storage, self)
        if dialog.exec():
            self.reload()

    def open_settings(self) -> None:
        dialog = SettingsDialog(self.storage, self, check_updates=self.check_updates_manually)
        if dialog.exec():
            # Тема могла змінитися; apply_theme сам перемальовує сітку.
            self.apply_theme()

    # ----------------------------------------------------------------- window

    def closeEvent(self, event):  # noqa: N802 - Qt naming
        """Закриття вікна ховає його в трей; вихід — лише через «Вийти» у треї."""
        event.ignore()
        self.hide()
        self.closed_to_tray.emit()
