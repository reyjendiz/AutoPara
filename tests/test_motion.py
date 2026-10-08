"""Motion: morphing between periods and views, and popups growing into place.

Everything else in the suite runs with motion switched off (see ``conftest._no_motion``); these
tests switch it on, with short durations, and check three things: that the state has changed before
the first frame, that the pictures really are where the progress says, and that nothing is left
behind when the motion ends or is overtaken.
"""

from __future__ import annotations

import time
from datetime import date, timedelta

import pytest
from PySide6.QtCore import Qt
from PySide6.QtGui import QColor, QPixmap
from PySide6.QtWidgets import QDialog, QMenu, QWidget

from autopara.core import theme
from autopara.core.models import THEME_DARK, THEME_LIGHT, VIEW_MONTH, VIEW_WEEK
from autopara.ui import popups, transitions
from autopara.ui.class_card import ClassCard
from tests.test_views import MONDAY, window, world  # noqa: F401  (fixtures)


@pytest.fixture(autouse=True)
def motion_on(monkeypatch):
    monkeypatch.setattr(transitions, "ENABLED", True)
    monkeypatch.setattr(transitions, "MORPH_MS", 40)
    monkeypatch.setattr(transitions, "FADE_MS", 40)


def wait_until(qapp, condition, seconds=2.0) -> bool:
    deadline = time.monotonic() + seconds
    while time.monotonic() < deadline:
        qapp.processEvents()
        if condition():
            return True
        time.sleep(0.005)
    return condition()


def solid(colour: str, width=120, height=80) -> QPixmap:
    pixmap = QPixmap(width, height)
    pixmap.fill(QColor(colour))
    return pixmap


class TestFade:
    def test_a_window_starts_transparent_and_ends_opaque(self, qapp):
        dialog = QDialog()
        animation = transitions.fade_in(dialog, 40)
        assert animation is not None and dialog.windowOpacity() == 0.0
        assert wait_until(qapp, lambda: dialog.windowOpacity() == 1.0)

    def test_a_child_widget_is_not_faded(self, qapp):
        parent = QWidget()
        child = QWidget(parent)
        assert transitions.fade_in(child) is None

    def test_it_does_nothing_when_motion_is_off(self, qapp, monkeypatch):
        monkeypatch.setattr(transitions, "ENABLED", False)
        dialog = QDialog()
        assert transitions.fade_in(dialog) is None
        assert dialog.windowOpacity() == 1.0

    def test_a_second_fade_replaces_the_first(self, qapp):
        dialog = QDialog()
        first = transitions.fade_in(dialog, 400)
        second = transitions.fade_in(dialog, 40)
        assert first is not second
        assert wait_until(qapp, lambda: dialog.windowOpacity() == 1.0)


def bare_dialog() -> QDialog:
    """A window with no title bar, like a menu or a tip: the kind that can be grown from a picture."""
    return QDialog(None, Qt.FramelessWindowHint)


