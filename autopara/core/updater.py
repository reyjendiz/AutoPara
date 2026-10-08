"""Finding, fetching and applying a newer release from GitHub.

Everything here is Qt-free and takes its network as an argument, so it can be tested without one;
``core/update_service.py`` is the part that schedules it and talks to the window.

What it will and will not do, because a program that downloads and runs an executable has to be
careful about both:

* It talks to **GitHub's HTTPS endpoints only**. The release is read from the API of the one
  repository in ``GITHUB_REPO``; the file it will fetch must live under that repository's
  ``releases/download/`` path; and every redirect on the way (GitHub serves downloads from a CDN)
  must stay on a GitHub host. Nothing a release page *says* can point the download elsewhere.
* It **verifies what it downloaded**: the size the release states and, when GitHub publishes one,
  the SHA-256 digest. A file that does not match is deleted, never kept and never run.
* It **never installs on its own**. Downloading may be automatic; applying an update is always the
  user's click. And it applies only a file it downloaded itself, from its own folder.
* Where it cannot install (running from source, or a platform whose release has no installer
  asset) it says a version is out and points at the release page.
"""

from __future__ import annotations

import hashlib
import json
import logging
import os
import platform as platform_module
import re
import ssl
import subprocess
import sys
import urllib.error
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Callable
from urllib.parse import urlparse

from .. import __version__

log = logging.getLogger(__name__)

GITHUB_REPO = "sevcenkoa864-oss/AutoPara"
API_URL = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
PAGE_PREFIX = f"https://github.com/{GITHUB_REPO}/"
DOWNLOAD_PREFIX = f"https://github.com/{GITHUB_REPO}/releases/download/"
TRUSTED_HOST_SUFFIXES = ("github.com", "githubusercontent.com")

WINDOWS_ASSET = re.compile(r"^AutoPara-.+-Setup\.exe$")
MAC_ASSET = re.compile(r"^AutoPara.*\.dmg$")

# A Mac gets the image built for its processor. The Apple Silicon image keeps the name it has always
# had (``AutoPara-<version>.dmg``) so every release already out, and every copy that looks for it,
# keeps working; the Intel one is ``AutoPara-Intel-<version>.dmg``.
#
# The word goes *before* the version on purpose. GitHub lists a release's files alphabetically, and
# copies of AutoPara older than the Intel image take the first ``.dmg`` they find: ``AutoPara-1.8.0-
# Intel.dmg`` sorts before ``AutoPara-1.8.0.dmg`` and an Apple Silicon Mac would have been offered the
# Intel one, while ``AutoPara-Intel-1.9.0.dmg`` sorts after every ``AutoPara-<digit>...`` name.
INTEL_WORD = "Intel"
_INTEL_MACHINES = ("x86_64", "amd64", "i386")


def is_intel_machine(machine: str | None = None) -> bool:
    return (machine or platform_module.machine()).lower() in _INTEL_MACHINES


def is_intel_image(name: str) -> bool:
    return INTEL_WORD.lower() in name.lower()


def mac_asset_name(version: str, machine: str | None = None) -> str:
    """The disk image's file name for ``version`` on a Mac with this processor.

    The one place the rule lives: ``build_mac.py`` names the file with it and :func:`pick_asset`
    chooses by it, so the two cannot drift apart.
    """
    if is_intel_machine(machine):
        return f"AutoPara-{INTEL_WORD}-{version}.dmg"
    return f"AutoPara-{version}.dmg"

MAX_METADATA_BYTES = 2_000_000
CHUNK = 64 * 1024


class UpdateError(Exception):
    """Anything that stops an update: no network, an untrusted URL, a file that does not verify."""


@dataclass(frozen=True)
class Asset:
    name: str
    url: str
    size: int = 0
    sha256: str | None = None


@dataclass(frozen=True)
class Release:
    version: str          # "1.6.0", without the "v"
    tag: str
    notes: str
    page_url: str         # the release's page on GitHub; "" if GitHub gave one we do not trust
    asset: Asset | None   # the installer for this platform, if the release has one


# ----------------------------------------------------------------------- versions


def parse_version(text: str) -> tuple[int, int, int] | None:
    match = re.fullmatch(r"\s*v?(\d+)\.(\d+)\.(\d+)\s*", text or "")
    return tuple(int(part) for part in match.groups()) if match else None  # type: ignore[return-value]


def is_newer(candidate: str, current: str = __version__) -> bool:
    new, old = parse_version(candidate), parse_version(current)
    return new is not None and old is not None and new > old


# ---------------------------------------------------------------------- trust


def is_trusted_url(url: str) -> bool:
    """HTTPS, on github.com or one of its content hosts. Anything else is not ours to fetch."""
    parts = urlparse(url or "")
    host = (parts.hostname or "").lower()
    return parts.scheme == "https" and any(
        host == suffix or host.endswith("." + suffix) for suffix in TRUSTED_HOST_SUFFIXES
    )


