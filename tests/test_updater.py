"""The update machinery without a network: versions, trust, parsing, download and apply.

Every network call goes through an opener the test supplies; ``conftest._no_network`` makes the real
one raise, so nothing here can reach GitHub by accident.
"""

from __future__ import annotations

import hashlib
import io
import json
import urllib.error

import pytest

from autopara.core import updater
from autopara.core.updater import Asset, UpdateError

REPO = updater.GITHUB_REPO
SETUP = f"https://github.com/{REPO}/releases/download/v1.6.0/AutoPara-1.6.0-Setup.exe"
DMG = f"https://github.com/{REPO}/releases/download/v1.6.0/AutoPara-1.6.0.dmg"


def payload(**overrides) -> dict:
    base = {
        "tag_name": "v1.6.0",
        "html_url": f"https://github.com/{REPO}/releases/tag/v1.6.0",
        "body": "Що нового\n\n- день, тиждень, місяць",
        "draft": False,
        "prerelease": False,
        "assets": [
            {"name": "AutoPara-1.6.0-Setup.exe", "browser_download_url": SETUP, "size": 12,
             "digest": "sha256:" + hashlib.sha256(b"installer-bytes").hexdigest()},
            {"name": "AutoPara-1.6.0.dmg", "browser_download_url": DMG, "size": 7},
        ],
    }
    base.update(overrides)
    return base


class FakeReply(io.BytesIO):
    def __init__(self, data: bytes, headers=None):
        super().__init__(data)
        self.headers = headers or {}

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        return False


class FakeOpener:
    """Stands in for ``urllib``'s opener: answers with canned bytes, or raises what it is told."""

    def __init__(self, data: bytes = b"", raises: Exception | None = None, headers=None):
        self.data, self.raises, self.headers = data, raises, headers
        self.requests: list = []

    def open(self, request, timeout=None):
        self.requests.append(request)
        if self.raises is not None:
            raise self.raises
        return FakeReply(self.data, self.headers)


class TestVersions:
    @pytest.mark.parametrize("text,expected", [
        ("1.6.0", (1, 6, 0)), ("v1.6.0", (1, 6, 0)), (" v10.0.12 ", (10, 0, 12)),
        ("1.6", None), ("1.6.0-beta", None), ("latest", None), ("", None), (None, None),
    ])
    def test_parsing(self, text, expected):
        assert updater.parse_version(text) == expected

    def test_newer_means_strictly_newer(self):
        assert updater.is_newer("1.6.0", "1.5.0")
        assert updater.is_newer("1.5.1", "1.5.0")
        assert updater.is_newer("2.0.0", "1.99.99")
        assert not updater.is_newer("1.5.0", "1.5.0")
        assert not updater.is_newer("1.4.9", "1.5.0")

    def test_ten_is_after_nine(self):
        """The reason versions are compared as numbers and not as text."""
        assert updater.is_newer("1.10.0", "1.9.0")

    def test_nonsense_is_never_an_update(self):
        assert not updater.is_newer("banana", "1.5.0")
        assert not updater.is_newer("1.6.0", "banana")


class TestTrust:
    @pytest.mark.parametrize("url", [
        "https://github.com/x/y", "https://api.github.com/repos/x/y",
        "https://objects.githubusercontent.com/abc", "https://release-assets.githubusercontent.com/x",
    ])
    def test_github_over_https_is_trusted(self, url):
        assert updater.is_trusted_url(url)

    @pytest.mark.parametrize("url", [
        "http://github.com/x", "https://evilgithub.com/x", "https://github.com.evil.org/x",
        "https://example.com/github.com", "file:///etc/passwd", "javascript:alert(1)", "", None,
        "https://githubusercontent.com.evil.org/",
    ])
    def test_everything_else_is_not(self, url):
        assert not updater.is_trusted_url(url)

    def test_only_a_packaged_build_on_a_known_platform_can_install_itself(self):
        assert updater.installable_here("win32", frozen=True)
        assert updater.installable_here("darwin", frozen=True)
        assert not updater.installable_here("win32", frozen=False)  # running from source
        assert not updater.installable_here("linux", frozen=True)


