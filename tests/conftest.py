"""The schedule documents the suite runs against.

Two of them, for two different jobs:

* ``schedule_path`` / ``courses`` / ``lessons`` are a **synthetic** timetable built from code
  (``tests/sample_timetable.py``). It has the structure of the real one and none of its content, and
  it never changes, so every test that describes how the importer resolves merges -- and every test
  that needs *some* timetable in a database -- always runs, here and in CI, with exact expectations.
* The **real** timetable is the user's personal document and is deliberately not in the repository
  (it contains live meeting links, and the university rewrites it). Point ``AUTOPARA_TEST_DOCX`` at
  it, or leave it on the Desktop where the importer found it originally. Tests that use it
  (``real_schedule_path``) assert only what must hold for *any* revision, reading the expected
  numbers from the document itself, so a new revision cannot fail them.
"""

from __future__ import annotations

import glob
import os
from pathlib import Path

import pytest

_ENV_VAR = "AUTOPARA_TEST_DOCX"
# Searched in order. The timetable tends to migrate between the Desktop, the Downloads folder and
# wherever the messaging app dropped it, and a suite that silently skips 100+ tests because it could
# not find the file looks exactly like a passing suite -- hence the wide net and the report header.
_FALLBACK_GLOBS = [
    str(Path.home() / "Desktop" / "*розклад*.docx"),
    str(Path.home() / "Downloads" / "*розклад*.docx"),
    str(Path.home() / "Downloads" / "*" / "*розклад*.docx"),
    str(Path(os.environ.get("APPDATA", Path.home())) / "AutoPara" / "schedules" / "*.docx"),
    str(Path.home() / "Desktop" / "*.docx"),
]


def _locate() -> str | None:
    configured = os.environ.get(_ENV_VAR)
    if configured and Path(configured).is_file():
        return configured
    for pattern in _FALLBACK_GLOBS:
        matches = sorted(glob.glob(pattern))
        if matches:
            return matches[0]
    return None


def pytest_report_header(config) -> str:
    """Say whether the real timetable was found.

    Without this a missing document is invisible: the real-document checks skip and the summary
    still reads green.
    """
    path = _locate()
    if not path:
        return f"real schedule document: NOT FOUND -- its checks will SKIP (set {_ENV_VAR})"
    return f"real schedule document: {path}"


@pytest.fixture(scope="session")
def schedule_path(tmp_path_factory) -> str:
    """The synthetic timetable: fixed, structural and always available."""
    from tests import sample_timetable

    path = tmp_path_factory.mktemp("timetable") / "sample-schedule.docx"
    sample_timetable.build(path)
    return str(path)


@pytest.fixture(scope="session")
def real_schedule_path() -> str:
    path = _locate()
    if not path:
        pytest.skip(f"real schedule .docx not found; set {_ENV_VAR} to its path")
    return path


@pytest.fixture(scope="session", params=["sample", "real"])
def document_path(request, schedule_path) -> str:
    """Each document in turn, for checks that hold for any timetable."""
    if request.param == "sample":
        return schedule_path
    return request.getfixturevalue("real_schedule_path")


@pytest.fixture(scope="session")
def document_courses(document_path):
    from autopara.importer.schedule_parser import parse_file

    return parse_file(document_path)


@pytest.fixture(scope="session")
def document_lessons(document_courses):
    return [lesson for course in document_courses for lesson in course.lessons]


@pytest.fixture(scope="session")
def courses(schedule_path):
    from autopara.importer.schedule_parser import parse_file

    return parse_file(schedule_path)


@pytest.fixture(scope="session")
def lessons(courses):
    return [lesson for course in courses for lesson in course.lessons]


@pytest.fixture(scope="session")
def qapp():
    """One QApplication for the whole session.

    Qt allows a single application object per process, so every test that needs Qt -- widgets or
    bare QObject signals -- must share this one. Tests run under the offscreen platform, so no
    window is ever displayed.
    """
    os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
    from PySide6.QtWidgets import QApplication

    return QApplication.instance() or QApplication([])


@pytest.fixture(scope="session")
def gui_app(qapp):
    """Alias kept for widget tests, so their intent reads clearly."""
    return qapp


@pytest.fixture(scope="session")
def hyperlink_targets(document_path) -> set[str]:
    """Every hyperlink target in the document, read straight from the zip.

    Deliberately bypasses the importer: this is the independent oracle the link tests compare
    against, so it must not share code (or bugs) with the parser under test.
    """
    import zipfile
    from xml.etree import ElementTree as ET

    with zipfile.ZipFile(document_path) as archive:
        rels = ET.fromstring(archive.read("word/_rels/document.xml.rels"))
    return {
        rel.get("Target")
        for rel in rels
        if "hyperlink" in (rel.get("Type") or "") and rel.get("Target")
    }


@pytest.fixture(scope="session")
def hyperlink_element_count(document_path) -> int:
    """How many <w:hyperlink> elements the document contains.

    Each lesson cell carries at most one, and neither a gridSpan (shared class) nor a vMerge
    continuation duplicates it, so this equals the number of *cells* that should end up
    contributing a link -- ``ParsedLesson.linked_cells`` summed over the parse. It is cells rather
    than lessons because a class written into two adjacent slots holds one link per slot and merges
    into a single lesson (BACKEND.md R12). Counting elements -- rather than unique URLs -- is what
    catches a link dropped from one lesson while the same URL survives on another.
    """
    import zipfile

    with zipfile.ZipFile(document_path) as archive:
        document = archive.read("word/document.xml").decode("utf-8")
    return document.count("<w:hyperlink")


@pytest.fixture(autouse=True)
def _no_motion(monkeypatch):
    """Nothing in the suite waits on an animation: transitions and fades are switched off.

    The tests that are *about* motion switch it back on for themselves.
    """
    from autopara.ui import transitions

    monkeypatch.setattr(transitions, "ENABLED", False)


@pytest.fixture(autouse=True)
def _no_network(monkeypatch):
    """No test reaches GitHub (or anywhere): the update code takes its network as an argument.

    Anything that tries the real one fails loudly instead of quietly depending on the machine's
    connection -- and a suite that downloaded and "installed" things would be a poor guest.
    """
    import urllib.request

    def refuse(*args, **kwargs):
        raise AssertionError("a test tried to use the network")

    monkeypatch.setattr(urllib.request.OpenerDirector, "open", refuse)
    monkeypatch.setattr(urllib.request, "urlopen", refuse)
