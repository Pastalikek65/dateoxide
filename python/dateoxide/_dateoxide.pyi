from __future__ import annotations

from datetime import datetime, tzinfo
from typing import Any, Callable, Iterable, Literal, Mapping, Sequence, TypedDict, final, overload

from .conf import PartOfDay, Settings

__version__: str

class _TimeSpanInfo(TypedDict, total=False):
    type: str
    direction: str
    matched_text: str
    start_pos: int
    end_pos: int
    number: int

class SettingValidationError(ValueError): ...

@final
class DateData:
    date_obj: datetime | None
    period: str
    locale: str | None
    parts: tuple[str, ...]
    part_of_day: PartOfDay | None
    def __new__(
        cls,
        *,
        date_obj: datetime | None = None,
        period: str | None = None,
        locale: str | None = None,
        parts: Sequence[str] | None = None,
        part_of_day: PartOfDay | None = None,
    ) -> DateData: ...
    def __getitem__(self, key: str, /) -> Any: ...
    def get(self, key: str, default: Any = None) -> Any: ...
    def __eq__(self, value: object, /) -> bool: ...
    def __repr__(self) -> str: ...
    def __reduce__(self) -> tuple[Any, tuple[Any, ...]]: ...

@final
class DateDataParser:
    def __new__(
        cls,
        languages: Sequence[str] | None = None,
        locales: Sequence[str] | None = None,
        region: str | None = None,
        try_previous_locales: bool = False,
        use_given_order: bool = False,
        settings: Settings | Mapping[str, Any] | None = None,
        detect_languages_function: Callable[..., Sequence[str]] | None = None,
    ) -> DateDataParser: ...
    def get_date_data(
        self,
        date_string: str,
        date_formats: Sequence[str] | None = None,
    ) -> DateData: ...
    def get_date_tuple(
        self,
        date_string: str,
        date_formats: Sequence[str] | None = None,
    ) -> tuple[datetime | None, str, str | None]: ...
    def parse(
        self,
        date_string: str,
        date_formats: Sequence[str] | None = None,
    ) -> datetime | None: ...
    def parse_batch(self, date_strings: Iterable[str | None]) -> list[datetime | None]: ...
    def parse_date_range(self, date_range_string: str) -> tuple[datetime, datetime] | None: ...

def parse(
    date_string: str,
    date_formats: Sequence[str] | None = None,
    languages: Sequence[str] | None = None,
    locales: Sequence[str] | None = None,
    region: str | None = None,
    settings: Settings | Mapping[str, Any] | None = None,
    detect_languages_function: Callable[..., Sequence[str]] | None = None,
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
    detect_languages_function: Callable[..., Sequence[str]] | None = None,
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
    detect_languages_function: Callable[..., Sequence[str]] | None = None,
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
    detect_languages_function: Callable[..., Sequence[str]] | None = None,
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
    detect_languages_function: Callable[..., Sequence[str]] | None = None,
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
    detect_languages_function: Callable[..., Sequence[str]] | None = None,
    strategy: str = "split",
    date_formats: Sequence[str] | None = None,
    add_period: bool = False,
    return_spans: bool = False,
) -> list[Any] | None: ...

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
def supported_languages() -> list[str]: ...
def _unpickle_datedata(
    date_obj: datetime | None,
    period: str,
    locale: str | None,
    parts: list[str],
    part_of_day: PartOfDay | None,
) -> DateData: ...

__all__ = [
    "DateData",
    "DateDataParser",
    "SettingValidationError",
    "__version__",
    "_unpickle_datedata",
    "detect_time_span",
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
    "search_dates",
    "supported_languages",
]
