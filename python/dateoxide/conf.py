"""Configuration classes and validation compatible with ``dateparser.conf``."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, time
from enum import Enum
from functools import wraps
import math
import sys
from typing import Any, Callable, Mapping

from ._dateoxide import SettingValidationError, parse as _raw_parse, pop_tz_offset_from_string as _pop_tz

if sys.version_info >= (3, 11):
    from enum import StrEnum as _StrEnumBase
else:
    class _StrEnumBase(str, Enum):
        pass


class PartOfDay(_StrEnumBase):
    EARLY_MORNING = "early_morning"
    MORNING = "morning"
    AFTERNOON = "afternoon"
    EVENING = "evening"
    NIGHT = "night"


@dataclass
class PartsOfDay:
    early_morning: time = time(3, 0)
    morning: time = time(9, 0)
    afternoon: time = time(15, 0)
    evening: time = time(18, 0)
    night: time = time(20, 0)


_VALID_DATE_ORDERS = {"MDY", "DMY", "YMD", "YDM", "MYD", "DYM"}
_VALID_STRICT_DATE_ORDERS = {"none", "year", "all"}
_VALID_PREFER_DATES_FROM = {"current_period", "past", "future"}
_VALID_PREFER_DAY_OF_MONTH = {"current", "first", "last"}
_VALID_PREFER_MONTH_OF_YEAR = {"current", "first", "last"}
_VALID_START_OF_WEEK = {"monday", "sunday"}
_VALID_PARSERS = {
    "timestamp",
    "negative-timestamp",
    "relative-time",
    "custom-formats",
    "absolute-time",
    "base-formats",
    "no-spaces-time",
}
_VALID_SETTINGS_KEYS = {
    "DATE_ORDER",
    "PREFER_LOCALE_DATE_ORDER",
    "STRICT_DATE_ORDER",
    "TIMEZONE",
    "TO_TIMEZONE",
    "RETURN_AS_TIMEZONE_AWARE",
    "TIMEZONE_ABBREVIATIONS",
    "PREFER_DAY_OF_MONTH",
    "PREFER_MONTH_OF_YEAR",
    "PREFER_DATES_FROM",
    "RELATIVE_BASE",
    "STRICT_PARSING",
    "REQUIRE_PARTS",
    "SKIP_TOKENS",
    "NORMALIZE",
    "DEFAULT_LANGUAGES",
    "USE_GIVEN_LANGUAGE_ORDER",
    "LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD",
    "RETURN_TIME_SPAN",
    "DEFAULT_START_OF_WEEK",
    "DEFAULT_DAYS_IN_MONTH",
    "RETURN_TIME_AS_PERIOD",
    "PARSERS",
    "IGNORE_SURROUNDING_TEXT",
    "CACHE_SIZE_LIMIT",
    "PARTS_OF_DAY",
}


class Settings:
    """Typed configuration container matching ``dateparser.conf.Settings``."""

    _initialized: bool
    _explicit_keys: set[str]

    def __init__(self, settings: Mapping[str, Any] | None = None, **kwargs: Any) -> None:
        object.__setattr__(self, "_initialized", False)
        self.DATE_ORDER: str = "MDY"
        self.PREFER_LOCALE_DATE_ORDER: bool = True
        self.STRICT_DATE_ORDER: str = "none"
        self.TIMEZONE: str = "local"
        self.TO_TIMEZONE: str | bool = False
        self.RETURN_AS_TIMEZONE_AWARE: str | bool = "default"
        self.TIMEZONE_ABBREVIATIONS: dict[str, str] = {}
        self.PREFER_DAY_OF_MONTH: str = "current"
        self.PREFER_MONTH_OF_YEAR: str = "current"
        self.PREFER_DATES_FROM: str = "current_period"
        self.RELATIVE_BASE: Any = False
        self.STRICT_PARSING: bool = False
        self.REQUIRE_PARTS: list[str] = []
        self.SKIP_TOKENS: list[str] = ["t"]
        self.NORMALIZE: bool = True
        self.DEFAULT_LANGUAGES: list[str] = []
        self.USE_GIVEN_LANGUAGE_ORDER: bool = False
        self.LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD: float = 0.5
        self.RETURN_TIME_SPAN: bool = False
        self.DEFAULT_START_OF_WEEK: str = "monday"
        self.DEFAULT_DAYS_IN_MONTH: int = 30
        self.RETURN_TIME_AS_PERIOD: bool = False
        self.PARSERS: list[str] = [
            "timestamp",
            "relative-time",
            "custom-formats",
            "absolute-time",
        ]
        self.IGNORE_SURROUNDING_TEXT: bool = False
        self.CACHE_SIZE_LIMIT: int = 1000
        self.PARTS_OF_DAY: PartsOfDay = PartsOfDay()

        merged: dict[str, Any] = {}
        if settings is not None:
            merged.update(dict(settings))
        merged.update(kwargs)

        explicit_keys: set[str] = set()
        for k, v in merged.items():
            if not hasattr(self, k):
                raise SettingValidationError(f"Unknown setting: {k!r}")
            object.__setattr__(self, k, v)
            explicit_keys.add(k)

        object.__setattr__(self, "_explicit_keys", explicit_keys)
        object.__setattr__(self, "_initialized", True)
        check_settings(self)

    def __setattr__(self, name: str, value: Any) -> None:
        if getattr(self, "_initialized", False) and not name.startswith("_"):
            self._explicit_keys.add(name)
        object.__setattr__(self, name, value)

    def replace(self, mod_settings: Mapping[str, Any] | None = None, **kw: Any) -> Settings:
        data = self.as_effective_dict()
        if mod_settings is not None:
            data.update(mod_settings)
        data.update(kw)
        return Settings(settings=data)

    def as_effective_dict(self) -> dict[str, Any]:
        """Return settings dictionary omitting un-overridden DATE_ORDER/SKIP_TOKENS/PARSERS defaults
        so bare `Settings()` preserves `PREFER_LOCALE_DATE_ORDER`, `region`, and zero-alloc fast-path."""
        d = self.as_dict()
        explicit = getattr(self, "_explicit_keys", None)
        if explicit is not None:
            for key in ("DATE_ORDER", "SKIP_TOKENS", "PARSERS"):
                if key not in explicit:
                    d.pop(key, None)
        return d

    def as_dict(self) -> dict[str, Any]:
        return {
            "DATE_ORDER": self.DATE_ORDER,
            "PREFER_LOCALE_DATE_ORDER": self.PREFER_LOCALE_DATE_ORDER,
            "STRICT_DATE_ORDER": self.STRICT_DATE_ORDER,
            "TIMEZONE": self.TIMEZONE,
            "TO_TIMEZONE": self.TO_TIMEZONE,
            "RETURN_AS_TIMEZONE_AWARE": self.RETURN_AS_TIMEZONE_AWARE,
            "TIMEZONE_ABBREVIATIONS": dict(self.TIMEZONE_ABBREVIATIONS),
            "PREFER_DAY_OF_MONTH": self.PREFER_DAY_OF_MONTH,
            "PREFER_MONTH_OF_YEAR": self.PREFER_MONTH_OF_YEAR,
            "PREFER_DATES_FROM": self.PREFER_DATES_FROM,
            "RELATIVE_BASE": self.RELATIVE_BASE,
            "STRICT_PARSING": self.STRICT_PARSING,
            "REQUIRE_PARTS": list(self.REQUIRE_PARTS),
            "SKIP_TOKENS": list(self.SKIP_TOKENS),
            "NORMALIZE": self.NORMALIZE,
            "DEFAULT_LANGUAGES": list(self.DEFAULT_LANGUAGES),
            "USE_GIVEN_LANGUAGE_ORDER": self.USE_GIVEN_LANGUAGE_ORDER,
            "LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD": self.LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD,
            "RETURN_TIME_SPAN": self.RETURN_TIME_SPAN,
            "DEFAULT_START_OF_WEEK": self.DEFAULT_START_OF_WEEK,
            "DEFAULT_DAYS_IN_MONTH": self.DEFAULT_DAYS_IN_MONTH,
            "RETURN_TIME_AS_PERIOD": self.RETURN_TIME_AS_PERIOD,
            "PARSERS": list(self.PARSERS),
            "IGNORE_SURROUNDING_TEXT": self.IGNORE_SURROUNDING_TEXT,
            "CACHE_SIZE_LIMIT": self.CACHE_SIZE_LIMIT,
            "PARTS_OF_DAY": self.PARTS_OF_DAY,
        }

    def __repr__(self) -> str:
        return f"Settings({self.as_dict()!r})"


def check_settings(cfg: Settings | Mapping[str, Any]) -> None:
    d = (
        {k: v for k, v in vars(cfg).items() if not k.startswith("_")}
        if isinstance(cfg, Settings)
        else dict(cfg)
    )
    for key in d:
        if key not in _VALID_SETTINGS_KEYS:
            raise SettingValidationError(f"{key!r} is not a valid setting")
    if "DATE_ORDER" in d and d["DATE_ORDER"] is not None:
        if not isinstance(d["DATE_ORDER"], str) or d["DATE_ORDER"].upper() not in _VALID_DATE_ORDERS:
            raise SettingValidationError(
                f"{d['DATE_ORDER']!r} is not a valid DATE_ORDER"
            )
    if "STRICT_DATE_ORDER" in d and d["STRICT_DATE_ORDER"] is not None:
        if (
            not isinstance(d["STRICT_DATE_ORDER"], str)
            or d["STRICT_DATE_ORDER"].lower() not in _VALID_STRICT_DATE_ORDERS
        ):
            raise SettingValidationError(
                f"{d['STRICT_DATE_ORDER']!r} is not a valid STRICT_DATE_ORDER"
            )
    if "PREFER_DATES_FROM" in d and d["PREFER_DATES_FROM"] is not None:
        if (
            not isinstance(d["PREFER_DATES_FROM"], str)
            or d["PREFER_DATES_FROM"].lower() not in _VALID_PREFER_DATES_FROM
        ):
            raise SettingValidationError(
                f"{d['PREFER_DATES_FROM']!r} is not a valid PREFER_DATES_FROM"
            )
    if "PREFER_DAY_OF_MONTH" in d and d["PREFER_DAY_OF_MONTH"] is not None:
        if (
            not isinstance(d["PREFER_DAY_OF_MONTH"], str)
            or d["PREFER_DAY_OF_MONTH"].lower() not in _VALID_PREFER_DAY_OF_MONTH
        ):
            raise SettingValidationError(
                f"{d['PREFER_DAY_OF_MONTH']!r} is not a valid PREFER_DAY_OF_MONTH"
            )
    if "PREFER_MONTH_OF_YEAR" in d and d["PREFER_MONTH_OF_YEAR"] is not None:
        if (
            not isinstance(d["PREFER_MONTH_OF_YEAR"], str)
            or d["PREFER_MONTH_OF_YEAR"].lower() not in _VALID_PREFER_MONTH_OF_YEAR
        ):
            raise SettingValidationError(
                f"{d['PREFER_MONTH_OF_YEAR']!r} is not a valid PREFER_MONTH_OF_YEAR"
            )
    if "DEFAULT_START_OF_WEEK" in d and d["DEFAULT_START_OF_WEEK"] is not None:
        if (
            not isinstance(d["DEFAULT_START_OF_WEEK"], str)
            or d["DEFAULT_START_OF_WEEK"].lower() not in _VALID_START_OF_WEEK
        ):
            raise SettingValidationError(
                f"{d['DEFAULT_START_OF_WEEK']!r} is not a valid DEFAULT_START_OF_WEEK"
            )
    if "PARSERS" in d and d["PARSERS"] is not None:
        if isinstance(d["PARSERS"], (str, bytes)):
            raise SettingValidationError("PARSERS must be a sequence of parser names")
        for p in d["PARSERS"]:
            if not isinstance(p, str) or p.lower() not in _VALID_PARSERS:
                raise SettingValidationError(f"{p!r} is not a valid parser name")
    if "REQUIRE_PARTS" in d and d["REQUIRE_PARTS"] is not None:
        if isinstance(d["REQUIRE_PARTS"], (str, bytes)):
            raise SettingValidationError("REQUIRE_PARTS must be a sequence of part names")
        for part in d["REQUIRE_PARTS"]:
            if not isinstance(part, str) or part.lower() not in {"year", "month", "day"}:
                raise SettingValidationError(f"{part!r} is not a valid REQUIRE_PARTS item")
    for seq_key in ("DEFAULT_LANGUAGES", "SKIP_TOKENS"):
        if seq_key in d and d[seq_key] is not None:
            val_seq = d[seq_key]
            if isinstance(val_seq, (str, bytes)) or not isinstance(val_seq, (list, tuple, set)):
                raise SettingValidationError(f"{seq_key} must be a sequence of strings")
            for item in val_seq:
                if not isinstance(item, str):
                    raise SettingValidationError(f"{seq_key} items must be strings")
    for bool_key in (
        "PREFER_LOCALE_DATE_ORDER",
        "STRICT_PARSING",
        "NORMALIZE",
        "USE_GIVEN_LANGUAGE_ORDER",
        "RETURN_TIME_SPAN",
        "RETURN_TIME_AS_PERIOD",
        "IGNORE_SURROUNDING_TEXT",
    ):
        if bool_key in d and d[bool_key] is not None and not isinstance(d[bool_key], bool):
            raise SettingValidationError(f"{bool_key} must be a bool")
    if "RELATIVE_BASE" in d and d["RELATIVE_BASE"] not in (None, False):
        if isinstance(d["RELATIVE_BASE"], bool) or not isinstance(d["RELATIVE_BASE"], datetime):
            raise SettingValidationError("RELATIVE_BASE must be a datetime object")
    if "RETURN_AS_TIMEZONE_AWARE" in d and d["RETURN_AS_TIMEZONE_AWARE"] is not None:
        val = d["RETURN_AS_TIMEZONE_AWARE"]
        if not isinstance(val, bool) and not (isinstance(val, str) and val.lower() == "default"):
            raise SettingValidationError(f"Invalid RETURN_AS_TIMEZONE_AWARE: {val!r}")
    if "DEFAULT_DAYS_IN_MONTH" in d and d["DEFAULT_DAYS_IN_MONTH"] is not None:
        dim = d["DEFAULT_DAYS_IN_MONTH"]
        if isinstance(dim, bool) or not isinstance(dim, int) or not (1 <= dim <= 31):
            raise SettingValidationError(f"Invalid DEFAULT_DAYS_IN_MONTH: {dim!r}")
    if "CACHE_SIZE_LIMIT" in d and d["CACHE_SIZE_LIMIT"] is not None:
        csl = d["CACHE_SIZE_LIMIT"]
        if isinstance(csl, bool) or not isinstance(csl, int) or csl < 0:
            raise SettingValidationError(f"Invalid CACHE_SIZE_LIMIT: {csl!r}")
    if "LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD" in d and d["LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD"] is not None:
        thr = d["LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD"]
        if (
            isinstance(thr, bool)
            or not isinstance(thr, (int, float))
            or not math.isfinite(thr)
            or not (0.0 <= float(thr) <= 1.0)
        ):
            raise SettingValidationError(f"Invalid LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD: {thr!r}")
    abbrevs = d.get("TIMEZONE_ABBREVIATIONS")
    if abbrevs is not None and not isinstance(abbrevs, Mapping):
        raise SettingValidationError("TIMEZONE_ABBREVIATIONS must be a mapping")
    tz_val = d.get("TIMEZONE")
    if tz_val is not None:
        if not isinstance(tz_val, str) or not tz_val.strip():
            raise SettingValidationError(f"Invalid TIMEZONE: {tz_val!r}")
        if tz_val.lower() != "local":
            _, parsed_tz = _pop_tz(f"2026-01-01 00:00 {tz_val}", True, dict(abbrevs) if abbrevs else None)
            if parsed_tz is None:
                raise SettingValidationError(f"Invalid TIMEZONE: {tz_val!r}")
    to_tz = d.get("TO_TIMEZONE")
    if to_tz not in (None, False):
        if isinstance(to_tz, bool) or not isinstance(to_tz, str) or not to_tz.strip():
            raise SettingValidationError(f"Invalid TO_TIMEZONE: {to_tz!r}")
        _, parsed_to = _pop_tz(f"2026-01-01 00:00 {to_tz}", True, dict(abbrevs) if abbrevs else None)
        if parsed_to is None:
            raise SettingValidationError(f"Invalid TO_TIMEZONE: {to_tz!r}")


settings = Settings()


def apply_settings(f: Callable[..., Any]) -> Callable[..., Any]:
    """Decorator mirroring ``dateparser.conf.apply_settings``."""

    @wraps(f)
    def wrapper(*args: Any, **kwargs: Any) -> Any:
        s = kwargs.get("settings")
        if isinstance(s, Mapping):
            check_settings(s)
            kwargs["settings"] = Settings(settings=s)
        elif isinstance(s, Settings):
            check_settings(s)
        elif s is None:
            kwargs["settings"] = settings
        else:
            raise SettingValidationError("settings must be a dict or Settings instance")
        return f(*args, **kwargs)

    return wrapper


# Expose raw parse reference if needed internally
_ = _raw_parse
