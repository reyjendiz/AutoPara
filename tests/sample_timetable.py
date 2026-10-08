"""A synthetic timetable ``.docx``, built from code, for the suite to run against.

The real timetable is the user's personal document (live meeting links), is not in git and drifts
from one revision to the next, so tests pinned to its numbers failed whenever the university
changed it -- and, with the file absent, simply skipped. This document has the same *structure*
and none of the personal content, and it is fixed, so the tests that describe how the importer
resolves merges can say exactly what they expect.

What it carries, and which rule of docs/BACKEND.md each part exercises:

* six tables, one per course, the fifth with no classes at all (a valid state);
* course I: a class shared by both groups through ``gridSpan`` (R1/R2), classes on every weekday
  (R4: the day is written once, on a ``vMerge`` restart row), one with no link, one on a Friday
  written with a backtick apostrophe (R8);
* course II: a class written into two adjacent slots, one session (R12), and an elective block
  merged vertically over three pairs (R5);
* course IV: seven grid columns and two groups, the second header spanning three (R2);
* every link is a real hyperlink relationship whose visible text is the URL as well (R3).
"""

from __future__ import annotations

import zipfile
from dataclasses import dataclass
from xml.sax.saxutils import escape

W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
HYPERLINK_TYPE = f"{R_NS}/hyperlink"

MEET_HISTORY = "https://meet.google.com/aaa-bbbb-ccc"
MEET_PSYCH = "https://meet.google.com/ddd-eeee-fff"
ZOOM_PEDAGOGY = "https://us02web.zoom.us/j/1000000001"
ZOOM_MEDIA = "https://us02web.zoom.us/j/1000000002"
ZOOM_LANGUAGE = "https://us02web.zoom.us/j/1000000003"
ZOOM_SPORT = "https://us02web.zoom.us/j/1000000004"
ZOOM_SPEECH = "https://us02web.zoom.us/j/1000000005"
ZOOM_THEORY = "https://us02web.zoom.us/j/1000000006"
ZOOM_ART = "https://us02web.zoom.us/j/1000000007"
ZOOM_MUSIC = "https://us02web.zoom.us/j/1000000008"
ZOOM_ECOLOGY = "https://us02web.zoom.us/j/1000000009"
ZOOM_LOGIC = "https://us02web.zoom.us/j/1000000010"
ZOOM_PRACTICE = "https://us02web.zoom.us/j/1000000011"


@dataclass
class C:
    """One lesson cell. ``vm`` is None, "restart" or "continue" (a vertical merge)."""

    lines: tuple[str, ...] = ()
    link: str | None = None
    span: int = 1
    vm: str | None = None


SKIP = object()  # the column a wide cell already covers: it emits nothing


def lesson(subject: str, teacher: str, link: str | None = None, span: int = 1, vm=None) -> C:
    return C((subject, f"({teacher})"), link, span, vm)


def cont(span: int = 1) -> C:
    return C((), None, span, "continue")


@dataclass
class Course:
    heading: str
    grid_columns: int
    groups: list[tuple[str, str, int]]  # (name, specialty, span)
    days: list[tuple[str, list[tuple[str, list]]]]  # (day name, [(time, cells)])


