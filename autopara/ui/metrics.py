"""Sizes that Qt will not work out from the stylesheet on its own."""

from __future__ import annotations

# The height of every input in a dialog. A line edit and a combo box take their height from the
# stylesheet's padding, but a spin box or time edit reports a size hint that ignores it: left to
# the layout, the rows were allotted 20 px, drew 36, and sat on top of one another. Pinning the
# height here makes every field -- and so every form row -- the same.
INPUT_HEIGHT = 36


def settle(dialog) -> None:
    """Measure a dialog again, now that the stylesheet has reached its widgets.

    A widget is first measured before the stylesheet's padding is applied, and a spin box does not
    announce that its size changed when it is. The layout therefore went on believing the dialog
    needed 650 px when it needed 750, and squeezed the rows of the form onto one another. Polishing
    every child, invalidating the layout and sizing the dialog afresh makes the first measurement
    the real one.

    The dialog is then resized to its hint directly. ``adjustSize`` would do it, but it caps a
    top-level window at two thirds of the screen, so on an ordinary laptop a tall form was given
    less room than its rows need and the same squeeze came back.
    """
    from PySide6.QtWidgets import QWidget

    dialog.ensurePolished()
    for child in dialog.findChildren(QWidget):
        child.ensurePolished()
        child.updateGeometry()
    layout = dialog.layout()
    if layout is not None:
        layout.invalidate()
    dialog.resize(dialog.sizeHint().expandedTo(dialog.minimumSize()))
