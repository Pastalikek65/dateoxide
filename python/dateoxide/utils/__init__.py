"""Utility helpers and submodules matching ``dateparser.utils``."""

from __future__ import annotations

import calendar
import re
import unicodedata
from datetime import datetime, timedelta, timezone, tzinfo
from typing import Any, Literal
from zoneinfo import ZoneInfo

from . import time_spans

_BRACES_RE = re.compile(r"\{[^}]*\}|\([^)]*\)|\[[^\]]*\]")
_OFFSET_RE = re.compile(r"^(?:UTC|GMT)?([+-])(\d{1,2})(?::?(\d{2}))?$", re.IGNORECASE)


def strip_braces(date_string: str) -> str:
    """Strip parenthesized/bracketed/braced annotations from a date string."""
    return _BRACES_RE.sub("", date_string).strip()


def normalize_unicode(
    string: str,
    form: Literal["NFC", "NFD", "NFKC", "NFKD"] = "NFKD",
) -> str:
    """Normalize a Unicode string (stripping combining marks under NFKD/NFD)."""
    normalized = unicodedata.normalize(form, string)
    return "".join(ch for ch in normalized if not unicodedata.combining(ch))


def find_date_separator(format: str) -> str | None:
    """Return the first common date separator found in a ``strptime`` format string."""
    for sep in ("/", "-", ".", " "):
        if sep in format:
            return sep
    return None


def get_last_day_of_month(year: int, month: int) -> int:
    """Return the last day number (28..31) for the given ``year`` and ``month``."""
    return calendar.monthrange(year, month)[1]


def get_previous_leap_year(year: int) -> int:
    """Return the greatest leap year strictly before ``year``."""
    y = year - 1
    while not calendar.isleap(y):
        y -= 1
    return y


def get_next_leap_year(year: int) -> int:
    """Return the smallest leap year strictly after ``year``."""
    y = year + 1
    while not calendar.isleap(y):
        y += 1
    return y


def _resolve_tz(tz_string: str) -> tzinfo:
    s = tz_string.strip()
    if s.upper() in ("UTC", "GMT", "Z"):
        return timezone.utc
    if s.lower() == "local":
        return datetime.now().astimezone().tzinfo or timezone.utc
    m = _OFFSET_RE.match(s)
    if m:
        sign = 1 if m.group(1) == "+" else -1
        hrs = int(m.group(2))
        mins = int(m.group(3) or "0")
        return timezone(sign * timedelta(hours=hrs, minutes=mins))
    return ZoneInfo(s)


def apply_timezone(date_time: datetime, tz_string: str) -> datetime:
    """Convert ``date_time`` (assumed UTC if naive) into ``tz_string``."""
    tz = _resolve_tz(tz_string)
    if date_time.tzinfo is None:
        date_time = date_time.replace(tzinfo=timezone.utc)
    return date_time.astimezone(tz)


def localize_timezone(date_time: datetime, tz_string: str) -> datetime:
    """Attach ``tz_string`` to a naive ``datetime`` (or convert if already aware)."""
    tz = _resolve_tz(tz_string)
    if date_time.tzinfo is None:
        return date_time.replace(tzinfo=tz)
    return date_time.astimezone(tz)


def apply_timezone_from_settings(date_obj: datetime, settings: Any | None) -> datetime:
    """Apply ``TO_TIMEZONE`` / ``RETURN_AS_TIMEZONE_AWARE`` from ``settings`` onto ``date_obj``."""
    if settings is None:
        return date_obj
    getter = settings.get if isinstance(settings, dict) else lambda k, d=None: getattr(settings, k, d)
    to_tz = getter("TO_TIMEZONE", False)
    if isinstance(to_tz, str) and to_tz:
        date_obj = apply_timezone(date_obj, to_tz)
    ret_aware = getter("RETURN_AS_TIMEZONE_AWARE", "default")
    if ret_aware is True and date_obj.tzinfo is None:
        src_tz = getter("TIMEZONE", "local")
        date_obj = localize_timezone(date_obj, str(src_tz or "local"))
    elif ret_aware is False and date_obj.tzinfo is not None:
        date_obj = date_obj.replace(tzinfo=None)
    return date_obj


def strptime(date_string: str, format: str) -> datetime:
    """Compatibility wrapper around ``datetime.strptime``."""
    return datetime.strptime(date_string, format)


__all__ = [
    "apply_timezone",
    "apply_timezone_from_settings",
    "find_date_separator",
    "get_last_day_of_month",
    "get_next_leap_year",
    "get_previous_leap_year",
    "localize_timezone",
    "normalize_unicode",
    "strip_braces",
    "strptime",
    "time_spans",
]