class TestPopupsGrowIn:
    """A popup is photographed, the photograph grows out of where the popup belongs, and the real
    popup takes over in the same place."""

    def test_a_bare_window_grows_in_when_it_is_shown(self, qapp):
        popups.install(qapp)
        dialog = bare_dialog()
        dialog.resize(300, 200)
        dialog.show()
        assert getattr(dialog, "_morph", None) is not None and dialog._ghost is not None
        assert wait_until(qapp, lambda: dialog.windowOpacity() == 1.0)
        assert wait_until(qapp, lambda: dialog._ghost is None), "the picture is gone afterwards"
        dialog.close()

    def test_a_dialog_with_a_title_bar_only_fades(self, qapp):
        """A photograph holds a window's contents and not its frame, which would pop in at the end."""
        popups.install(qapp)
        dialog = QDialog()
        dialog.resize(300, 200)
        dialog.show()
        assert getattr(dialog, "_ghost", None) is None
        assert getattr(dialog, "_fade", None) is not None
        assert wait_until(qapp, lambda: dialog.windowOpacity() == 1.0)
        dialog.close()

    def test_a_menu_grows_in_when_it_is_shown(self, qapp):
        popups.install(qapp)
        menu = QMenu()
        menu.addAction("Відкрити")
        menu.show()
        assert getattr(menu, "_morph", None) is not None
        assert wait_until(qapp, lambda: menu.windowOpacity() == 1.0)
        menu.close()

    def test_the_real_popup_is_invisible_while_its_picture_grows(self, qapp):
        dialog = bare_dialog()
        dialog.resize(300, 200)
        dialog.show()
        animation = transitions.morph_in(dialog)
        assert dialog.windowOpacity() == 0.0, "only the picture shows meanwhile"
        assert dialog._ghost.isVisible()
        assert wait_until(qapp, lambda: dialog.windowOpacity() == 1.0)
        assert animation is dialog._morph
        dialog.close()

    def test_the_picture_starts_small_around_its_origin_and_ends_full_size(self, qapp):
        from PySide6.QtCore import QPoint

        dialog = bare_dialog()
        dialog.resize(400, 300)
        dialog.show()
        animation = transitions.morph_in(dialog, QPoint(dialog.geometry().left(), dialog.geometry().top()))
        ghost = dialog._ghost
        animation.stop()
        ghost.set_progress(0.0)
        start = ghost.current_rect()
        assert start.width() == pytest.approx(400 * transitions.POPUP_FROM)
        assert start.topLeft().x() == pytest.approx(0) and start.topLeft().y() == pytest.approx(0), (
            "grows out of the corner it was asked to grow from"
        )
        ghost.set_progress(1.0)
        end = ghost.current_rect()
        assert (end.width(), end.height()) == (400, 300)
        transitions._close_ghost(dialog)
        dialog.close()

    def test_a_dialog_grows_from_its_middle(self, qapp):
        dialog = bare_dialog()
        dialog.resize(400, 300)
        dialog.show()
        animation = transitions.morph_in(dialog)
        animation.stop()
        ghost = dialog._ghost
        ghost.set_progress(0.0)
        rect = ghost.current_rect()
        assert rect.center().x() == pytest.approx(200, abs=1.5)
        assert rect.center().y() == pytest.approx(150, abs=1.5)
        transitions._close_ghost(dialog)
        dialog.close()

    def test_a_pointer_outside_the_popup_is_clamped_to_its_nearest_edge(self, qapp):
        from PySide6.QtCore import QPoint

        dialog = bare_dialog()
        dialog.resize(400, 300)
        dialog.show()
        far = QPoint(dialog.geometry().right() + 500, dialog.geometry().bottom() + 500)
        animation = transitions.morph_in(dialog, far)
        animation.stop()
        ghost = dialog._ghost
        ghost.set_progress(0.0)
        rect = ghost.current_rect()
        assert rect.right() == pytest.approx(399, abs=1.5), "anchored at the bottom-right corner"
        transitions._close_ghost(dialog)
        dialog.close()

    def test_it_does_nothing_when_motion_is_off(self, qapp, monkeypatch):
        monkeypatch.setattr(transitions, "ENABLED", False)
        dialog = bare_dialog()
        dialog.show()
        assert transitions.morph_in(dialog) is None
        assert dialog.windowOpacity() == 1.0
        dialog.close()

    def test_a_child_widget_is_not_morphed(self, qapp):
        parent = QWidget()
        assert transitions.morph_in(QWidget(parent)) is None

    def test_a_popup_with_no_size_falls_back_to_a_plain_fade(self, qapp):
        dialog = bare_dialog()
        dialog.setGeometry(0, 0, 0, 0)
        animation = transitions.morph_in(dialog)
        assert animation is not None and getattr(dialog, "_ghost", None) is None
        assert wait_until(qapp, lambda: dialog.windowOpacity() == 1.0)

    def test_a_second_morph_replaces_the_first_and_leaves_no_picture_behind(self, qapp):
        dialog = bare_dialog()
        dialog.resize(300, 200)
        dialog.show()
        transitions.morph_in(dialog)
        first = dialog._ghost
        transitions.morph_in(dialog)
        assert dialog._ghost is not first
        assert wait_until(qapp, lambda: dialog.windowOpacity() == 1.0)
        dialog.close()

    def test_a_popup_destroyed_mid_morph_does_not_raise(self, qapp):
        dialog = bare_dialog()
        dialog.resize(300, 200)
        dialog.show()
        transitions.morph_in(dialog)
        ghost = dialog._ghost
        dialog.deleteLater()
        dialog.close()
        for _ in range(20):
            qapp.processEvents()
            time.sleep(0.01)
        try:
            ghost.close()
        except RuntimeError:
            pass  # it had already closed itself

    def test_a_tooltip_grows_in_but_does_not_blink_between_tips(self, qapp):
        from PySide6.QtCore import QPoint

        styler = popups.install(qapp)
        styler.tip.hide()
        styler.tip.show_text("Перша", QPoint(200, 200))
        first = styler.tip._morph
        assert wait_until(qapp, lambda: styler.tip.windowOpacity() == 1.0)
        styler.tip.show_text("Друга", QPoint(220, 210))  # already showing: just moves and re-words
        assert styler.tip._morph is first
        assert styler.tip.windowOpacity() == 1.0
        styler.tip.hide()


