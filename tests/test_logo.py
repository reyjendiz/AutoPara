"""The app mark is one shipped picture; everything else is made from it."""

from __future__ import annotations

import struct

from PySide6.QtGui import QColor

from autopara.ui import tray


def test_the_logo_ships_with_the_app():
    assert tray.LOGO_PATH.is_file()
    assert tray.LOGO_PATH.suffix == ".png"


def test_it_is_a_square_with_see_through_corners(qapp):
    source = tray._source().toImage()
    assert source.width() == source.height() == 1024
    assert QColor.fromRgba(source.pixel(0, 0)).alpha() == 0, "a rounded tile, not a filled square"
    assert QColor.fromRgba(source.pixel(512, 512)).alpha() == 255


def test_every_size_comes_from_it_at_twice_the_pixels(qapp):
    for size in (16, 30, 64, 96):
        pixmap = tray.icon_pixmap(size)
        assert pixmap.width() == pixmap.height() == size * 2
        assert pixmap.devicePixelRatio() == 2


def test_it_is_not_blank_at_the_smallest_size(qapp):
    image = tray.icon_pixmap(16).toImage()
    opaque = sum(
        QColor.fromRgba(image.pixel(x, y)).alpha() > 200
        for x in range(image.width())
        for y in range(image.height())
    )
    assert opaque > image.width() * image.height() * 0.5


def test_the_application_icon_is_built_from_it(qapp):
    assert not tray.build_icon().isNull()


def test_the_ico_holds_every_size(qapp, tmp_path):
    raw = tray.write_ico(tmp_path / "AutoPara.ico").read_bytes()
    _, _, count = struct.unpack("<HHH", raw[:6])
    assert count == len(tray.ICO_SIZES)
