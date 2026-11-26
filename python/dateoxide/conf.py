"""Configuration classes and validation compatible with ``dateparser.conf``."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import time
from enum import Enum
from functools import wraps
from typing import Any, Callable, Iterable, Mapping

from ._dateoxide import parse as _raw_parse


class SettingValidationError(ValueError):
    """Raised when invalid configuration values are passed to ``Settings``."""


class PartOfDay(str, Enum):
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
    "relative-time",
    "custom-formats",
    "absolute-time",
    "base-formats",
    "no-spaces-time",
}


class Settings:
    """Typed configuration container matching ``dateparser.conf.Settings``."""

    def __init__(self, settings: Mapping[str, Any] | None = None, **kwargs: Any) -> None:
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

        for k, v in merged.items():
            if not hasattr(self, k):
                raise SettingValidationError(f"Unknown setting: {k!r}")
            setattr(self, k, v)

        check_settings(self)

    def replace(self, **kw: Any) -> Settings:
        data = self.as_dict()
        data.update(kw)
        return Settings(settings=data)

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
    d = cfg.as_dict() if isinstance(cfg, Settings) else dict(cfg)
    if "DATE_ORDER" in d and d["DATE_ORDER"] is not None:
        if str(d["DATE_ORDER"]).upper() not in _VALID_DATE_ORDERS:
            raise SettingValidationError(
                f"{d['DATE_ORDER']!r} is not a valid DATE_ORDER"
            )
    if "STRICT_DATE_ORDER" in d and d["STRICT_DATE_ORDER"] is not None:
        if str(d["STRICT_DATE_ORDER"]).lower() not in _VALID_STRICT_DATE_ORDERS:
            raise SettingValidationError(
                f"{d['STRICT_DATE_ORDER']!r} is not a valid STRICT_DATE_ORDER"
            )
    if "PREFER_DATES_FROM" in d and d["PREFER_DATES_FROM"] is not None:
        if str(d["PREFER_DATES_FROM"]).lower() not in _VALID_PREFER_DATES_FROM:
            raise SettingValidationError(
                f"{d['PREFER_DATES_FROM']!r} is not a valid PREFER_DATES_FROM"
            )
    if "PREFER_DAY_OF_MONTH" in d and d["PREFER_DAY_OF_MONTH"] is not None:
        if str(d["PREFER_DAY_OF_MONTH"]).lower() not in _VALID_PREFER_DAY_OF_MONTH:
            raise SettingValidationError(
                f"{d['PREFER_DAY_OF_MONTH']!r} is not a valid PREFER_DAY_OF_MONTH"
            )
    if "PREFER_MONTH_OF_YEAR" in d and d["PREFER_MONTH_OF_YEAR"] is not None:
        if str(d["PREFER_MONTH_OF_YEAR"]).lower() not in _VALID_PREFER_MONTH_OF_YEAR:
            raise SettingValidationError(
                f"{d['PREFER_MONTH_OF_YEAR']!r} is not a valid PREFER_MONTH_OF_YEAR"
            )
    if "DEFAULT_START_OF_WEEK" in d and d["DEFAULT_START_OF_WEEK"] is not None:
        if str(d["DEFAULT_START_OF_WEEK"]).lower() not in _VALID_START_OF_WEEK:
            raise SettingValidationError(
                f"{d['DEFAULT_START_OF_WEEK']!r} is not a valid DEFAULT_START_OF_WEEK"
            )
    if "PARSERS" in d and d["PARSERS"] is not None:
        for p in d["PARSERS"]:
            if str(p).lower() not in _VALID_PARSERS:
                raise SettingValidationError(f"{p!r} is not a valid parser name")


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
