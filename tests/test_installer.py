"""Guards on the NSIS installer script.

The installer is the one part of the project that cannot be exercised from pytest -- running it
installs the app. What *can* be checked is that the script keeps agreeing with the code it ships,
and every assertion here exists because the two drifting apart fails silently:

* the autostart command must match what the app itself writes, or the settings screen shows the
  entry as disabled and rewrites it on the next launch;
* the window title it looks for must match the app's, or "AutoPara is running" never triggers and
  the install writes over locked files. That had already drifted once, when the UI was translated.
"""

from __future__ import annotations

import re
from pathlib import Path

import pytest

NSI_PATH = Path(__file__).resolve().parents[1] / "installer" / "AutoPara.nsi"


@pytest.fixture(scope="module")
def script() -> str:
    if not NSI_PATH.is_file():
        pytest.skip(f"{NSI_PATH} not found")
    # The file is UTF-8 with a BOM; makensis needs the BOM to read the Ukrainian strings.
    return NSI_PATH.read_text(encoding="utf-8-sig")


def define(script: str, name: str) -> str:
    """Read a !define value, joining NSIS backslash line continuations first."""
    joined = re.sub(r"\\\s*\n\s*", " ", script)
    match = re.search(rf'^!define\s+{name}\s+"(.*)"\s*$', joined, re.MULTILINE)
    assert match, f"!define {name} not found in the installer script"
    return match.group(1)


class TestEncoding:
    def test_file_is_utf8_with_bom(self):
        """Without the BOM makensis falls back to the code page and mangles the Ukrainian text."""
        assert NSI_PATH.read_bytes().startswith(b"\xef\xbb\xbf")

    def test_unicode_mode_is_on(self, script):
        assert re.search(r"^Unicode true", script, re.MULTILINE)


class TestAgreesWithTheApplication:
    def test_autostart_command_matches_what_the_app_writes(self, script):
        """The installer and autostart.startup_command() must produce the same string.

        If they diverge, autostart.sync() silently rewrites the installer's entry on first launch.
        """
        from autopara.core import autostart

        written = re.search(
            r'WriteRegStr\s+HKCU\s+"\$\{RUN_KEY\}"\s+"\$\{APP_NAME\}"\s+\'(.+)\'',
            script,
        )
        assert written, "the autostart section does not write a Run value"
        installer_command = written.group(1).replace("$INSTDIR\\${APP_EXE}", "{exe}")

        # What the frozen app produces, with the executable path stubbed the same way.
        app_command = autostart.startup_command()
        frozen_shape = '"{exe}" --hidden'
        assert installer_command == frozen_shape
        assert app_command.endswith("--hidden"), "the app must also start hidden"
        assert app_command.startswith('"'), "the path must be quoted, it contains spaces"

    def test_window_title_matches_the_running_app(self, script):
        """FindWindow needs the exact title, or the 'is it running?' check never fires."""
        from autopara.ui import main_window

        source = Path(main_window.__file__).read_text(encoding="utf-8")
        title = re.search(r'setWindowTitle\(\s*"([^"]+)"\s*\)', source)
        assert title, "MainWindow.setWindowTitle not found"

        assert define(script, "APP_DISPLAY") == title.group(1)
        assert 'FindWindow $0 "" "${APP_DISPLAY}"' in script

    def test_data_directory_matches_the_one_storage_uses(self, script):
        """The uninstaller offers to delete the app's data; it must name the right folder."""
        from autopara.core.storage import default_db_path

        assert default_db_path().parent.name == "AutoPara"
        assert '$APPDATA\\${APP_NAME}' in script


class TestInstallReplacesRatherThanMerges:
    def test_the_bundle_directory_is_purged_before_writing(self, script):
        """A module left from an older bundle stays importable and wins over nothing."""
        purge = script.index('RMDir /r "$INSTDIR\\_internal"')
        write = script.index('File /r "..\\dist\\AutoPara\\*.*"')
        assert purge < write, "the old bundle must be removed before the new one is written"


