"""Solar Hijri (Jalali / Persian) calendar parser."""

from __future__ import annotations

from typing import Any

from .._dateoxide import DateData, parse_jalali
from . import CalendarBase


class JalaliCalendar(CalendarBase):
    def get_date(self, settings: Any = None) -> DateData | None:
        dd = parse_jalali(self.source, settings=settings)
        return dd if dd.date_obj is not None else None


__all__ = ["JalaliCalendar"]