def element(lesson_id, day, x, y, w=20, h=20, colour="#00ff00") -> transitions.Element:
    from PySide6.QtCore import QRect

    return transitions.Element(lesson_id, day, QRect(x, y, w, h), solid(colour, w, h))


def snap(colour, elements=(), first=None) -> transitions.Snapshot:
    return transitions.Snapshot(solid(colour), list(elements), first)


class TestMorphOverlay:
    """Two snapshots of one surface, one becoming the other."""

    DAY = date(2026, 3, 4)

    def overlay(self, qapp, old, new, direction=0, same_view=True):
        stage = QWidget()
        stage.resize(120, 80)
        overlay = transitions.MorphOverlay(stage, old, direction, same_view)
        overlay.setGeometry(0, 0, 120, 80)
        overlay._new = new
        overlay._match()
        stage.show()
        overlay.show()
        qapp.processEvents()
        return stage, overlay

    def pixel(self, overlay, x=60, y=40) -> QColor:
        return QColor(overlay.grab().toImage().pixel(x, y))

    # ---------------------------------------------------------- the empty calendar

    def test_it_starts_on_the_old_calendar_and_ends_on_the_new_one(self, qapp):
        stage, overlay = self.overlay(qapp, snap("#ff0000"), snap("#0000ff"))
        overlay.set_progress(0.0)
        assert self.pixel(overlay).red() > 240 and self.pixel(overlay).blue() < 15
        overlay.set_progress(1.0)
        assert self.pixel(overlay).blue() > 240 and self.pixel(overlay).red() < 15
        stage.close()

    def test_in_between_both_calendars_show(self, qapp):
        stage, overlay = self.overlay(qapp, snap("#ff0000"), snap("#0000ff"), direction=0)
        overlay.set_progress(0.5)
        colour = self.pixel(overlay)
        assert 100 < colour.red() < 160 and 100 < colour.blue() < 160
        stage.close()

    def halves(self) -> QPixmap:
        """Left half blue, right half green: where the boundary is says how far the calendar drifted."""
        pixmap = QPixmap(120, 80)
        pixmap.fill(QColor("#0000ff"))
        from PySide6.QtGui import QPainter

        painter = QPainter(pixmap)
        painter.fillRect(60, 0, 60, 80, QColor("#00ff00"))
        painter.end()
        return pixmap

    def test_the_new_calendar_drifts_in_from_the_side_time_is_moving_towards(self, qapp):
        new = transitions.Snapshot(self.halves())
        old = snap("#ff0000")
        stage, forward = self.overlay(qapp, old, new, direction=1)
        forward.set_progress(0.5)  # drifted 7 px: the boundary has moved right of 60
        pixel = QColor(forward.grab().toImage().pixel(64, 40))
        assert pixel.blue() > pixel.green(), "forward: the boundary is to the right of where it ends"
        stage.close()
        stage, backward = self.overlay(qapp, old, new, direction=-1)
        backward.set_progress(0.5)  # the boundary has moved left of 60
        pixel = QColor(backward.grab().toImage().pixel(56, 40))
        assert pixel.green() > pixel.blue()
        stage.close()

    def test_a_change_of_view_does_not_drift(self, qapp):
        new = transitions.Snapshot(self.halves())
        stage, overlay = self.overlay(qapp, snap("#ff0000"), new, direction=0)
        overlay.set_progress(0.5)
        left = QColor(overlay.grab().toImage().pixel(56, 40))
        right = QColor(overlay.grab().toImage().pixel(64, 40))
        assert left.blue() > left.green() and right.green() > right.blue()
        stage.close()

    # ------------------------------------------------------------------ matching

    def test_the_same_lesson_in_the_same_place_is_one_class(self, qapp):
        old = snap("#808080", [element(1, self.DAY, 10, 10)], first=self.DAY)
        new = snap("#808080", [element(1, self.DAY + timedelta(days=7), 70, 40)],
                   first=self.DAY + timedelta(days=7))
        stage, overlay = self.overlay(qapp, old, new, same_view=True)
        assert len(overlay._pairs) == 1 and not overlay._leaving and not overlay._arriving
        stage.close()

    def test_a_lesson_in_a_different_place_is_a_different_class(self, qapp):
        old = snap("#808080", [element(1, self.DAY, 10, 10)], first=self.DAY)
        new = snap("#808080", [element(1, self.DAY + timedelta(days=8), 70, 40)],
                   first=self.DAY + timedelta(days=7))  # a day later in the week
        stage, overlay = self.overlay(qapp, old, new, same_view=True)
        assert not overlay._pairs and len(overlay._leaving) == 1 and len(overlay._arriving) == 1
        stage.close()

    def test_a_different_lesson_in_the_same_place_is_a_different_class(self, qapp):
        old = snap("#808080", [element(1, self.DAY, 10, 10)], first=self.DAY)
        new = snap("#808080", [element(2, self.DAY, 10, 10)], first=self.DAY)
        stage, overlay = self.overlay(qapp, old, new)
        assert not overlay._pairs
        stage.close()

    def test_across_views_the_same_lesson_on_the_same_date_is_one_class(self, qapp):
        """A chip in the month and the card in the week for the same class on the same date."""
        old = snap("#808080", [element(1, self.DAY, 10, 10, 30, 10)], first=date(2026, 3, 1))
        new = snap("#808080", [element(1, self.DAY, 40, 20, 60, 50)], first=date(2026, 3, 2))
        stage, overlay = self.overlay(qapp, old, new, same_view=False)
        assert len(overlay._pairs) == 1
        stage.close()

    def test_across_views_the_same_lesson_on_another_date_is_not(self, qapp):
        old = snap("#808080", [element(1, self.DAY, 10, 10)], first=date(2026, 3, 1))
        new = snap("#808080", [element(1, self.DAY + timedelta(days=7), 10, 10)], first=date(2026, 3, 2))
        stage, overlay = self.overlay(qapp, old, new, same_view=False)
        assert not overlay._pairs
        stage.close()

    # -------------------------------------------------------------------- motion

    def matched(self, qapp):
        old = snap("#808080", [element(1, self.DAY, 10, 10)], first=self.DAY)
        new = snap("#808080", [element(1, self.DAY, 80, 50)], first=self.DAY)
        return self.overlay(qapp, old, new)

    def test_a_class_that_is_on_both_sides_travels_from_where_it_was_to_where_it_is(self, qapp):
        stage, overlay = self.matched(qapp)
        overlay.set_progress(0.0)
        assert self.pixel(overlay, 20, 20).green() > 240, "at its old place"
        assert abs(self.pixel(overlay, 90, 60).green() - 128) < 10, "its new place is still bare"
        overlay.set_progress(1.0)
        assert self.pixel(overlay, 90, 60).green() > 240, "at its new place"
        assert abs(self.pixel(overlay, 20, 20).green() - 128) < 10
        stage.close()

    def test_halfway_it_is_halfway_between(self, qapp):
        stage, overlay = self.matched(qapp)
        overlay.set_progress(0.5)
        assert self.pixel(overlay, 55, 40).green() > 240  # the middle of the straight line between
        assert abs(self.pixel(overlay, 20, 20).green() - 128) < 10, "it has left its old place"
        stage.close()

    def test_it_changes_size_on_the_way(self, qapp):
        old = snap("#808080", [element(1, self.DAY, 10, 10, 20, 20)], first=self.DAY)
        new = snap("#808080", [element(1, self.DAY, 10, 10, 80, 20)], first=self.DAY)
        stage, overlay = self.overlay(qapp, old, new)
        overlay.set_progress(0.5)  # 50 wide now: reaches x = 59
        assert self.pixel(overlay, 55, 20).green() > 240
        assert abs(self.pixel(overlay, 85, 20).green() - 128) < 10
        stage.close()

    def test_a_class_only_on_the_old_calendar_shrinks_away(self, qapp):
        old = snap("#808080", [element(1, self.DAY, 40, 30)], first=self.DAY)
        stage, overlay = self.overlay(qapp, old, snap("#808080", first=self.DAY))
        overlay.set_progress(0.0)
        assert self.pixel(overlay, 50, 40).green() > 240
        overlay.set_progress(1.0)
        assert abs(self.pixel(overlay, 50, 40).green() - 128) < 10, "gone by the end"
        stage.close()

    def test_a_class_only_on_the_new_calendar_grows_in(self, qapp):
        new = snap("#808080", [element(1, self.DAY, 40, 30)], first=self.DAY)
        stage, overlay = self.overlay(qapp, snap("#808080", first=self.DAY), new)
        overlay.set_progress(0.0)
        assert abs(self.pixel(overlay, 50, 40).green() - 128) < 10, "not there yet"
        overlay.set_progress(1.0)
        assert self.pixel(overlay, 50, 40).green() > 240
        stage.close()

    def test_it_lets_every_click_through(self, qapp):
        stage, overlay = self.overlay(qapp, snap("#ff0000"), snap("#0000ff"))
        assert overlay.testAttribute(Qt.WA_TransparentForMouseEvents)
        stage.close()

    def test_progress_is_clamped(self, qapp):
        stage, overlay = self.overlay(qapp, snap("#ff0000"), snap("#0000ff"))
        overlay.set_progress(7)
        assert overlay.progress == 1.0
        overlay.set_progress(-3)
        assert overlay.progress == 0.0
        stage.close()

    def test_run_morphs_and_then_goes_away(self, qapp):
        stage = QWidget()
        stage.resize(120, 80)
        overlay = transitions.MorphOverlay(stage, snap("#ff0000"), 1)
        overlay.setGeometry(0, 0, 120, 80)
        stage.show()
        overlay.show()
        done = []
        overlay.finished.connect(lambda: done.append(True))
        overlay.run(snap("#0000ff"))
        assert wait_until(qapp, lambda: done)
        stage.close()


