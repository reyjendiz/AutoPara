"""Rounded tooltips and menus: Qt will not round a popup's corners from a stylesheet."""

from __future__ import annotations

import pytest
from PySide6.QtCore import QEvent, QPoint, Qt
from PySide6.QtGui import QColor, QHelpEvent
from PySide6.QtWidgets import QApplication, QMenu, QPushButton

from autopara.core import theme
from autopara.core.models import THEME_DARK, THEME_LIGHT
from autopara.ui import popups


@pytest.fixture
def styler(qapp):
    return popups.install(qapp)


def ask_for_tooltip(widget) -> bool:
    event = QHelpEvent(QEvent.ToolTip, QPoint(4, 4), QPoint(120, 140))
    return QApplication.sendEvent(widget, event)


class TestTooltip:
    def test_a_tooltip_request_is_answered_by_the_rounded_one(self, styler, qapp):
        button = QPushButton("x")
        button.setToolTip("Середа, 7 жовтня · 17:00\nКлацніть, щоб створити пару")
        button.show()
        ask_for_tooltip(button)

        assert styler.tip.isVisible()
        assert styler.tip.label.text().startswith("Середа, 7 жовтня")
        styler.tip.hide()
        button.close()

    def test_a_widget_with_no_tooltip_shows_nothing(self, styler, qapp):
        button = QPushButton("x")
        button.show()
        ask_for_tooltip(button)
        assert not styler.tip.isVisible()
        button.close()

    def test_leaving_the_widget_dismisses_it(self, styler, qapp):
        button = QPushButton("x")
        button.setToolTip("Підказка")
        button.show()
        ask_for_tooltip(button)
        assert styler.tip.isVisible()
        QApplication.sendEvent(button, QEvent(QEvent.Leave))
        assert not styler.tip.isVisible()
        button.close()

    def test_a_click_dismisses_it_too(self, styler, qapp):
        button = QPushButton("x")
        button.setToolTip("Підказка")
        button.show()
        ask_for_tooltip(button)
        styler.eventFilter(button, QEvent(QEvent.MouseButtonPress))
        assert not styler.tip.isVisible()
        button.close()

    @pytest.mark.parametrize("name", [THEME_LIGHT, THEME_DARK])
    def test_the_plate_is_rounded_and_in_the_tooltip_colours(self, styler, qapp, name):
        if theme.active() != name or qapp.styleSheet() == "":
            theme.apply(qapp, name)
        try:
            styler.tip.show_text("Підказка", QPoint(200, 200))
            qapp.processEvents()
            image = styler.tip.grab().toImage()
            corner = QColor.fromRgba(image.pixel(0, 0))
            side = QColor(image.pixel(3, image.height() // 2))
            assert corner.alpha() == 0, "the corner must be see-through: that is what rounds it"
            assert side.name() == theme.PALETTES[name]["tooltip_bg"]
        finally:
            styler.tip.hide()

    def test_the_tip_stays_on_the_screen(self, styler, qapp):
        screen = qapp.primaryScreen().availableGeometry()
        styler.tip.show_text("Підказка " * 20, QPoint(screen.right() - 2, screen.bottom() - 2))
        assert styler.tip.geometry().right() <= screen.right()
        assert styler.tip.geometry().bottom() <= screen.bottom()
        styler.tip.hide()


class TestMenu:
    def test_a_menu_is_made_translucent_and_frameless_before_it_is_shown(self, styler, qapp):
        menu = QMenu()
        menu.addAction("Відкрити")
        menu.ensurePolished()
        assert menu.testAttribute(Qt.WA_TranslucentBackground)
        assert menu.windowFlags() & Qt.FramelessWindowHint

    def test_it_is_done_once(self, styler, qapp):
        menu = QMenu()
        menu.ensurePolished()
        flags = menu.windowFlags()
        styler.eventFilter(menu, QEvent(QEvent.Polish))
        assert menu.windowFlags() == flags

    def test_the_stylesheet_rounds_menus(self):
        assert "QMenu {" in theme.stylesheet(THEME_LIGHT)
        rendered = theme.stylesheet(THEME_LIGHT)
        block = rendered.split("QMenu {")[1].split("}")[0]
        assert "border-radius" in block
