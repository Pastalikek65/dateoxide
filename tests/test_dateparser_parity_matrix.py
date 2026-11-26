"""Comprehensive head-to-head parity matrix between dateoxide and dateparser across inputs, settings, timezones, and error behavior."""
from __future__ import annotations

from datetime import datetime, timezone
import pytest

import dateoxide
from dateoxide import Settings, SettingValidationError
from dateoxide.date import DateDataParser
from dateoxide.freshness_date_parser import FreshnessDateDataParser

dateparser = pytest.importorskip("dateparser")
from dateparser.conf import SettingValidationError as DpSettingValidationError
from dateparser.date import DateDataParser as DpDateDataParser

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


@pytest.mark.parametrize(
    "text,kwargs",
    [
        ("2026-11-26", {}),
        ("2026-11-26 15:30:45", {}),
        ("Thu, 26 Nov 2026 15:30:00 +0000", {}),
        ("05/11/2026", {"settings": {"DATE_ORDER": "DMY"}}),
        ("05/11/2026", {"settings": {"DATE_ORDER": "MDY"}}),
        ("2026/11/05", {"settings": {"DATE_ORDER": "YMD"}}),
        ("12 December 2025", {}),
        ("12 de diciembre de 2025", {"languages": ["es"]}),
        ("29 февраля 2024 года", {"languages": ["ru"]}),
        ("15 maja 2025", {"languages": ["pl"]}),
        ("3 days ago", {"settings": {"RELATIVE_BASE": FIXED_BASE}}),
        ("in 2 weeks", {"settings": {"RELATIVE_BASE": FIXED_BASE}}),
        ("yesterday", {"settings": {"RELATIVE_BASE": FIXED_BASE}}),
        ("tomorrow at 5pm", {"settings": {"RELATIVE_BASE": FIXED_BASE}}),
        ("1 year, 2 months ago", {"settings": {"RELATIVE_BASE": FIXED_BASE}}),
        ("December 2025", {"settings": {"RELATIVE_BASE": FIXED_BASE, "PREFER_DAY_OF_MONTH": "first"}}),
        ("December 2025", {"settings": {"RELATIVE_BASE": FIXED_BASE, "PREFER_DAY_OF_MONTH": "last"}}),
        ("2025", {"settings": {"RELATIVE_BASE": FIXED_BASE, "PREFER_MONTH_OF_YEAR": "first"}}),
        ("2025", {"settings": {"RELATIVE_BASE": FIXED_BASE, "PREFER_MONTH_OF_YEAR": "last"}}),
        ("09:00", {"settings": {"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "future"}}),
        ("18:00", {"settings": {"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "past"}}),
        ("15 December", {"settings": {"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "past"}}),
        ("15 January", {"settings": {"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "future"}}),
        ("circa 12 December 2025", {"settings": {"SKIP_TOKENS": ["circa"]}}),
        ("2024-366", {"date_formats": ["%Y-%j"]}),
        ("2025-366", {"date_formats": ["%Y-%j"]}),
        ("December 2025", {"settings": {"STRICT_PARSING": True}}),
        ("December 2025", {"settings": {"REQUIRE_PARTS": ["day"]}}),
        ("12 December 2025", {"settings": {"REQUIRE_PARTS": ["day", "month", "year"]}}),
        ("not a valid date at all", {}),
        ("", {}),
        ("    ", {}),
    ],
)
def test_parity_same_inputs_and_settings(text: str, kwargs: dict):
    ox = dateoxide.parse(text, **kwargs)
    dp = dateparser.parse(text, **kwargs)
    assert ox == dp, f"text={text!r}, kwargs={kwargs}: dateoxide={ox!r} vs dateparser={dp!r}"


@pytest.mark.parametrize(
    "text,cfg",
    [
        ("2026-07-15 12:00:00", {"TIMEZONE": "America/New_York", "TO_TIMEZONE": "Europe/Istanbul"}),
        ("2026-01-15 12:00:00", {"TIMEZONE": "America/New_York", "TO_TIMEZONE": "Europe/Istanbul"}),
        ("2026-11-26 12:00:00 UTC", {"TO_TIMEZONE": "Asia/Tokyo"}),
        ("2026-11-26 12:00:00", {"TIMEZONE": "UTC", "RETURN_AS_TIMEZONE_AWARE": True}),
        ("2026-11-26 12:00:00 +03:00", {"RETURN_AS_TIMEZONE_AWARE": False}),
    ],
)
def test_parity_timezones_and_dst(text: str, cfg: dict):
    ox = dateoxide.parse(text, settings=cfg)
    dp = dateparser.parse(text, settings=cfg)
    assert ox is not None and dp is not None
    assert ox.replace(tzinfo=None) == dp.replace(tzinfo=None)
    assert (ox.tzinfo is not None) == (dp.tzinfo is not None)
    if ox.tzinfo is not None and dp.tzinfo is not None:
        assert ox.utcoffset() == dp.utcoffset()


def test_parity_date_data_fields_and_return_time_as_period():
    cfg = {"RELATIVE_BASE": FIXED_BASE, "RETURN_TIME_AS_PERIOD": True}
    ox_dd = DateDataParser(settings=cfg).get_date_data("2026-11-26 14:30:00")
    dp_dd = DpDateDataParser(settings=cfg).get_date_data("2026-11-26 14:30:00")
    assert ox_dd.date_obj == dp_dd.date_obj
    assert ox_dd.period == dp_dd.period == "time"

    # Relative period compatibility
    for phrase, expected_period in [("yesterday", "day"), ("last week", "week"), ("last month", "month"), ("last year", "year")]:
        ox_r = DateDataParser(settings={"RELATIVE_BASE": FIXED_BASE}).get_date_data(phrase)
        dp_r = DpDateDataParser(settings={"RELATIVE_BASE": FIXED_BASE}).get_date_data(phrase)
        assert ox_r.period == dp_r.period == expected_period


def test_parity_freshness_parser():
    fdp = FreshnessDateDataParser()
    dd = fdp.get_date_data("3 days ago")
    assert dd.date_obj is not None
    assert dd.period == "day"
    assert fdp.get_date_data("12 December 2025").date_obj is None


def test_parity_error_and_validation_behavior():
    invalid_settings = [
        {"DATE_ORDER": "XYZ"},
        {"STRICT_DATE_ORDER": "bogus"},
        {"PREFER_DATES_FROM": "tomorrow"},
        {"PREFER_DAY_OF_MONTH": "middle"},
        {"PREFER_MONTH_OF_YEAR": "middle"},
        {"UNKNOWN_SETTING_KEY": 123},
    ]
    for bad in invalid_settings:
        with pytest.raises(SettingValidationError):
            Settings(settings=bad)
        with pytest.raises((DpSettingValidationError, Exception)):
            dateparser.parse("2026-11-26", settings=bad)

    # Non-string inputs must raise TypeError cleanly in both libraries (no segfault/panic)
    for non_str in (None, 12345, ["2026-11-26"], {"date": "2026-11-26"}):
        with pytest.raises(TypeError):
            dateoxide.parse(non_str)  # type: ignore[arg-type]
        with pytest.raises(TypeError):
            dateparser.parse(non_str)  # type: ignore[arg-type]


def test_parity_rfc3339_utc_equivalence():
    s = "2026-11-26T15:30:00Z"
    ox = dateoxide.parse(s)
    dp = dateparser.parse(s)
    assert ox is not None and dp is not None
    assert ox.astimezone(timezone.utc) == dp.astimezone(timezone.utc)