class TestSnapshots:
    """The calendar photographed in two layers."""

    @pytest.fixture(autouse=True)
    def styled(self, qapp):
        # An unstyled card is as grey as the grid under it, and in the light theme a white card sits
        # on a white grid: only the dark theme gives the pixels something to differ by.
        theme.apply(qapp, THEME_DARK)
        yield
        theme.apply(qapp, THEME_LIGHT)

    def test_a_week_is_the_empty_grid_plus_its_classes(self, window, qapp):
        window.show()
        qapp.processEvents()
        surface = window.grid
        cards = [c for c in surface.findChildren(ClassCard) if c.isVisible()]
        assert cards, "the fixture has a class in this week"

        shot = transitions.snapshot(surface)

        assert len(shot.elements) == len(cards)
        assert shot.first_day == surface.morph_first_day()
        assert all(e.rect.intersects(surface.rect()) for e in shot.elements)
        assert all(not e.picture.isNull() for e in shot.elements)
        assert all(c.isVisible() for c in cards), "the classes are shown again afterwards"

    def test_the_background_has_no_classes_on_it(self, window, qapp):
        window.show()
        qapp.processEvents()
        surface = window.grid
        shot = transitions.snapshot(surface)
        element = shot.elements[0]
        live = surface.grab().toImage()
        empty = shot.background.toImage()
        point = element.rect.center()
        assert QColor(live.pixel(point)) != QColor(empty.pixel(point)), "the class was left out"

    def test_a_month_is_the_tiles_plus_its_chips(self, window, qapp):
        window.storage.set_setting("view_mode", VIEW_MONTH)
        window.reload()
        window.show()
        qapp.processEvents()
        shot = transitions.snapshot(window.month)
        chips = [c for t in window.month.tiles for c in t.chips]
        assert chips and len(shot.elements) == len(chips)
        assert shot.first_day == window.month.tiles[0].day
        assert all(c.isVisible() for c in chips)

    def test_a_surface_that_names_no_classes_is_just_a_picture(self, qapp):
        plain = QWidget()
        plain.resize(50, 40)
        plain.show()
        shot = transitions.snapshot(plain)
        assert shot.elements == [] and not shot.background.isNull() and shot.first_day is None


