"""Schedules the update check and tells the window what it found.

The network work (:mod:`autopara.core.updater`) runs on a worker thread so a slow connection can
never freeze the window; its answers come back as signals and are acted on in the main thread. The
service decides *what to do* about an answer -- ask, download, stay quiet -- from the
``update_mode`` setting, and keeps the user's "not this version" and the time of the last check in
storage like every other piece of state.
"""

from __future__ import annotations

import logging
import sys
import threading
from datetime import datetime
from pathlib import Path
from typing import Callable

from PySide6.QtCore import QObject, QTimer, Signal

from .. import __version__
from . import updater
from .models import UPDATE_AUTO, UPDATE_OFF
from .storage import Storage

log = logging.getLogger(__name__)

FIRST_CHECK_MS = 15_000                 # after start-up has settled, not during it
CHECK_EVERY_MS = 6 * 60 * 60 * 1000     # a long-running tray app checks a few times a day


def in_thread(job: Callable[[], None]) -> None:
    threading.Thread(target=job, daemon=True, name="autopara-update").start()


class UpdateService(QObject):
    available = Signal(object)          # Release -- a newer version exists
    downloading = Signal(object, int)   # Release, percent
    ready = Signal(object, str)         # Release, path of the verified download
    up_to_date = Signal(bool)           # manual -- the version running is the newest
    failed = Signal(str, bool)          # message, manual

    # Worker -> main thread. One signal, so every answer takes the same road.
    _outcome = Signal(str, object, bool)

    def __init__(
        self,
        storage: Storage,
        current: str = __version__,
        fetch: Callable[..., updater.Release | None] = updater.fetch_latest,
        fetch_file: Callable[..., Path] = updater.download,
        run: Callable[[Callable[[], None]], None] = in_thread,
        platform: str = sys.platform,
        frozen: bool | None = None,
        directory: Path | None = None,
        parent: QObject | None = None,
    ):
        super().__init__(parent)
        self.storage = storage
        self.current = current
        self._fetch = fetch
        self._fetch_file = fetch_file
        self._run = run
        self.platform = platform
        self.installable = updater.installable_here(platform, frozen)
        self.directory = directory or updater.updates_dir(storage.path)
        self._checking = False
        self._downloading = False
        self._timer = QTimer(self)
        self._timer.setInterval(CHECK_EVERY_MS)
        self._timer.timeout.connect(lambda: self.check_now(manual=False))
        self._outcome.connect(self._handle)

    # ---------------------------------------------------------------- schedule

    def start(self) -> None:
        """Begin the periodic check (unless the user switched it off)."""
        updater.tidy(self.directory, self.current)
        if self.storage.settings().update_mode == UPDATE_OFF:
            return
        QTimer.singleShot(FIRST_CHECK_MS, self, lambda: self.check_now(manual=False))
        self._timer.start()

    # ------------------------------------------------------------------- check

    def check_now(self, manual: bool = True) -> None:
        """Ask GitHub. An automatic check obeys the setting; a manual one always runs."""
        if self._checking:
            return
        if not manual and self.storage.settings().update_mode == UPDATE_OFF:
            return
        self._checking = True

        def job() -> None:
            try:
                self._outcome.emit("checked", self._fetch(platform=self.platform), manual)
            except updater.UpdateError as error:
                self._outcome.emit("failed", str(error), manual)
            except Exception as error:  # a bug must not become a silent dead timer
                log.exception("update check crashed")
                self._outcome.emit("failed", str(error), manual)

        self._run(job)

    # -------------------------------------------------------- answers (main thread)

    def _handle(self, kind: str, payload, manual: bool) -> None:
        if kind == "failed":
            self._checking = self._downloading = False
            log.info("update: %s", payload)
            self.failed.emit(str(payload), manual)
        elif kind == "checked":
            self._checking = False
            self._checked(payload, manual)
        elif kind == "progress":
            release, percent = payload
            self.downloading.emit(release, percent)
        elif kind == "downloaded":
            self._downloading = False
            release, path = payload
            self.ready.emit(release, str(path))

    def _checked(self, release: updater.Release | None, manual: bool) -> None:
        self.storage.set_setting("last_update_check", datetime.now().isoformat(timespec="seconds"))
        if release is None or not updater.is_newer(release.version, self.current):
            self.up_to_date.emit(manual)
            return
        settings = self.storage.settings()
        if release.version == settings.skipped_update_version and not manual:
            return  # they said "not this one"; a manual check asks again
        if settings.update_mode == UPDATE_AUTO and self.installable and release.asset:
            self.download_release(release)
        else:
            self.available.emit(release)

    # ---------------------------------------------------------------- download

    def download_release(self, release: updater.Release) -> None:
        """Fetch the installer for ``release``. Never runs it."""
        if not release.asset or not self.installable or self._downloading:
            return
        self._downloading = True
        asset = release.asset
        self.downloading.emit(release, 0)

        def job() -> None:
            last = -1

            def progress(done: int, total: int) -> None:
                nonlocal last
                percent = int(done * 100 / total) if total else 0
                if percent != last:  # one signal per percent, not one per chunk
                    last = percent
                    self._outcome.emit("progress", (release, percent), False)

            try:
                path = self._fetch_file(asset, self.directory, progress=progress)
                self._outcome.emit("downloaded", (release, path), False)
            except updater.UpdateError as error:
                self._outcome.emit("failed", str(error), True)
            except Exception as error:
                log.exception("update download crashed")
                self._outcome.emit("failed", str(error), True)

        self._run(job)

    def skip(self, release: updater.Release) -> None:
        """Remember "not this version": automatic checks stay quiet about it from now on."""
        self.storage.set_setting("skipped_update_version", release.version)

    def apply(self, path: str) -> bool:
        """Start the installer for a download this service made. True means: quit now."""
        return updater.apply(Path(path), self.directory, self.platform)
