"""dateoxide: Blazing-fast, low-memory multilingual date parser written in Rust.

Drop-in accelerator for Python's ``dateparser`` supporting 205 languages,
non-Gregorian calendars, time spans, and GIL-free parallel batch processing.
"""

from __future__ import annotations

import importlib
from typing import TYPE_CHECKING, Any

from ._dateoxide import (
    DateData,
    DateDataParser,
    __version__,
    detect_time_span,
    generate_time_span,
    parse,
    parse_batch,
    parse_batch_timestamps,
    parse_date_range,
    parse_french_republican,
    parse_freshness,
    parse_hijri,
    parse_jalali,
    parse_minguo,
    pop_tz_offset_from_string,
    search_dates,
    supported_languages,
)

if TYPE_CHECKING:
    from . import (
        calendars,
        conf,
        custom_language_detection,
        date,
        freshness_date_parser,
        search,
        timezone_parser,
        utils,
    )
    from .conf import PartOfDay, PartsOfDay, SettingValidationError, Settings, apply_settings

_LAZY_SUBMODULES = {
    "calendars",
    "conf",
    "custom_language_detection",
    "date",
    "freshness_date_parser",
    "search",
    "timezone_parser",
    "utils",
}
_LAZY_CONF_ATTRS = {
    "PartOfDay",
    "PartsOfDay",
    "SettingValidationError",
    "Settings",
    "apply_settings",
}


def __getattr__(name: str) -> Any:
    if name in _LAZY_SUBMODULES:
        mod = importlib.import_module(f".{name}", __name__)
        globals()[name] = mod
        return mod
    if name in _LAZY_CONF_ATTRS:
        conf_mod = importlib.import_module(".conf", __name__)
        val = getattr(conf_mod, name)
        globals()[name] = val
        return val
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(__all__))


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