class TestNavigationAnimates:
    def shown(self, window, qapp):
        window.show()
        qapp.processEvents()
        return window

    def test_the_state_changes_before_any_frame_of_the_motion(self, window, qapp):
        self.shown(window, qapp)
        window._step(1)
        assert window.anchor == MONDAY + timedelta(days=7), "moved at once, not when it finishes"
        assert window._overlay is not None

    def test_the_overlay_covers_exactly_the_surface_and_then_goes_away(self, window, qapp):
        self.shown(window, qapp)
        window._step(1)
        overlay = window._overlay
        assert overlay.geometry() == window.grid.geometry()
        assert wait_until(qapp, lambda: window._overlay is None)
        assert window.grid.isVisibleTo(window)

    def test_a_newer_move_finishes_the_older_one(self, window, qapp):
        self.shown(window, qapp)
        window._step(1)
        first = window._overlay
        window._step(1)
        assert window.anchor == MONDAY + timedelta(days=14)
        assert window._overlay is not first
        assert wait_until(qapp, lambda: window._overlay is None)

    def test_a_window_that_is_not_on_screen_just_moves(self, window, qapp):
        window._step(1)
        assert window.anchor == MONDAY + timedelta(days=7)
        assert window._overlay is None

    def test_nothing_animates_when_motion_is_off(self, window, qapp, monkeypatch):
        self.shown(window, qapp)
        monkeypatch.setattr(transitions, "ENABLED", False)
        window._step(1)
        assert window._overlay is None

    def record(self, window, monkeypatch):
        seen: list[tuple[int, bool]] = []
        monkeypatch.setattr(
            transitions, "play",
            lambda surface, old, direction, same_view=True: seen.append((direction, same_view)),
        )
        return seen

    def test_forward_is_plus_one_and_back_is_minus_one(self, window, qapp, monkeypatch):
        self.shown(window, qapp)
        seen = self.record(window, monkeypatch)
        window._step(1)
        window._step(-1)
        assert seen == [(1, True), (-1, True)]

    def test_today_travels_the_way_today_is(self, window, qapp, monkeypatch):
        self.shown(window, qapp)
        seen = self.record(window, monkeypatch)
        window.anchor = date.today() - timedelta(days=30)
        window.go_today()
        window.anchor = date.today() + timedelta(days=30)
        window.go_today()
        assert seen == [(1, True), (-1, True)]

    def test_changing_the_view_morphs_by_date_not_by_place(self, window, qapp, monkeypatch):
        self.shown(window, qapp)
        seen = self.record(window, monkeypatch)
        window.set_view_mode(VIEW_MONTH)
        assert seen == [(0, False)]
        assert window.view_mode() == VIEW_MONTH

    def test_choosing_the_view_already_showing_does_not_animate(self, window, qapp, monkeypatch):
        self.shown(window, qapp)
        seen = self.record(window, monkeypatch)
        window.set_view_mode(VIEW_WEEK)
        assert seen == []

    def test_every_view_can_be_reached_with_motion_on(self, window, qapp):
        self.shown(window, qapp)
        for mode in ("month", "day", "week"):
            window.set_view_mode(mode)
            assert wait_until(qapp, lambda: window._overlay is None)
            assert window.view_mode() == mode


