"""What the app does with the answer: ask, download, stay quiet -- and what the user sees."""

from __future__ import annotations

import pytest
from PySide6.QtWidgets import QLabel, QMessageBox, QPushButton

from autopara.core.models import UPDATE_ASK, UPDATE_AUTO, UPDATE_OFF
from autopara.core.scheduler import Scheduler
from autopara.core.storage import Storage
from autopara.core.update_service import UpdateService
from autopara.core.updater import Asset, Release, UpdateError

ASSET = Asset("AutoPara-1.6.0-Setup.exe", "https://github.com/x/y/releases/download/v1.6.0/a.exe", 5)


def release(version="1.6.0", asset=ASSET, notes="Що нового\n- день, тиждень, місяць") -> Release:
    return Release(version, f"v{version}", notes, "https://github.com/sevcenkoa864-oss/AutoPara/releases/tag/v1.6.0", asset)


@pytest.fixture
def storage(tmp_path):
    s = Storage(tmp_path / "u.db")
    yield s
    s.close()


class Probe:
    """Records every signal a service emits."""

    def __init__(self, service):
        self.events: list[tuple] = []
        service.available.connect(lambda r: self.events.append(("available", r.version)))
        service.downloading.connect(lambda r, p: self.events.append(("downloading", p)))
        service.ready.connect(lambda r, path: self.events.append(("ready", path)))
        service.up_to_date.connect(lambda manual: self.events.append(("current", manual)))
        service.failed.connect(lambda msg, manual: self.events.append(("failed", manual)))

    def kinds(self):
        return [e[0] for e in self.events]


def make(storage, tmp_path, found=None, error=None, frozen=True, platform="win32", file_error=None, **kw):
    calls = {"fetch": 0, "download": 0}

    def fetch(**_):
        calls["fetch"] += 1
        if error:
            raise error
        return found

    def fetch_file(asset, directory, progress=None, **_):
        calls["download"] += 1
        if file_error:
            raise file_error
        progress(5, 10)
        progress(10, 10)
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / asset.name
        path.write_bytes(b"x")
        return path

    service = UpdateService(
        storage, current="1.5.0", fetch=fetch, fetch_file=fetch_file, run=lambda job: job(),
        platform=platform, frozen=frozen, directory=tmp_path / "updates", **kw,
    )
    return service, Probe(service), calls


class TestChecking:
    def test_a_newer_version_is_announced(self, storage, tmp_path, qapp):
        service, probe, _ = make(storage, tmp_path, found=release())
        service.check_now(manual=False)
        assert probe.events == [("available", "1.6.0")]

    def test_the_same_or_an_older_version_is_not(self, storage, tmp_path, qapp):
        for version in ("1.5.0", "1.4.0"):
            service, probe, _ = make(storage, tmp_path, found=release(version))
            service.check_now(manual=True)
            assert probe.events == [("current", True)]

    def test_no_release_at_all_is_up_to_date(self, storage, tmp_path, qapp):
        service, probe, _ = make(storage, tmp_path, found=None)
        service.check_now(manual=False)
        assert probe.events == [("current", False)]

    def test_the_time_of_the_check_is_remembered(self, storage, tmp_path, qapp):
        service, _, _ = make(storage, tmp_path, found=None)
        assert storage.settings().last_update_check == ""
        service.check_now()
        assert storage.settings().last_update_check.startswith("20")

    def test_a_failed_check_says_whether_anyone_asked(self, storage, tmp_path, qapp):
        service, probe, _ = make(storage, tmp_path, error=UpdateError("no route"))
        service.check_now(manual=False)
        service.check_now(manual=True)
        assert probe.events == [("failed", False), ("failed", True)]

    def test_a_check_can_run_again_after_a_failure(self, storage, tmp_path, qapp):
        service, probe, calls = make(storage, tmp_path, error=UpdateError("x"))
        service.check_now()
        service.check_now()
        assert calls["fetch"] == 2

    def test_a_crash_in_the_worker_becomes_a_failure_not_a_dead_service(self, storage, tmp_path, qapp):
        service, probe, calls = make(storage, tmp_path, error=RuntimeError("bug"))
        service.check_now(manual=True)
        assert probe.events == [("failed", True)]
        service.check_now(manual=True)
        assert calls["fetch"] == 2