class TestParsingAReleaseObject:
    def test_a_normal_release(self):
        release = updater.parse_release(payload(), "win32")
        assert (release.version, release.tag) == ("1.6.0", "v1.6.0")
        assert release.page_url.startswith(f"https://github.com/{REPO}/")
        assert release.asset.name == "AutoPara-1.6.0-Setup.exe"
        assert release.asset.sha256 == hashlib.sha256(b"installer-bytes").hexdigest()

    def test_each_platform_gets_its_own_file(self):
        assert updater.parse_release(payload(), "darwin").asset.name == "AutoPara-1.6.0.dmg"
        assert updater.parse_release(payload(), "linux").asset is None

    def test_a_draft_or_a_prerelease_is_not_offered(self):
        assert updater.parse_release(payload(draft=True), "win32") is None
        assert updater.parse_release(payload(prerelease=True), "win32") is None

    def test_a_tag_that_is_not_a_version_is_ignored(self):
        assert updater.parse_release(payload(tag_name="nightly"), "win32") is None

    def test_a_release_with_no_file_for_us_is_still_news(self):
        release = updater.parse_release(payload(assets=[]), "win32")
        assert release is not None and release.asset is None

    def test_a_page_outside_the_repository_is_dropped(self):
        release = updater.parse_release(payload(html_url="https://evil.example/x"), "win32")
        assert release.page_url == ""

    def test_a_file_that_is_not_from_this_repositorys_releases_is_ignored(self):
        evil = [{"name": "AutoPara-9.9.9-Setup.exe",
                 "browser_download_url": "https://evil.example/AutoPara-9.9.9-Setup.exe", "size": 1}]
        assert updater.parse_release(payload(assets=evil), "win32").asset is None
        other = [{"name": "AutoPara-9.9.9-Setup.exe", "size": 1,
                  "browser_download_url": "https://github.com/someone/else/releases/download/v1/AutoPara-9.9.9-Setup.exe"}]
        assert updater.parse_release(payload(assets=other), "win32").asset is None

    def test_a_file_name_with_a_path_in_it_is_ignored(self):
        tricky = [{"name": "../AutoPara-1.6.0-Setup.exe", "size": 1, "browser_download_url": SETUP}]
        assert updater.parse_release(payload(assets=tricky), "win32").asset is None

    def test_garbage_is_not_a_release(self):
        assert updater.parse_release([], "win32") is None
        assert updater.parse_release({"tag_name": 7}, "win32") is None


class TestTls:
    def test_the_systems_roots_are_used_when_there_are_any(self, monkeypatch):
        import ssl

        sentinel = ssl.create_default_context()
        monkeypatch.setattr(sentinel, "cert_store_stats", lambda: {"x509_ca": 5})
        monkeypatch.setattr(updater.ssl, "create_default_context", lambda **k: sentinel)
        assert updater.tls_context() is sentinel

    def test_certifi_is_the_fallback_when_the_store_is_empty(self, monkeypatch):
        import ssl
        import sys
        import types

        empty = ssl.create_default_context()
        monkeypatch.setattr(empty, "cert_store_stats", lambda: {"x509_ca": 0})
        asked = []

        def make(**kwargs):
            asked.append(kwargs)
            return empty

        monkeypatch.setattr(updater.ssl, "create_default_context", make)
        monkeypatch.setitem(sys.modules, "certifi", types.SimpleNamespace(where=lambda: "/c/cacert.pem"))
        updater.tls_context()
        assert asked == [{}, {"cafile": "/c/cacert.pem"}]

    def test_no_certifi_and_no_roots_still_returns_a_context(self, monkeypatch):
        import ssl
        import sys

        empty = ssl.create_default_context()
        monkeypatch.setattr(empty, "cert_store_stats", lambda: {"x509_ca": 0})
        monkeypatch.setattr(updater.ssl, "create_default_context", lambda **k: empty)
        monkeypatch.setitem(sys.modules, "certifi", None)  # makes ``import certifi`` fail
        assert updater.tls_context() is empty


