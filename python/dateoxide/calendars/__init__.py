"""Non-Gregorian calendars powered natively by Rust (Jalali, Hijri, Minguo, French Republican)."""

from __future__ import annotations

from typing import Any

from ..date import DateData


class CalendarBase:
    """Base interface matching ``dateparser.calendars.CalendarBase``."""

    def __init__(self, source: str) -> None:
        self.source = source

    def get_date(self, settings: Any = None) -> DateData | None:
        raise NotImplementedError


from .jalali import JalaliCalendar
from .hijri import HijriCalendar
from .minguo import MinguoCalendar
from .french_republican import FrenchRepublicanCalendar

__all__ = [
    "CalendarBase",
    "JalaliCalendar",
    "HijriCalendar",
    "MinguoCalendar",
    "FrenchRepublicanCalendar",
]