class TestWhatTheUserAsked:
    def test_off_means_no_automatic_check_but_a_manual_one_still_works(self, storage, tmp_path, qapp):
        storage.set_setting("update_mode", UPDATE_OFF)
        service, probe, calls = make(storage, tmp_path, found=release())
        service.check_now(manual=False)
        assert calls["fetch"] == 0
        service.check_now(manual=True)
        assert probe.events == [("available", "1.6.0")]

    def test_off_does_not_even_start_the_timer(self, storage, tmp_path, qapp):
        storage.set_setting("update_mode", UPDATE_OFF)
        service, _, _ = make(storage, tmp_path, found=None)
        service.start()
        assert not service._timer.isActive()

    def test_ask_never_downloads_by_itself(self, storage, tmp_path, qapp):
        storage.set_setting("update_mode", UPDATE_ASK)
        service, probe, calls = make(storage, tmp_path, found=release())
        service.check_now(manual=False)
        assert calls["download"] == 0 and probe.kinds() == ["available"]

    def test_auto_downloads_and_then_waits_for_the_user(self, storage, tmp_path, qapp):
        storage.set_setting("update_mode", UPDATE_AUTO)
        service, probe, calls = make(storage, tmp_path, found=release())
        service.check_now(manual=False)
        assert calls["download"] == 1
        assert probe.kinds() == ["downloading", "downloading", "downloading", "ready"]
        assert probe.events[-1][1].endswith("AutoPara-1.6.0-Setup.exe")
        # Nothing was installed: the service has no way to do that without being asked.

    def test_auto_still_only_announces_when_this_build_cannot_install_itself(self, storage, tmp_path, qapp):
        storage.set_setting("update_mode", UPDATE_AUTO)
        service, probe, calls = make(storage, tmp_path, found=release(), frozen=False)
        service.check_now(manual=False)
        assert calls["download"] == 0 and probe.kinds() == ["available"]

    def test_auto_only_announces_when_the_release_has_no_file_for_us(self, storage, tmp_path, qapp):
        storage.set_setting("update_mode", UPDATE_AUTO)
        service, probe, calls = make(storage, tmp_path, found=release(asset=None))
        service.check_now(manual=False)
        assert calls["download"] == 0 and probe.kinds() == ["available"]

    def test_a_skipped_version_stays_quiet_until_someone_asks(self, storage, tmp_path, qapp):
        service, probe, _ = make(storage, tmp_path, found=release())
        service.skip(release())
        service.check_now(manual=False)
        assert probe.events == []
        service.check_now(manual=True)
        assert probe.events == [("available", "1.6.0")]

    def test_a_skipped_version_does_not_hide_the_next_one(self, storage, tmp_path, qapp):
        service, probe, _ = make(storage, tmp_path, found=release("1.7.0"))
        service.skip(release("1.6.0"))
        service.check_now(manual=False)
        assert probe.events == [("available", "1.7.0")]