def installable_here(platform: str = sys.platform, frozen: bool | None = None) -> bool:
    """Whether this process can replace itself: a packaged build on a platform with an installer."""
    frozen = getattr(sys, "frozen", False) if frozen is None else frozen
    return bool(frozen) and platform in ("win32", "darwin")


# ---------------------------------------------------------------------- release


def pick_asset(
    assets: list[dict], platform: str = sys.platform, machine: str | None = None
) -> Asset | None:
    """The file to download on ``platform``, or None when the release has none for it.

    On a Mac ``machine`` is the processor (default: this one). An Intel Mac only takes the image
    named for Intel and an Apple Silicon Mac only the one that is not: the wrong image would
    download and install fine and then not start, which is worse than offering nothing.
    """
    pattern = WINDOWS_ASSET if platform == "win32" else MAC_ASSET if platform == "darwin" else None
    if pattern is None:
        return None
    want_intel = is_intel_machine(machine)
    for entry in assets:
        name = str(entry.get("name", ""))
        url = str(entry.get("browser_download_url", ""))
        if not pattern.match(name) or Path(name).name != name or not url.startswith(DOWNLOAD_PREFIX):
            continue
        if platform == "darwin" and is_intel_image(name) != want_intel:
            continue
        digest = str(entry.get("digest") or "")
        sha = digest.split(":", 1)[1].lower() if digest.lower().startswith("sha256:") else None
        return Asset(name=name, url=url, size=int(entry.get("size") or 0), sha256=sha)
    return None


def parse_release(
    payload: dict, platform: str = sys.platform, machine: str | None = None
) -> Release | None:
    """A GitHub "release" object as a ``Release``, or None for a draft, a pre-release or nonsense."""
    if not isinstance(payload, dict) or payload.get("draft") or payload.get("prerelease"):
        return None
    tag = str(payload.get("tag_name", ""))
    parsed = parse_version(tag)
    if parsed is None:
        return None
    page = str(payload.get("html_url", ""))
    return Release(
        version=".".join(str(part) for part in parsed),
        tag=tag,
        notes=str(payload.get("body") or "").strip(),
        page_url=page if page.startswith(PAGE_PREFIX) else "",
        asset=pick_asset(list(payload.get("assets") or []), platform, machine),
    )


