"""Drop-in ``dateparser.timezone_parser`` compatible submodule."""

from __future__ import annotations

from datetime import datetime, tzinfo, timezone

from ._dateoxide import pop_tz_offset_from_string


def get_local_tz_offset() -> tzinfo:
    return datetime.now().astimezone().tzinfo or timezone.utc


__all__ = ["pop_tz_offset_from_string", "get_local_tz_offset"]
