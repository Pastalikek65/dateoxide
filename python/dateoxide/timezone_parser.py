"""Drop-in ``dateparser.timezone_parser`` compatible submodule."""

from __future__ import annotations

import re
from datetime import datetime, timedelta, timezone

from ._dateoxide import pop_tz_offset_from_string

_OFFSET_RE = re.compile(r"^(?:(?:UTC|GMT)?[+-]\d{1,2}(?::?\d{2})?|Z|UTC|GMT)$", re.IGNORECASE)
_ABBR_RE = re.compile(r"^[A-Z]{2,5}$")


def get_local_tz_offset() -> timedelta:
    return _compute_local_tz_offset()


def _compute_local_tz_offset() -> timedelta:
    off = datetime.now().astimezone().utcoffset()
    return off if off is not None else timedelta(0)


local_tz_offset: timedelta = _compute_local_tz_offset()


def convert_to_local_tz(datetime_obj: datetime, datetime_tz_offset: timedelta) -> datetime:
    """Convert ``datetime_obj`` from ``datetime_tz_offset`` into the machine's local offset."""
    if datetime_obj.tzinfo is None:
        aware = datetime_obj.replace(tzinfo=timezone(datetime_tz_offset))
    else:
        aware = datetime_obj
    return aware.astimezone(timezone(local_tz_offset))


def word_is_tz(word: str) -> bool:
    """Return True if ``word`` looks like a standalone timezone token."""
    w = word.strip("(), ")
    if not w:
        return False
    if _OFFSET_RE.match(w) or _ABBR_RE.match(w):
        _, tz = pop_tz_offset_from_string(f"2026-01-01 00:00 {w}")
        return tz is not None
    if "/" in w:
        _, tz = pop_tz_offset_from_string(f"2026-01-01 00:00 {w}")
        return tz is not None
    return False


def is_timezone_token(token: str) -> bool:
    """Alias matching ``dateparser.timezone_parser.is_timezone_token``."""
    return word_is_tz(token)


__all__ = [
    "convert_to_local_tz",
    "get_local_tz_offset",
    "is_timezone_token",
    "local_tz_offset",
    "pop_tz_offset_from_string",
    "word_is_tz",
]
