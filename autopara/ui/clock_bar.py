"""A quiet clock at the foot of the rail: the system time, to the second, at half strength.

It is information, not an action, so it is deliberately dim -- half opacity on both the fill and
the text -- and sits under the rail pill rather than competing with the calendar.
"""

from __future__ import annotations

from datetime import datetime

from PySide6.QtCore import Qt, QTimer
from PySide6.QtWidgets import QGraphicsOpacityEffect, QLabel

OPACITY = 0.5
TICK_MS = 1000


def clock_text(now: datetime) -> str:
    return now.strftime("%H:%M:%S")


class ClockBar(QLabel):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("ClockBar")
        self.setAlignment(Qt.AlignCenter)
        self.setFixedSize(60, 20)  # radius 10 in the stylesheet is exactly half of the height
        self.setToolTip("Поточний час")
        effect = QGraphicsOpacityEffect(self)
        effect.setOpacity(OPACITY)
        self.setGraphicsEffect(effect)
        self.tick()
        self._timer = QTimer(self)
        self._timer.timeout.connect(self.tick)
        self._timer.start(TICK_MS)

    def tick(self) -> None:
        self.setText(clock_text(datetime.now()))