class TestDownloading:
    def test_the_download_is_by_request(self, storage, tmp_path, qapp):
        service, probe, calls = make(storage, tmp_path, found=release())
        service.download_release(release())
        assert probe.kinds()[-1] == "ready" and calls["download"] == 1

    def test_percent_is_reported_once_per_step(self, storage, tmp_path, qapp):
        service, probe, _ = make(storage, tmp_path, found=release())
        service.download_release(release())
        assert [e[1] for e in probe.events if e[0] == "downloading"] == [0, 50, 100]

    def test_a_failed_download_is_reported_and_can_be_retried(self, storage, tmp_path, qapp):
        service, probe, calls = make(storage, tmp_path, found=release(), file_error=UpdateError("bad"))
        service.download_release(release())
        assert probe.events[-1] == ("failed", True)
        service.download_release(release())
        assert calls["download"] == 2

    def test_nothing_to_download_for_a_release_without_a_file(self, storage, tmp_path, qapp):
        service, probe, calls = make(storage, tmp_path, found=release())
        service.download_release(release(asset=None))
        assert calls["download"] == 0 and probe.events == []

    def test_a_build_that_cannot_install_does_not_download(self, storage, tmp_path, qapp):
        service, probe, calls = make(storage, tmp_path, found=release(), frozen=False)
        service.download_release(release())
        assert calls["download"] == 0


class TestApplying:
    def test_the_service_applies_only_through_the_updater(self, storage, tmp_path, qapp, monkeypatch):
        service, _, _ = make(storage, tmp_path, found=release())
        seen = []
        monkeypatch.setattr("autopara.core.update_service.updater.apply",
                            lambda path, directory, platform: seen.append((path.name, directory)) or True)
        assert service.apply(str(tmp_path / "updates" / "AutoPara-1.6.0-Setup.exe")) is True
        assert seen == [("AutoPara-1.6.0-Setup.exe", tmp_path / "updates")]


class TestInTheWindow:
    @pytest.fixture(autouse=True)
    def no_modal_boxes(self, monkeypatch):
        """A real message box would block the suite forever: every one is recorded instead."""
        self.shown = []
        for name, kind in (("information", "info"), ("warning", "warn"), ("critical", "crit")):
            monkeypatch.setattr(
                QMessageBox, name, lambda *a, _kind=kind, **k: self.shown.append((_kind, a[1]))
            )

    @pytest.fixture
    def window(self, storage, tmp_path, qapp):
        from autopara.ui.main_window import MainWindow

        service, probe, calls = make(storage, tmp_path, found=release())
        win = MainWindow(storage, Scheduler(storage))
        win.attach_updates(service)
        win.service, win.probe, win.calls = service, probe, calls
        win.show()
        return win

    def test_a_new_version_shows_the_strip_with_a_download_button(self, window):
        window.service.check_now(manual=False)
        banner = window.update_banner
        assert banner.isVisible() and "1.6.0" in banner.title.text()
        assert banner.main_button.text() == "Завантажити"
        assert banner.skip_button.isVisibleTo(banner) and banner.later_button.isVisibleTo(banner)

    def test_the_download_button_downloads_and_the_strip_offers_to_install(self, window):
        window.service.check_now(manual=False)
        window.update_banner.main_button.click()
        assert window.calls["download"] == 1
        banner = window.update_banner
        assert "завантажено" in banner.title.text()
        assert banner.main_button.text() == "Встановити й перезапустити"

    def test_install_hands_the_file_to_the_app_and_does_nothing_else(self, window):
        window.service.check_now(manual=False)
        window.update_banner.main_button.click()
        wanted = []
        window.install_requested.connect(wanted.append)
        window.update_banner.main_button.click()
        assert len(wanted) == 1 and wanted[0].endswith("AutoPara-1.6.0-Setup.exe")

    def test_later_hides_it_and_skip_remembers(self, window, storage):
        window.service.check_now(manual=False)
        window.update_banner.later_button.click()
        assert not window.update_banner.isVisible()
        window.service.check_now(manual=False)
        window.update_banner.skip_button.click()
        assert storage.settings().skipped_update_version == "1.6.0"
        assert not window.update_banner.isVisible()

    def test_a_manual_check_with_nothing_new_says_so(self, window):
        shown = self.shown
        window.service._fetch = lambda **_: None
        window.check_updates_manually()
        assert shown == [("info", "Оновлень немає")]

    def test_an_automatic_check_with_nothing_new_is_silent(self, window):
        shown = self.shown
        window.service._fetch = lambda **_: None
        window.service.check_now(manual=False)
        assert shown == []

    def test_a_failed_manual_check_says_so_in_ukrainian(self, window):
        shown = self.shown
        window.service._fetch = lambda **_: (_ for _ in ()).throw(UpdateError("no route"))
        window.check_updates_manually()
        assert shown == [("warn", "Не вдалося оновити")]

    def test_a_failed_automatic_check_is_silent(self, window):
        shown = self.shown
        window.service._fetch = lambda **_: (_ for _ in ()).throw(UpdateError("no route"))
        window.service.check_now(manual=False)
        assert shown == []

    def test_a_failed_download_returns_the_strip_to_the_offer(self, window):
        window.service.check_now(manual=False)
        window.service._fetch_file = lambda *a, **k: (_ for _ in ()).throw(UpdateError("bad"))
        window.update_banner.main_button.click()
        assert window.update_banner.main_button.text() == "Завантажити"

    def test_a_build_that_cannot_install_offers_the_release_page(self, storage, tmp_path, qapp):
        from autopara.ui.main_window import MainWindow

        service, _, _ = make(storage, tmp_path, found=release(), frozen=False)
        win = MainWindow(storage, Scheduler(storage))
        win.attach_updates(service)
        win.show()
        service.check_now(manual=False)
        assert win.update_banner.main_button.text() == "Відкрити сторінку"

    def test_a_mac_is_told_to_drag_the_app_across_not_that_it_will_restart(self, storage, tmp_path, qapp):
        from autopara.ui.main_window import MainWindow

        service, _, _ = make(storage, tmp_path, found=release(), platform="darwin")
        win = MainWindow(storage, Scheduler(storage))
        win.attach_updates(service)
        win.show()
        service.check_now(manual=False)
        win.update_banner.main_button.click()
        assert win.update_banner.main_button.text() == "Відкрити образ"
        assert "Програми" in win.update_banner.detail.text()

    def test_everything_on_the_strip_is_ukrainian(self, window):
        window.service.check_now(manual=False)
        banner = window.update_banner
        for widget in banner.findChildren(QLabel) + banner.findChildren(QPushButton):
            for word in widget.text().replace("«", " ").replace("»", " ").split():
                assert not (word.isascii() and word.isalpha() and len(word) > 2 and word != "AutoPara"), word

    def test_the_strip_shows_only_the_first_line_of_the_notes(self, window):
        window.service.check_now(manual=False)
        assert window.update_banner.detail.text() == "Що нового"


