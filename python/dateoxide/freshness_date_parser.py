"""Drop-in ``dateparser.freshness_date_parser`` compatible submodule."""

from __future__ import annotations

from datetime import datetime
from typing import Any, Mapping, Sequence

from ._dateoxide import DateData, parse_freshness


class FreshnessDateDataParser:
    """Parser specialized in relative/freshness date expressions."""

    def get_date_data(
        self,
        date_string: str,
        settings: Any = None,
        languages: Sequence[str] | None = None,
    ) -> DateData:
        if isinstance(languages, (str, bytes)):
            raise TypeError("languages must be a sequence of language codes, not str/bytes")
        return parse_freshness(
            date_string,
            languages=list(languages) if languages is not None else None,
            settings=settings,
        )

    def parse(
        self,
        date_string: str,
        settings: Any = None,
        languages: Sequence[str] | None = None,
    ) -> datetime | None:
        return self.get_date_data(date_string, settings=settings, languages=languages).date_obj

    def get_kwargs(self, date_string: str) -> dict[str, Any]:
        dd = self.get_date_data(date_string)
        return {"date_obj": dd.date_obj, "period": dd.period, "locale": dd.locale}


freshness_date_parser = FreshnessDateDataParser()

__all__ = ["FreshnessDateDataParser", "freshness_date_parser"]
