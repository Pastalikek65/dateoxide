from __future__ import annotations

from datetime import datetime, tzinfo
from typing import Any, Callable, Iterable, Literal, Mapping, Sequence, overload

from . import calendars as calendars
from . import conf as conf
from . import custom_language_detection as custom_language_detection
from . import date as date
from . import freshness_date_parser as freshness_date_parser
from . import search as search
from . import timezone_parser as timezone_parser
from . import utils as utils
from ._dateoxide import (
    DateData as DateData,
    DateDataParser as DateDataParser,
    _TimeSpanInfo,
)
from .conf import (
    PartOfDay as PartOfDay,
    PartsOfDay as PartsOfDay,
    SettingValidationError as SettingValidationError,
    Settings as Settings,
    apply_settings as apply_settings,
)

__version__: str

def parse(
    date_string: str,
    date_formats: Sequence[str] | None = None,
    languages: Sequence[str] | None = None,
    locales: Sequence[str] | None = None,
    region: str | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
    detect_languages_function: Callable[..., Sequence[str] | None] | None = None,
) -> datetime | None: ...

def parse_batch(
    date_strings: Iterable[str | None],
    date_formats: Sequence[str] | None = None,
    languages: Sequence[str] | None = None,
    locales: Sequence[str] | None = None,
    region: str | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
) -> list[datetime | None]: ...

def parse_batch_timestamps(
    date_strings: Iterable[str | None],
    date_formats: Sequence[str] | None = None,
    languages: Sequence[str] | None = None,
    locales: Sequence[str] | None = None,
    region: str | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
) -> list[float | None]: ...

def parse_date_range(
    date_range_string: str,
    languages: Sequence[str] | None = None,
    locales: Sequence[str] | None = None,
    region: str | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
) -> tuple[datetime, datetime] | None: ...

@overload
def search_dates(
    text: str,
    languages: Sequence[str] | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
    add_detected_language: Literal[False] = False,
    detect_languages_function: Callable[..., Sequence[str] | None] | None = None,
    strategy: str = "split",
    date_formats: Sequence[str] | None = None,
    add_period: Literal[False] = False,
    return_spans: Literal[False] = False,
) -> list[tuple[str, datetime]] | None: ...

@overload
def search_dates(
    text: str,
    languages: Sequence[str] | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
    add_detected_language: Literal[True] = ...,
    detect_languages_function: Callable[..., Sequence[str] | None] | None = None,
    strategy: str = "split",
    date_formats: Sequence[str] | None = None,
    add_period: Literal[False] = False,
    return_spans: Literal[False] = False,
) -> list[tuple[str, datetime, str | None]] | None: ...

@overload
def search_dates(
    text: str,
    languages: Sequence[str] | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
    add_detected_language: Literal[False] = False,
    detect_languages_function: Callable[..., Sequence[str] | None] | None = None,
    strategy: str = "split",
    date_formats: Sequence[str] | None = None,
    add_period: Literal[True] = ...,
    return_spans: Literal[False] = False,
) -> list[tuple[str, datetime, str]] | None: ...

@overload
def search_dates(
    text: str,
    languages: Sequence[str] | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
    add_detected_language: Literal[True] = ...,
    detect_languages_function: Callable[..., Sequence[str] | None] | None = None,
    strategy: str = "split",
    date_formats: Sequence[str] | None = None,
    add_period: Literal[True] = ...,
    return_spans: Literal[False] = False,
) -> list[tuple[str, datetime, str, str | None]] | None: ...

@overload
def search_dates(
    text: str,
    languages: Sequence[str] | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
    add_detected_language: bool = False,
    detect_languages_function: Callable[..., Sequence[str] | None] | None = None,
    strategy: str = "split",
    date_formats: Sequence[str] | None = None,
    add_period: bool = False,
    return_spans: bool = False,
) -> list[Any] | None: ...

def supported_languages() -> list[str]: ...
def parse_freshness(
    date_string: str,
    languages: Sequence[str] | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
) -> DateData: ...
def pop_tz_offset_from_string(
    date_string: str,
    as_offset: bool = True,
    timezone_abbreviations: Mapping[str, str] | None = None,
) -> tuple[str, tzinfo | str | None]: ...
def parse_jalali(source: str, settings: Settings | Mapping[str, Any] | None = None) -> DateData: ...
def parse_hijri(source: str, settings: Settings | Mapping[str, Any] | None = None) -> DateData: ...
def parse_minguo(source: str, settings: Settings | Mapping[str, Any] | None = None) -> DateData: ...
def parse_french_republican(source: str, settings: Settings | Mapping[str, Any] | None = None) -> DateData: ...
def detect_time_span(text: str) -> _TimeSpanInfo | None: ...
def generate_time_span(
    span_info: Mapping[str, Any],
    base_date: datetime | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
) -> tuple[datetime, datetime]: ...

__all__ = [
    "DateData",
    "DateDataParser",
    "PartOfDay",
    "PartsOfDay",
    "SettingValidationError",
    "Settings",
    "__version__",
    "apply_settings",
    "calendars",
    "conf",
    "custom_language_detection",
    "date",
    "detect_time_span",
    "freshness_date_parser",
    "generate_time_span",
    "parse",
    "parse_batch",
    "parse_batch_timestamps",
    "parse_date_range",
    "parse_french_republican",
    "parse_freshness",
    "parse_hijri",
    "parse_jalali",
    "parse_minguo",
    "pop_tz_offset_from_string",
    "search",
    "search_dates",
    "supported_languages",
    "timezone_parser",
    "utils",
]
