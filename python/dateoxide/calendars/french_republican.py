"""French Republican calendar parser."""

from __future__ import annotations

from typing import Any

from .._dateoxide import DateData, parse_french_republican
from . import CalendarBase


class FrenchRepublicanCalendar(CalendarBase):
    def get_date(self, settings: Any = None) -> DateData | None:
        dd = parse_french_republican(self.source, settings=settings)
        return dd if dd.date_obj is not None else None


__all__ = ["FrenchRepublicanCalendar"]
