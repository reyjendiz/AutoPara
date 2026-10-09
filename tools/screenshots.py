#!/usr/bin/env python3
"""Render the pictures in the README, from the synthetic sample timetable.

    python tools/screenshots.py

Nothing personal is in them: the timetable is ``tests/sample_timetable.py`` (invented subjects,
teachers and links), and the clock is pinned to a Monday morning so the "now" line, today's date and
the pill beside the title all show, and the pictures come out the same every time. Runs headless.
"""

from __future__ import annotations

import datetime as real
import os
import sys
import tempfile
from pathlib import Path

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))

PINNED = real.datetime(2026, 3, 2, 9, 40, 20)  # a Monday, in the middle of the second class
SIZE = (1240, 800)
OUT = ROOT / "docs"


def pin_the_clock() -> None:
    """Make ``today()`` and ``now()`` answer with ``PINNED`` in every module that asks."""

    class Date(real.date):
        @classmethod
        def today(cls):
            return PINNED.date()

    class DateTime(real.datetime):
        @classmethod
        def now(cls, tz=None):
            return PINNED

    from autopara.ui import clock_bar, main_window, week_grid

    for module in (main_window, week_grid):
        module.date, module.datetime = Date, DateTime
    clock_bar.datetime = DateTime


def main() -> int:
    from PySide6.QtWidgets import QApplication

    app = QApplication([])
    from autopara.app import _load_fonts, _smooth_text

    _load_fonts()
    _smooth_text(app)
    pin_the_clock()

    from autopara.core import theme
    from autopara.core.scheduler import Scheduler
    from autopara.core.storage import Storage
    from autopara.importer.schedule_parser import parse_file
    from autopara.ui import transitions
    from autopara.ui.main_window import MainWindow
    from tests import sample_timetable

    transitions.ENABLED = False  # a still picture: nothing may be mid-glide
    work = Path(tempfile.mkdtemp())
    sample_timetable.build(work / "sample.docx")
    storage = Storage(work / "demo.db")
    storage.import_courses(parse_file(str(work / "sample.docx")))
    course = storage.courses()[0]
    group = storage.groups(course.id)[0]
    storage.set_setting("selected_course_id", course.id)
    storage.set_setting("selected_group_id", group.id)

    shots = (
        ("week-grid.png", "light", "week"),
        ("week-grid-dark.png", "dark", "week"),
        ("month-view.png", "light", "month"),
        ("day-view.png", "light", "day"),
    )
    for name, theme_name, view in shots:
        theme.apply(app, theme_name)
        window = MainWindow(storage, Scheduler(storage))
        window.resize(*SIZE)
        window.anchor = PINNED.date()
        storage.set_setting("view_mode", view)
        window.show()
        window._refresh_icons()
        window.reload()
        app.processEvents()
        window.grab().save(str(OUT / name))
        print("wrote", OUT / name)
        window.close()

    # The two dialogs people ask about: changing a class, and the settings.
    from autopara.ui.edit_dialog import EditDialog
    from autopara.ui.settings_dialog import SettingsDialog

    theme.apply(app, "light")
    lesson = next(
        item for item in storage.lessons_for_group(group.id) if item.occurs_on(PINNED.date())
    )
    for name, dialog in (
        ("edit-dialog.png", EditDialog(storage, group.id, lesson, None, day=PINNED.date())),
        ("settings.png", SettingsDialog(storage)),
    ):
        dialog.show()
        for _ in range(3):  # let the form lay itself out before the picture is taken
            app.processEvents()
            dialog.layout().activate()
        dialog.resize(dialog.sizeHint().expandedTo(dialog.minimumSizeHint()))
        for _ in range(3):
            app.processEvents()
        dialog.grab().save(str(OUT / name))
        print("wrote", OUT / name)
        dialog.close()
    storage.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