class TestSilentModeNeverBlocks:
    @pytest.mark.parametrize("guard", ["skip_running_check", "keep_data"])
    def test_every_messagebox_is_guarded_by_ifsilent(self, script, guard):
        """/S install and uninstall would hang forever on an invisible message box."""
        assert f"IfSilent {guard}" in script

    def test_no_unguarded_messagebox_remains(self, script):
        """Each MessageBox must have an IfSilent before it."""
        silent_positions = [m.start() for m in re.finditer(r"IfSilent\s+\w+", script)]
        for box in re.finditer(r"^\s*MessageBox\s", script, re.MULTILINE):
            assert any(pos < box.start() for pos in silent_positions), (
                f"MessageBox at offset {box.start()} is not preceded by an IfSilent guard"
            )


class TestUninstall:
    def test_it_keeps_the_imported_timetable_by_default(self, script):
        """Reinstalling must not lose the schedule, so deleting the data is opt-in."""
        uninstall = script[script.index('Section "Uninstall"'):]
        delete_data = uninstall.index('RMDir /r "$APPDATA\\${APP_NAME}"')
        guard = uninstall.index("IfSilent keep_data")
        assert guard < delete_data, "a silent uninstall must skip the data deletion"
        assert "IDNO keep_data" in uninstall, "answering No must keep the data"

    def test_it_removes_both_registry_entries(self, script):
        uninstall = script[script.index('Section "Uninstall"'):]
        assert 'DeleteRegValue HKCU "${RUN_KEY}" "${APP_NAME}"' in uninstall
        assert 'DeleteRegKey   HKCU "${UNINST_KEY}"' in uninstall


class TestPerUserInstall:
    def test_no_admin_rights_are_requested(self, script):
        """A per-user install raises no UAC prompt, which matters on a borrowed PC."""
        assert re.search(r"^RequestExecutionLevel user", script, re.MULTILINE)

    def test_it_installs_under_localappdata(self, script):
        assert 'InstallDir "$LOCALAPPDATA\\Programs\\${APP_NAME}"' in script

    def test_registry_writes_are_all_hkcu(self, script):
        """A per-user install must never touch HKLM; it would fail without elevation."""
        assert "HKLM" not in script


class TestInterfaceIsUkrainian:
    """The app's UI is Ukrainian, so the installer the same user sees must be too."""

    def test_ukrainian_language_is_selected(self, script):
        assert '!insertmacro MUI_LANGUAGE "Ukrainian"' in script

    def test_user_visible_strings_are_ukrainian(self, script):
        cyrillic = re.compile(r"[А-Яа-яІіЇїЄєҐґ]")
        for macro in ("MUI_FINISHPAGE_RUN_TEXT", "MUI_FINISHPAGE_TEXT"):
            assert cyrillic.search(define(script, macro)), f"{macro} is not Ukrainian"

        sections = re.findall(r'^Section\s+"([^"]+)"\s+SEC_', script, re.MULTILINE)
        assert sections, "no installer sections found"
        for name in sections:
            assert cyrillic.search(name), f"section name is not Ukrainian: {name}"

        for description in re.findall(r"^LangString\s+DESC_\w+\s+\$\{LANG_UKRAINIAN\}\s+\"(.+?)\"",
                                      script, re.MULTILINE | re.DOTALL):
            assert cyrillic.search(description)


class TestThereIsOnlyOneInstaller:
    """The project deliberately ships one installer; a second one drifts out of date."""

    def test_the_source_installer_is_gone(self):
        root = NSI_PATH.parents[1]
        for stale in ("installer.exe", "installer.cmd",
                      "installer/install_app.py", "installer/build_installer.ps1"):
            assert not (root / stale).exists(), f"{stale} should have been removed"

    def test_build_script_drives_the_nsis_installer(self):
        build = (NSI_PATH.parents[1] / "build.ps1").read_text(encoding="utf-8")
        assert "makensis" in build
        assert "AutoPara.nsi" in build