class TestSegmentedPill:
    """The selected pill of День / Тиждень / Місяць is its own widget and travels between them."""

    @pytest.fixture
    def bar(self, qapp, monkeypatch):
        from autopara.ui import nav_bar
        from autopara.ui.nav_bar import NavBar

        monkeypatch.setattr(nav_bar, "THUMB_MS", 60)
        if qapp.styleSheet() == "":  # the segments get their exact 32 px from the stylesheet
            theme.apply(qapp, THEME_LIGHT)
        bar = NavBar()
        bar.resize(900, 60)
        bar.show()
        bar.set_view("week")
        qapp.processEvents()
        yield bar
        bar.close()

    def on(self, bar):
        return [m for m, b in bar.view_buttons.items() if b.property("on") == "true"]

    def test_the_pill_sits_exactly_behind_the_chosen_segment(self, bar, qapp):
        week = bar.view_buttons["week"]
        assert bar._thumb.geometry() == week.geometry()
        assert not bar._thumb.isHidden()
        assert self.on(bar) == ["week"]

    def test_choosing_another_segment_makes_the_pill_travel_there(self, bar, qapp):
        start = bar._thumb.geometry()
        bar.view_buttons["month"].click()
        assert bar._glide is not None, "it travels rather than jumping"
        qapp.processEvents()
        assert bar._thumb.geometry() != bar.view_buttons["month"].geometry() or bar._glide is None
        assert wait_until(qapp, lambda: bar._thumb.geometry() == bar.view_buttons["month"].geometry())
        assert bar._thumb.geometry() != start

    def test_the_text_stays_readable_all_the_way(self, bar, qapp):
        """Pale text on the arriving pill, or dark text on the bare bar, is unreadable for a moment:
        the segment the pill left stays lit until the pill is nearer the new one."""
        assert self.on(bar) == ["week"]
        bar.view_buttons["month"].click()
        assert self.on(bar) == ["week"], "still the old one at the very start"
        assert wait_until(qapp, lambda: self.on(bar) == ["month"])
        assert bar._thumb.geometry().center().x() > bar.view_buttons["week"].geometry().center().x()

    def test_it_never_leaves_two_segments_lit(self, bar, qapp):
        for mode in ("month", "day", "week", "month"):
            bar.view_buttons[mode].click()
            assert len(self.on(bar)) <= 1
        assert wait_until(qapp, lambda: self.on(bar) == ["month"])

    def test_the_pill_follows_the_segments_when_the_bar_is_resized(self, bar, qapp):
        bar.resize(1200, 60)
        qapp.processEvents()
        assert bar._thumb.geometry() == bar.view_buttons["week"].geometry()

    def test_without_motion_it_jumps_at_once(self, bar, qapp, monkeypatch):
        monkeypatch.setattr(transitions, "ENABLED", False)
        bar.view_buttons["day"].click()
        assert bar._glide is None
        assert bar._thumb.geometry() == bar.view_buttons["day"].geometry()
        assert self.on(bar) == ["day"]

    def test_setting_the_view_from_the_window_also_moves_it(self, bar, qapp):
        bar.set_view("day")
        assert bar._glide is not None
        assert wait_until(qapp, lambda: bar._thumb.geometry() == bar.view_buttons["day"].geometry())

    def test_the_pill_is_a_full_round_end(self, bar, qapp):
        assert bar._thumb.height() == 32
        stage = bar.findChild(type(bar._thumb.parent()), "Segmented")
        assert stage.height() == 40


