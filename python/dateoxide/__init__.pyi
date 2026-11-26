from __future__ import annotations

from datetime import datetime
from typing import Any, Callable, Literal, Mapping, Sequence, overload

__version__: str

class DateData:
    date_obj: datetime | None
    period: str
    locale: str | None
    def __getitem__(self, key: str) -> Any: ...
    def get(self, key: str, default: Any = None) -> Any: ...

class DateDataParser:
    def __init__(
        self,
        languages: Sequence[str] | None = None,
        locales: Sequence[str] | None = None,
        region: str | None = None,
        try_previous_locales: bool = False,
        use_given_order: bool = False,
        settings: Mapping[str, Any] | None = None,
        detect_languages_function: Callable[..., Any] | None = None,
    ) -> None: ...
    def get_date_data(
        self,
        date_string: str,
        date_formats: Sequence[str] | None = None,
    ) -> DateData: ...
    def parse(
        self,
        date_string: str,
        date_formats: Sequence[str] | None = None,
    ) -> datetime | None: ...
    def parse_batch(self, date_strings: Sequence[str]) -> list[datetime | None]: ...

def parse(
    date_string: str,
    date_formats: Sequence[str] | None = None,
    languages: Sequence[str] | None = None,
    locales: Sequence[str] | None = None,
    region: str | None = None,
    settings: Mapping[str, Any] | None = None,
    detect_languages_function: Callable[..., Any] | None = None,
) -> datetime | None: ...

def parse_batch(
    date_strings: Sequence[str],
    date_formats: Sequence[str] | None = None,
    languages: Sequence[str] | None = None,
    locales: Sequence[str] | None = None,
    region: str | None = None,
    settings: Mapping[str, Any] | None = None,
) -> list[datetime | None]: ...

@overload
def search_dates(
    text: str,
    languages: Sequence[str] | None = None,
    settings: Mapping[str, Any] | None = None,
    add_detected_language: Literal[False] = False,
) -> list[tuple[str, datetime]] | None: ...

@overload
def search_dates(
    text: str,
    languages: Sequence[str] | None = None,
    settings: Mapping[str, Any] | None = None,
    add_detected_language: Literal[True] = ...,
) -> list[tuple[str, datetime, str | None]] | None: ...

def search_dates(
    text: str,
    languages: Sequence[str] | None = None,
    settings: Mapping[str, Any] | None = None,
    add_detected_language: bool = False,
) -> list[tuple[str, datetime]] | list[tuple[str, datetime, str | None]] | None: ...

def supported_languages() -> list[str]: ...
