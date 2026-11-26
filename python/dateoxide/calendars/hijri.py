"""Islamic Hijri calendar parser."""

from __future__ import annotations

from typing import Any

from .._dateoxide import DateData, parse_hijri
from . import CalendarBase


class HijriCalendar(CalendarBase):
    def get_date(self, settings: Any = None) -> DateData | None:
        dd = parse_hijri(self.source, settings=settings)
        return dd if dd.date_obj is not None else None


__all__ = ["HijriCalendar"]