class TestFetchLatest:
    def test_it_returns_the_parsed_release(self):
        opener = FakeOpener(json.dumps(payload()).encode())
        release = updater.fetch_latest(opener, platform="win32")
        assert release.version == "1.6.0"
        request = opener.requests[0]
        assert request.full_url == updater.API_URL
        assert request.get_header("User-agent").startswith("AutoPara/")

    def test_a_repository_with_no_release_is_not_an_error(self):
        error = urllib.error.HTTPError(updater.API_URL, 404, "Not Found", {}, None)
        assert updater.fetch_latest(FakeOpener(raises=error)) is None

    @pytest.mark.parametrize("failure", [
        urllib.error.HTTPError(updater.API_URL, 500, "boom", {}, None),
        urllib.error.URLError("no route"), TimeoutError("slow"), OSError("down"),
    ])
    def test_a_failure_is_an_update_error(self, failure):
        with pytest.raises(UpdateError):
            updater.fetch_latest(FakeOpener(raises=failure))

    def test_an_answer_that_is_not_json_is_an_update_error(self):
        with pytest.raises(UpdateError):
            updater.fetch_latest(FakeOpener(b"<html>"))

    def test_an_enormous_answer_is_refused(self):
        with pytest.raises(UpdateError):
            updater.fetch_latest(FakeOpener(b"x" * (updater.MAX_METADATA_BYTES + 5)))

    def test_a_redirect_off_github_is_refused(self):
        handler = updater._TrustedRedirects()
        with pytest.raises(UpdateError):
            handler.redirect_request(None, None, 302, "Found", {}, "https://evil.example/payload.exe")

    def test_a_redirect_to_githubs_cdn_is_followed(self):
        import urllib.request

        handler = updater._TrustedRedirects()
        request = urllib.request.Request(SETUP)
        followed = handler.redirect_request(
            request, None, 302, "Found", {}, "https://objects.githubusercontent.com/abc"
        )
        assert followed.full_url == "https://objects.githubusercontent.com/abc"


class TestDownload:
    DATA = b"installer-bytes"

    def asset(self, **overrides) -> Asset:
        base = dict(name="AutoPara-1.6.0-Setup.exe", url=SETUP, size=len(self.DATA),
                    sha256=hashlib.sha256(self.DATA).hexdigest())
        base.update(overrides)
        return Asset(**base)

    def test_it_downloads_and_verifies(self, tmp_path):
        seen: list[tuple[int, int]] = []
        path = updater.download(
            self.asset(),
            tmp_path,
            lambda done, total: seen.append((done, total)),
            opener=FakeOpener(self.DATA, headers={"Content-Length": str(len(self.DATA))}),
        )
        assert path == tmp_path / "AutoPara-1.6.0-Setup.exe"
        assert path.read_bytes() == self.DATA
        assert seen[-1] == (len(self.DATA), len(self.DATA))
        assert not list(tmp_path.glob("*.part")), "no partial file is left behind"

    def test_a_wrong_checksum_is_deleted_not_kept(self, tmp_path):
        with pytest.raises(UpdateError, match="checksum"):
            updater.download(self.asset(sha256="0" * 64), tmp_path, opener=FakeOpener(self.DATA))
        assert list(tmp_path.iterdir()) == []

    def test_a_wrong_size_is_deleted_not_kept(self, tmp_path):
        with pytest.raises(UpdateError):
            updater.download(self.asset(size=999), tmp_path, opener=FakeOpener(self.DATA))
        assert list(tmp_path.iterdir()) == []

    def test_without_a_published_checksum_the_size_still_has_to_match(self, tmp_path):
        path = updater.download(self.asset(sha256=None), tmp_path, opener=FakeOpener(self.DATA))
        assert path.read_bytes() == self.DATA

    def test_a_file_that_is_already_there_and_verifies_is_not_fetched_again(self, tmp_path):
        (tmp_path / "AutoPara-1.6.0-Setup.exe").write_bytes(self.DATA)
        opener = FakeOpener(raises=AssertionError("the network must not be touched"))
        assert updater.download(self.asset(), tmp_path, opener=opener).read_bytes() == self.DATA

    def test_a_corrupt_leftover_is_replaced(self, tmp_path):
        (tmp_path / "AutoPara-1.6.0-Setup.exe").write_bytes(b"half")
        path = updater.download(self.asset(), tmp_path, opener=FakeOpener(self.DATA))
        assert path.read_bytes() == self.DATA

    def test_a_network_failure_leaves_nothing_and_says_so(self, tmp_path):
        with pytest.raises(UpdateError):
            updater.download(self.asset(), tmp_path, opener=FakeOpener(raises=urllib.error.URLError("x")))
        assert list(tmp_path.iterdir()) == []

    def test_it_can_be_cancelled(self, tmp_path):
        with pytest.raises(UpdateError, match="cancelled"):
            updater.download(self.asset(), tmp_path, opener=FakeOpener(self.DATA), cancelled=lambda: True)
        assert list(tmp_path.iterdir()) == []

    @pytest.mark.parametrize("url,name", [
        ("https://evil.example/AutoPara-1.6.0-Setup.exe", "AutoPara-1.6.0-Setup.exe"),
        (SETUP, "../escape.exe"),
        ("http://github.com/x", "AutoPara-1.6.0-Setup.exe"),
    ])
    def test_only_this_projects_release_files_are_fetched(self, tmp_path, url, name):
        opener = FakeOpener(self.DATA)
        with pytest.raises(UpdateError):
            updater.download(self.asset(url=url, name=name), tmp_path, opener=opener)
        assert opener.requests == []

    def test_verifies_helper(self, tmp_path):
        target = tmp_path / "f"
        target.write_bytes(self.DATA)
        assert updater.verifies(target, self.asset())
        assert not updater.verifies(target, self.asset(sha256="1" * 64))
        assert not updater.verifies(tmp_path / "missing", self.asset())