COURSES: list[Course] = [
    Course(
        "І КУРС",
        5,
        [("А2-Бд26-11 група", "Дошкільна освіта", 1), ("А2-Бд26-12 група", "Дошкільна освіта", 1)],
        [
            (
                "Понеділок",
                [
                    ("8.00", [lesson("Історія України", "Іваненко І.І.", MEET_HISTORY, 2), SKIP]),
                    (
                        "9.30",
                        [
                            lesson("Загальна педагогіка", "Роман Н.М.", ZOOM_PEDAGOGY),
                            lesson("Основи медіаграмотності", "Коваль О.П.", ZOOM_MEDIA),
                        ],
                    ),
                    ("11.20", [lesson("Іноземна мова", "Бондар А.В.", ZOOM_LANGUAGE, 2), SKIP]),
                    ("14.40", [lesson("Фізичне виховання", "Мельник С.Д.", ZOOM_SPORT), None]),
                ],
            ),
            (
                "Вівторок",
                [
                    ("8.00", [lesson("Психологія", "Шевчук Т.О.", MEET_PSYCH, 2), SKIP]),
                    ("9.30", [None, lesson("Методика розвитку мовлення", "Ткач Л.І.", ZOOM_SPEECH)]),
                ],
            ),
            (
                "Середа",
                [
                    ("11.20", [lesson("Логіка", "Гриценко В.М.", ZOOM_LOGIC, 2), SKIP]),
                    ("14.40", [C(("Консультація", "див. розклад на сайті"), None, 2), SKIP]),
                ],
            ),
            (
                "Четвер",
                [("9.30", [lesson("Основи екології", "Савчук Н.Р.", ZOOM_ECOLOGY), None])],
            ),
            (
                "П`ятниця",
                [("13.00", [None, lesson("Музичне виховання", "Лисенко Д.А.", ZOOM_MUSIC)])],
            ),
            (
                "Субота",
                [("8.00", [lesson("Практика", "Остапенко Р.Л.", ZOOM_PRACTICE, 2), SKIP])],
            ),
        ],
    ),
    Course(
        "ІІ КУРС",
        5,
        [("21 група", "Дошкільна освіта", 1), ("22 група", "Дошкільна освіта", 1)],
        [
            (
                "Понеділок",
                [
                    (
                        "9.30",
                        [lesson("Історія і теорія дошкільної освіти", "Мазур Є.К.", ZOOM_THEORY, 2), SKIP],
                    ),
                    (
                        "11.20",
                        [lesson("Історія і теорія дошкільної освіти", "Мазур Є.К.", ZOOM_THEORY, 2), SKIP],
                    ),
                ],
            ),
            (
                "Вівторок",
                [
                    ("8.00", [C(("Дисципліна вільного вибору",), None, 2, "restart"), SKIP]),
                    ("9.30", [cont(2), SKIP]),
                    ("11.20", [cont(2), SKIP]),
                ],
            ),
            ("Середа", [("8.00", [lesson("Образотворче мистецтво", "Дзюба К.П.", ZOOM_ART), None])]),
        ],
    ),
    Course(
        "ІІІ КУРС",
        5,
        [("31 група", "Дошкільна освіта", 1), ("32 група", "Дошкільна освіта", 1)],
        [
            (
                "Середа",
                [
                    ("8.00", [C(("Дисципліна вільного вибору",), None, 2, "restart"), SKIP]),
                    ("9.30", [cont(2), SKIP]),
                    ("11.20", [cont(2), SKIP]),
                ],
            ),
            ("П'ятниця", [("13.00", [None, lesson("Теорія мистецтва", "Дзюба К.П.", ZOOM_ART)])]),
        ],
    ),
    Course(
        "ІV КУРС",
        7,
        [("41 група", "Дошкільна освіта", 1), ("42 група", "Дошкільна освіта", 3)],
        [
            (
                "Понеділок",
                [
                    (
                        "8.00",
                        [
                            lesson("Основи екології", "Савчук Н.Р.", ZOOM_ECOLOGY),
                            lesson("Логіка", "Гриценко В.М.", ZOOM_LOGIC, 3),
                            SKIP,
                            SKIP,
                        ],
                    )
                ],
            ),
            ("Четвер", [("9.30", [None, lesson("Музичне виховання", "Лисенко Д.А.", ZOOM_MUSIC, 3), SKIP, SKIP])]),
        ],
    ),
    Course("V КУРС", 4, [("51 група", "Дошкільна освіта", 1)], []),
    Course(
        "VI КУРС",
        4,
        [("61 група", "Дошкільна освіта", 1)],
        [
            (
                "Четвер",
                [
                    ("8.00", [C(("Дисципліна вільного вибору",), None, 1, "restart")]),
                    ("9.30", [cont()]),
                    ("11.20", [cont()]),
                ],
            ),
            ("Субота", [("11.20", [lesson("Семінар", "Руденко О.І.", ZOOM_SPEECH)])]),
        ],
    ),
]