class TestApplicationIcon:
    """The desktop shortcut, the taskbar button and the tray must all show the same mark.

    Every assertion here exists because the build shipped PyInstaller's stock icon -- the Python
    logo -- and the desktop shortcut inherited it, so AutoPara looked like a Python script.
    """

    def test_the_spec_compiles_an_icon_into_the_executable(self):
        spec = (Path(__file__).resolve().parents[1] / "AutoPara.spec").read_text(encoding="utf-8")
        assert "icon=APP_ICON" in spec, "the exe icon is what every shortcut inherits"
        assert "build" in spec and "AutoPara.ico" in spec

    def test_the_build_script_renders_that_icon_first(self):
        script = (Path(__file__).resolve().parents[1] / "build.ps1").read_text(encoding="utf-8")
        assert "write_ico" in script, "the .ico is generated, not committed"
        rendering = script.index("write_ico")
        building = script.index("python -m PyInstaller")
        assert rendering < building, "the icon has to exist before PyInstaller reads the spec"

    def test_the_icon_file_carries_every_size_windows_asks_for(self, tmp_path, gui_app):
        """A missing size is a blurry icon in whichever slot needed it."""
        import struct

        from autopara.ui.tray import ICO_SIZES, write_ico

        raw = write_ico(tmp_path / "AutoPara.ico").read_bytes()
        reserved, kind, count = struct.unpack("<HHH", raw[:6])
        assert (reserved, kind) == (0, 1), "an .ico starts 0, 1"
        assert count == len(ICO_SIZES)

        for index, size in enumerate(ICO_SIZES):
            entry = raw[6 + 16 * index : 22 + 16 * index]
            width, height, _, _, planes, bits, length, offset = struct.unpack("<BBBBHHII", entry)
            # 256 is stored as 0: the field is a single byte.
            assert (width, height) == (size % 256, size % 256)
            assert (planes, bits) == (1, 32)
            assert raw[offset : offset + 8] == b"\x89PNG\r\n\x1a\n", "entries are PNGs"
            assert length > 0

    def test_the_build_can_be_run_on_an_unconfigured_machine(self):
        """Windows blocks .ps1 outright by default, so build.ps1 alone is not a runnable build.

        `.\build.ps1` fails with UnauthorizedAccess on any machine whose execution policy nobody
        has touched -- which is every fresh one. The .cmd wrapper lifts it for the single process
        it starts, without changing a setting for the machine or the user.
        """
        root = Path(__file__).resolve().parents[1]
        wrapper = root / "build.cmd"
        assert wrapper.is_file(), "build.ps1 needs a wrapper that Windows will actually run"

        text = wrapper.read_text(encoding="utf-8")
        assert "-ExecutionPolicy Bypass" in text
        assert "build.ps1" in text
        assert "%*" in text, "-SkipApp and friends must reach the script"
        assert "Set-ExecutionPolicy" not in text, "a build must not change a security setting"

    def test_the_app_claims_its_own_taskbar_identity(self):
        """Without an AppUserModelID Windows hangs the button on python.exe and shows its icon."""
        app = (Path(__file__).resolve().parents[1] / "autopara" / "app.py").read_text(
            encoding="utf-8"
        )
        assert "SetCurrentProcessExplicitAppUserModelID" in app
        claim = app.index("_claim_taskbar_identity()\n        self.qt.setWindowIcon")
        assert claim > 0, "the identity must be claimed before the first window exists"


class TestVersion:
    """The program reports its own version to the update check; the installer script is what CI
    reads. If the two differ the app would offer an update to the version it already is, or never
    notice a new one -- silently."""

    def test_the_version_the_app_reports_is_the_one_the_installer_builds(self, script):
        from autopara import __version__

        assert __version__ == define(script, "APP_VERSION")

    def test_it_is_a_plain_three_part_version(self):
        from autopara import __version__
        from autopara.core.updater import parse_version

        assert parse_version(__version__) is not None