class TestHoverFadeAndClock:
    def test_the_plus_fill_eases_in_and_out_instead_of_switching(self, qapp, monkeypatch):
        from PySide6.QtCore import QEvent, QPointF
        from PySide6.QtGui import QEnterEvent

        from autopara.ui import transitions
        from autopara.ui.transitions import FadeButton

        monkeypatch.setattr(transitions, "ENABLED", True)
        button = FadeButton("", rest="rail_primary_bg")
        button.show()
        qapp.processEvents()
        point = QPointF(5, 5)
        button.enterEvent(QEnterEvent(point, point, point))
        assert button._fade is not None and button._fade.duration() == transitions.HOVER_MS
        assert 0.0 <= button._level < 1.0, "the fill must not jump on the first frame"
        button._fade.setCurrentTime(transitions.HOVER_MS)
        assert button._level == 1.0
        button.leaveEvent(QEvent(QEvent.Leave))
        button._fade.setCurrentTime(transitions.HOVER_MS)
        assert button._level == 0.0
        button.close()

    def test_the_hover_tint_is_a_small_step_from_rest(self, qapp):
        from autopara.core import theme
        from autopara.ui.transitions import FadeButton
        from PySide6.QtGui import QColor

        button = FadeButton("", rest="rail_primary_bg")
        rest = QColor(theme.token("rail_primary_bg")).lightness()
        button._level = 1.0
        assert 0 < rest - button._fill().lightness() <= 20

    def test_the_clock_shows_the_system_time_at_half_opacity(self, qapp):
        from datetime import datetime

        from autopara.ui.clock_bar import OPACITY, ClockBar, clock_text

        bar = ClockBar()
        assert OPACITY == 0.5 and bar.graphicsEffect().opacity() == 0.5
        assert len(bar.text()) == 8 and bar.text().count(":") == 2
        assert clock_text(datetime(2026, 1, 2, 3, 4, 5)) == "03:04:05"