class _Links:
    """Hands out relationship ids, one per hyperlink element (the document may reuse a target)."""

    def __init__(self) -> None:
        self.targets: list[str] = []

    def add(self, url: str) -> str:
        self.targets.append(url)
        return f"rId{len(self.targets)}"


def _run(text: str) -> str:
    return f'<w:r><w:t xml:space="preserve">{escape(text)}</w:t></w:r>'


def _paragraph(lines, link: str | None, links: _Links) -> str:
    runs: list[str] = []
    for index, line in enumerate(lines):
        if index:
            runs.append("<w:r><w:br/></w:r>")
        runs.append(_run(line))
    if link:
        runs.append("<w:r><w:br/></w:r>")
        runs.append(f'<w:hyperlink r:id="{links.add(link)}">{_run(link)}</w:hyperlink>')
    return f"<w:p>{''.join(runs)}</w:p>"


def _cell(lines=(), link=None, span=1, vm=None, links: _Links | None = None) -> str:
    props = ""
    if span > 1:
        props += f'<w:gridSpan w:val="{span}"/>'
    if vm == "restart":
        props += '<w:vMerge w:val="restart"/>'
    elif vm == "continue":
        props += "<w:vMerge/>"
    properties = f"<w:tcPr>{props}</w:tcPr>" if props else ""
    return f"<w:tc>{properties}{_paragraph(lines, link, links or _Links())}</w:tc>"


def _row(cells: list[str]) -> str:
    return f"<w:tr>{''.join(cells)}</w:tr>"


def _table(course: Course, links: _Links) -> str:
    grid = "".join('<w:gridCol w:w="1500"/>' for _ in range(course.grid_columns))
    head_groups = [_cell((name,), span=span) for name, _, span in course.groups]
    head_specialty = [_cell((specialty,), span=span) for _, specialty, span in course.groups]
    rows = [
        _row([_cell(("День",)), _cell(("Пара",)), _cell(("Час",))] + head_groups),
        _row([_cell(), _cell(), _cell()] + head_specialty),
    ]
    for day, slots in course.days:
        for position, (time, cells) in enumerate(slots):
            pair = {"8.00": 1, "9.30": 2, "11.20": 3, "13.00": 4, "14.40": 5, "16.10": 6}[time]
            day_cell = (
                _cell((day,), vm="restart", links=links)
                if position == 0
                else _cell(vm="continue", links=links)
            )
            if len(slots) == 1:
                day_cell = _cell((day,), links=links)
            body = [day_cell, _cell((str(pair),), links=links), _cell((time,), links=links)]
            for item in cells:
                if item is SKIP:
                    continue
                if item is None:
                    body.append(_cell(links=links))
                else:
                    body.append(_cell(item.lines, item.link, item.span, item.vm, links))
            rows.append(_row(body))
    return f"<w:tbl><w:tblPr/><w:tblGrid>{grid}</w:tblGrid>{''.join(rows)}</w:tbl>"


def build(path) -> list[str]:
    """Write the document to ``path``; returns every hyperlink target, in document order."""
    links = _Links()
    body = []
    for course in COURSES:
        body.append(_paragraph((course.heading,), None, links))
        body.append(_table(course, links))
    document = (
        f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<w:document xmlns:w="{W_NS}" xmlns:r="{R_NS}"><w:body>{"".join(body)}</w:body></w:document>'
    )
    relationships = "".join(
        f'<Relationship Id="rId{i}" Type="{HYPERLINK_TYPE}" Target="{escape(url)}" TargetMode="External"/>'
        for i, url in enumerate(links.targets, start=1)
    )
    rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{relationships}</Relationships>'
    )
    content_types = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        "</Types>"
    )
    root_rels = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>'
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f'<Relationship Id="rId1" Type="{R_NS}/officeDocument" Target="word/document.xml"/>'
        "</Relationships>"
    )
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        archive.writestr("[Content_Types].xml", content_types)
        archive.writestr("_rels/.rels", root_rels)
        archive.writestr("word/document.xml", document)
        archive.writestr("word/_rels/document.xml.rels", rels)
    return links.targets