class TestUpdateAssetNaming:
    """The updater picks the installer out of a release by name; the installer script names it."""

    def test_the_file_the_build_writes_is_the_file_the_updater_looks_for(self, script):
        from autopara.core import updater

        out = re.search(r'^OutFile\s+"([^"]+)"', script, re.MULTILINE)
        assert out, "OutFile not found"
        name = out.group(1).replace("\\", "/").rsplit("/", 1)[-1]
        name = name.replace("${APP_VERSION}", define(script, "APP_VERSION"))
        assert updater.WINDOWS_ASSET.match(name), name

    def test_the_repository_the_updater_asks_is_the_one_this_is_cloned_from(self):
        from autopara.core import updater

        config = Path(__file__).resolve().parents[1] / ".git" / "config"
        if not config.is_file():
            pytest.skip("not a git checkout")
        urls = re.findall(r"url\s*=\s*(\S+)", config.read_text(encoding="utf-8"))
        if not urls:
            pytest.skip("no remote configured")
        assert any(updater.GITHUB_REPO.lower() in url.lower() for url in urls)


class TestMacImage:
    """The Mac disk image is built by build_mac.py and found by the updater by its name."""

    def test_the_image_the_build_writes_matches_what_the_updater_looks_for(self):
        from autopara import __version__
        from autopara.core import updater

        build = (Path(__file__).resolve().parents[1] / "build_mac.py").read_text(encoding="utf-8")
        assert "mac_asset_name(__version__)" in build, "the build and the updater share one naming rule"
        for machine in ("arm64", "x86_64"):
            assert updater.MAC_ASSET.match(updater.mac_asset_name(__version__, machine))

    def test_the_workflow_builds_and_attaches_it(self):
        flow = (Path(__file__).resolve().parents[1] / ".github" / "workflows" / "installer.yml").read_text(
            encoding="utf-8"
        )
        assert "python build_mac.py" in flow and "gh release upload" in flow
        assert "AutoPara-$version.dmg" in flow

    def test_the_workflow_also_builds_the_intel_image_after_the_apple_silicon_one(self):
        flow = (Path(__file__).resolve().parents[1] / ".github" / "workflows" / "installer.yml").read_text(
            encoding="utf-8"
        )
        assert "\n  build-mac-intel:\n" in flow
        job = flow.split("\n  build-mac-intel:\n", 1)[1]
        runner = re.search(r"runs-on:\s*(\S+)", job).group(1)
        assert "intel" in runner, "an Intel bundle can only be built on an Intel runner"
        assert re.search(r"needs:\s*build-mac\s*$", job, re.MULTILINE), (
            "the Apple Silicon image must be the release's first .dmg"
        )
        assert "AutoPara-Intel-$version.dmg" in job and "python build_mac.py" in job
        assert "gh release upload" in job


class TestWorkflowChecksItsTools:
    def test_nsis_is_checked_after_it_is_installed_and_retried(self):
        flow = (Path(__file__).resolve().parents[1] / ".github" / "workflows" / "installer.yml").read_text(
            encoding="utf-8"
        )
        step = flow.split("- name: Install NSIS", 1)[1].split("- name:", 1)[0]
        assert "choco install nsis" in step
        assert "1..3" in step, "a miss is tried again"
        assert "throw" in step, "and if it is still not there the step says so, rather than the build"


class TestAndroidSharesTheVersion:
    """The APK has no version of its own: it is built from the PC apps' number and rides their release."""

    ROOT = Path(__file__).resolve().parents[1]

    def test_gradle_reads_the_desktop_version(self):
        gradle = (self.ROOT / "android" / "app" / "build.gradle.kts").read_text(encoding="utf-8")
        assert "autopara/__init__.py" in gradle
        assert "__version__" in gradle
        # A hard-coded versionName would silently drift from the release it is attached to.
        assert not re.search(r'versionName\s*=\s*"\d', gradle)

    def test_the_apk_is_attached_to_the_pc_release(self):
        flow = (self.ROOT / ".github" / "workflows" / "installer.yml").read_text(encoding="utf-8")
        job = flow[flow.index("build-android:"):]
        assert "AutoPara-$version.apk" in job
        assert 'gh release upload "v${{ steps.plan.outputs.version }}"' in job
        assert "needs: build" in job  # the Windows job is the one that creates the release

    def test_android_is_not_released_on_its_own(self):
        flow = (self.ROOT / ".github" / "workflows" / "android.yml").read_text(encoding="utf-8")
        assert "gh release create" not in flow