class TestSettings:
    def test_the_choice_is_saved(self, storage, qapp):
        from autopara.ui.settings_dialog import SettingsDialog

        dialog = SettingsDialog(storage)
        assert dialog.update_combo.currentData() == UPDATE_ASK
        dialog.update_combo.setCurrentIndex(dialog.update_combo.findData(UPDATE_AUTO))
        dialog._accept()
        assert storage.settings().update_mode == UPDATE_AUTO

    def test_check_now_saves_the_choice_first_then_checks(self, storage, qapp):
        from autopara.ui.settings_dialog import SettingsDialog

        seen = []
        dialog = SettingsDialog(storage, check_updates=lambda: seen.append(storage.settings().update_mode))
        dialog.update_combo.setCurrentIndex(dialog.update_combo.findData(UPDATE_OFF))
        dialog.check_button.click()
        assert seen == [UPDATE_OFF]

    def test_without_a_service_the_button_is_disabled(self, storage, qapp):
        from autopara.ui.settings_dialog import SettingsDialog

        assert not SettingsDialog(storage).check_button.isEnabled()

    def test_the_running_version_is_shown(self, storage, qapp):
        from autopara import __version__
        from autopara.ui.settings_dialog import SettingsDialog

        assert SettingsDialog(storage).version_label.text() == f"Версія {__version__}"

    def test_a_garbled_stored_mode_reads_as_ask(self, storage):
        storage.set_setting("update_mode", "sometimes")
        assert storage.settings().update_mode == UPDATE_ASK