class _TrustedRedirects(urllib.request.HTTPRedirectHandler):
    """Follow redirects only to hosts that are themselves trusted."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):  # noqa: N802 - urllib naming
        if not is_trusted_url(newurl):
            raise UpdateError(f"refused a redirect to {urlparse(newurl).hostname or newurl!r}")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def tls_context() -> ssl.SSLContext:
    """The system's trusted roots -- or, where a packaged build finds none, ``certifi``'s.

    A frozen macOS app has no Keychain-aware OpenSSL behind it, and an empty trust store makes every
    HTTPS request fail with a certificate error. Falling back only when the store is empty keeps
    the system's roots (which a company proxy may rely on) in charge everywhere they exist.
    """
    context = ssl.create_default_context()
    if context.cert_store_stats().get("x509_ca", 0) == 0:
        try:
            import certifi

            context = ssl.create_default_context(cafile=certifi.where())
        except ImportError:
            log.warning("no trusted certificates found and certifi is not installed")
    return context


def default_opener():
    return urllib.request.build_opener(
        urllib.request.HTTPSHandler(context=tls_context()), _TrustedRedirects()
    )


def _request(url: str, accept: str) -> urllib.request.Request:
    if not is_trusted_url(url):
        raise UpdateError(f"not a GitHub HTTPS address: {url!r}")
    return urllib.request.Request(
        url, headers={"Accept": accept, "User-Agent": f"AutoPara/{__version__}"}
    )


def fetch_latest(opener=None, timeout: float = 10.0, platform: str = sys.platform) -> Release | None:
    """The newest published release, or None if the repository has none yet."""
    opener = opener or default_opener()
    try:
        with opener.open(_request(API_URL, "application/vnd.github+json"), timeout=timeout) as reply:
            raw = reply.read(MAX_METADATA_BYTES + 1)
    except urllib.error.HTTPError as error:
        if error.code == 404:  # no release has been published
            return None
        raise UpdateError(f"GitHub answered {error.code}") from error
    except (urllib.error.URLError, OSError, TimeoutError) as error:
        raise UpdateError(f"could not reach GitHub: {error}") from error
    if len(raw) > MAX_METADATA_BYTES:
        raise UpdateError("the release description is implausibly large")
    try:
        return parse_release(json.loads(raw.decode("utf-8")), platform)
    except (ValueError, UnicodeDecodeError) as error:
        raise UpdateError("GitHub's answer was not understandable") from error


# --------------------------------------------------------------------- download


def updates_dir(db_path: str | Path | None = None) -> Path:
    """Where downloads wait, beside the database (and so cleared by a reinstall)."""
    from .storage import default_db_path

    directory = Path(db_path or default_db_path()).parent / "updates"
    directory.mkdir(parents=True, exist_ok=True)
    return directory


def sha256_of(path: Path) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(CHUNK), b""):
            digest.update(block)
    return digest.hexdigest()


def verifies(path: Path, asset: Asset) -> bool:
    """Whether the file on disk is the file the release describes."""
    try:
        if not path.is_file():
            return False
        if asset.size and path.stat().st_size != asset.size:
            return False
        return asset.sha256 is None or sha256_of(path) == asset.sha256
    except OSError:
        return False


def download(
    asset: Asset,
    directory: Path,
    progress: Callable[[int, int], None] | None = None,
    opener=None,
    timeout: float = 30.0,
    cancelled: Callable[[], bool] | None = None,
) -> Path:
    """Fetch ``asset`` into ``directory`` and return its path, verified.

    A file already there that verifies is reused. The download goes to ``<name>.part`` and is
    renamed only once it checks out, so a half-finished or corrupt file never has the real name.
    """
    if not asset.url.startswith(DOWNLOAD_PREFIX) or Path(asset.name).name != asset.name:
        raise UpdateError("this is not a file from this project's releases")
    directory.mkdir(parents=True, exist_ok=True)
    target = directory / asset.name
    if verifies(target, asset):
        return target

    part = directory / (asset.name + ".part")
    opener = opener or default_opener()
    digest = hashlib.sha256()
    received = 0
    try:
        with opener.open(_request(asset.url, "application/octet-stream"), timeout=timeout) as reply:
            total = int(reply.headers.get("Content-Length") or asset.size or 0)
            with open(part, "wb") as out:
                while True:
                    if cancelled is not None and cancelled():
                        raise UpdateError("cancelled")
                    block = reply.read(CHUNK)
                    if not block:
                        break
                    out.write(block)
                    digest.update(block)
                    received += len(block)
                    if progress is not None:
                        progress(received, total)
        if asset.size and received != asset.size:
            raise UpdateError(f"expected {asset.size} bytes, received {received}")
        if asset.sha256 and digest.hexdigest() != asset.sha256:
            raise UpdateError("the downloaded file does not match its published checksum")
        os.replace(part, target)
        return target
    except UpdateError:
        part.unlink(missing_ok=True)
        raise
    except (urllib.error.URLError, OSError, TimeoutError) as error:
        part.unlink(missing_ok=True)
        raise UpdateError(f"the download failed: {error}") from error


def tidy(directory: Path, current: str = __version__) -> None:
    """Delete leftovers: partial files, and installers for a version we already are (or are past)."""
    for entry in directory.glob("*"):
        try:
            if entry.suffix == ".part":
                entry.unlink()
                continue
            match = re.search(r"(\d+\.\d+\.\d+)", entry.name)
            if match and not is_newer(match.group(1), current):
                entry.unlink()
        except OSError:
            log.warning("could not remove %s", entry)


# ------------------------------------------------------------------------- apply


def windows_command(installer: Path | str, program: Path | str) -> str:
    """The command line that waits for us to exit, installs silently, then starts the new copy.

    ``cmd /S /C "..."`` always strips exactly the outer quotes, which is what lets the paths inside
    carry quotes of their own. The ping is a pause that needs no console (``timeout`` refuses to
    run without one): the installer replaces files this process still holds open, and silent mode
    skips the installer's own "is it running?" check.
    """
    return (
        f'cmd /S /C "ping -n 3 127.0.0.1 >nul & "{installer}" /S & start "" "{program}""'
    )


def apply(installer: Path, directory: Path, platform: str = sys.platform, program: str | None = None) -> bool:
    """Hand the downloaded file to the system. The caller quits the app when this returns True.

    Only a file inside ``directory`` is ever launched.
    """
    try:
        installer = installer.resolve(strict=True)
        directory = directory.resolve(strict=True)
    except OSError:
        return False
    if directory not in installer.parents:
        log.error("refusing to run %s: it is not in %s", installer, directory)
        return False
    try:
        if platform == "win32":
            flags = (
                getattr(subprocess, "CREATE_NO_WINDOW", 0)
                | getattr(subprocess, "DETACHED_PROCESS", 0)
                | getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0)
            )
            subprocess.Popen(
                windows_command(installer, program or sys.executable),
                creationflags=flags,
                close_fds=True,
            )
            return True
        if platform == "darwin":
            # A disk image is opened for the user to drag the app across; it is not installed for
            # them, and the running copy stays up until they do.
            subprocess.run(["open", str(installer)], check=True)
            return False
    except (OSError, subprocess.SubprocessError):
        log.exception("could not start the update")
    return False