class TestTidy:
    def test_it_removes_partials_and_installers_we_already_are(self, tmp_path):
        for name in ("AutoPara-1.4.0-Setup.exe", "AutoPara-1.5.0-Setup.exe", "AutoPara-1.6.0-Setup.exe",
                     "AutoPara-1.7.0-Setup.exe.part"):
            (tmp_path / name).write_bytes(b"x")
        updater.tidy(tmp_path, current="1.5.0")
        assert sorted(p.name for p in tmp_path.iterdir()) == ["AutoPara-1.6.0-Setup.exe"]


class TestApply:
    def make(self, tmp_path):
        folder = tmp_path / "updates"
        folder.mkdir()
        installer = folder / "AutoPara-1.6.0-Setup.exe"
        installer.write_bytes(b"x")
        return folder, installer

    def test_the_windows_command_waits_installs_quietly_then_starts_the_new_copy(self):
        command = updater.windows_command(r"C:\u\AutoPara-1.6.0-Setup.exe", r"C:\p\AutoPara.exe")
        assert command.startswith('cmd /S /C "') and command.endswith('"')
        inner = command[len('cmd /S /C "'):-1]
        waited, installed, started = (part.strip() for part in inner.split(" & "))
        assert "ping" in waited
        assert installed == r'"C:\u\AutoPara-1.6.0-Setup.exe" /S'
        assert started == r'start "" "C:\p\AutoPara.exe"'

    def test_on_windows_it_launches_the_installer_and_says_to_quit(self, tmp_path, monkeypatch):
        folder, installer = self.make(tmp_path)
        launched = []
        monkeypatch.setattr(updater.subprocess, "Popen", lambda *a, **k: launched.append((a, k)))
        assert updater.apply(installer, folder, "win32", program="AutoPara.exe") is True
        command = launched[0][0][0]
        assert str(installer.resolve()) in command and "/S" in command

    def test_on_a_mac_it_opens_the_image_and_does_not_quit(self, tmp_path, monkeypatch):
        folder, image = self.make(tmp_path)
        ran = []
        monkeypatch.setattr(updater.subprocess, "run", lambda *a, **k: ran.append(a))
        assert updater.apply(image, folder, "darwin") is False
        assert ran[0][0][0] == "open"

    def test_a_file_outside_the_updates_folder_is_never_run(self, tmp_path, monkeypatch):
        folder, _ = self.make(tmp_path)
        stranger = tmp_path / "evil.exe"
        stranger.write_bytes(b"x")
        monkeypatch.setattr(updater.subprocess, "Popen", lambda *a, **k: pytest.fail("ran it"))
        assert updater.apply(stranger, folder, "win32") is False

    def test_a_path_that_climbs_out_is_never_run(self, tmp_path, monkeypatch):
        folder, _ = self.make(tmp_path)
        (tmp_path / "evil.exe").write_bytes(b"x")
        monkeypatch.setattr(updater.subprocess, "Popen", lambda *a, **k: pytest.fail("ran it"))
        assert updater.apply(folder / ".." / "evil.exe", folder, "win32") is False

    def test_a_missing_file_is_just_false(self, tmp_path):
        folder, installer = self.make(tmp_path)
        installer.unlink()
        assert updater.apply(installer, folder, "win32") is False

    def test_a_launch_that_fails_is_false_not_a_crash(self, tmp_path, monkeypatch):
        folder, installer = self.make(tmp_path)

        def boom(*a, **k):
            raise OSError("blocked")

        monkeypatch.setattr(updater.subprocess, "Popen", boom)
        assert updater.apply(installer, folder, "win32") is False
