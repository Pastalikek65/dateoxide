"""Comprehensive edge-case, regression, ISO week, NoSpacesTime, DST, and search_dates tests."""
from __future__ import annotations

from datetime import datetime, timezone
import pytest

import dateoxide
from dateoxide.date import DateDataParser
from dateoxide.search import search_dates

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


def test_iso_8601_week_dates():
    assert dateoxide.parse("2026-W48-4") == datetime(2026, 11, 26)
    assert dateoxide.parse("2026-W48") == datetime(2026, 11, 23)
    assert dateoxide.parse("2026W484") == datetime(2026, 11, 26)
    assert dateoxide.parse("2026W48") == datetime(2026, 11, 23)
    assert dateoxide.parse("2026-W48-4T15:30:00Z") == datetime(
        2026, 11, 26, 15, 30, 0, tzinfo=timezone.utc
    )
    assert dateoxide.parse("2024-W01-1") == datetime(2024, 1, 1)

    # Invalid week / weekday / strict mode on week-only
    assert dateoxide.parse("2026-W54-1") is None
    assert dateoxide.parse("2026-W48-8") is None
    assert dateoxide.parse("2026-W48", settings={"STRICT_PARSING": True}) is None

    dd = DateDataParser().get_date_data("2026-W48")
    assert dd.period == "week"
    assert dd.date_obj == datetime(2026, 11, 23)


def test_no_spaces_time_and_compact_dates():
    assert dateoxide.parse(
        "1430",
        settings={"RELATIVE_BASE": FIXED_BASE, "PARSERS": ["no-spaces-time"]},
    ) == datetime(2026, 11, 26, 14, 30)
    assert dateoxide.parse(
        "143025",
        settings={"RELATIVE_BASE": FIXED_BASE, "PARSERS": ["no-spaces-time"]},
    ) == datetime(2026, 11, 26, 14, 30, 25)
    assert dateoxide.parse(
        "0915",
        settings={
            "RELATIVE_BASE": FIXED_BASE,
            "PARSERS": ["no-spaces-time"],
            "PREFER_DATES_FROM": "future",
        },
    ) == datetime(2026, 11, 27, 9, 15)

    assert dateoxide.parse(
        "26112026",
        settings={"DATE_ORDER": "DMY", "PARSERS": ["no-spaces-time"]},
    ) == datetime(2026, 11, 26)
    assert dateoxide.parse(
        "11262026",
        settings={"DATE_ORDER": "MDY", "PARSERS": ["no-spaces-time"]},
    ) == datetime(2026, 11, 26)
    assert (
        dateoxide.parse(
            "26112026",
            settings={
                "DATE_ORDER": "MDY",
                "STRICT_DATE_ORDER": "all",
                "PARSERS": ["no-spaces-time"],
            },
        )
        is None
    )


def test_time_only_prefer_dates_from_past_and_future():
    # Base is 2026-11-26 15:00:00
    assert dateoxide.parse(
        "09:00",
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "future"},
    ) == datetime(2026, 11, 27, 9, 0)
    assert dateoxide.parse(
        "09:00",
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "past"},
    ) == datetime(2026, 11, 26, 9, 0)
    assert dateoxide.parse(
        "18:00",
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "past"},
    ) == datetime(2026, 11, 25, 18, 0)
    assert dateoxide.parse(
        "18:00",
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "future"},
    ) == datetime(2026, 11, 26, 18, 0)
    assert dateoxide.parse(
        "12am", settings={"RELATIVE_BASE": FIXED_BASE}
    ) == datetime(2026, 11, 26, 0, 0)
    assert dateoxide.parse(
        "12pm", settings={"RELATIVE_BASE": FIXED_BASE}
    ) == datetime(2026, 11, 26, 12, 0)


def test_compound_multilingual_relative_expressions():
    cfg = {"RELATIVE_BASE": FIXED_BASE}
    assert dateoxide.parse("1 year, 2 months ago", settings=cfg) == datetime(2025, 9, 26, 15, 0)
    assert dateoxide.parse("2 days and 3 hours ago", settings=cfg) == datetime(2026, 11, 24, 12, 0)
    assert dateoxide.parse("1 hour 30 minutes ago", settings=cfg) == datetime(2026, 11, 26, 13, 30)
    assert dateoxide.parse("in 2 weeks and 3 days", settings=cfg) == datetime(2026, 12, 13, 15, 0)
    assert dateoxide.parse("1 yıl 2 ay önce", settings=cfg) == datetime(2025, 9, 26, 15, 0)
    assert dateoxide.parse("2 saat 30 dakika sonra", settings=cfg) == datetime(2026, 11, 26, 17, 30)
    assert dateoxide.parse("vor 1 Jahr und 2 Monaten", settings=cfg) == datetime(2025, 9, 26, 15, 0)
    assert dateoxide.parse("il y a 1 an et 2 mois", settings=cfg) == datetime(2025, 9, 26, 15, 0)
    assert dateoxide.parse("half an hour ago", settings=cfg) == datetime(2026, 11, 26, 14, 30)


def test_search_dates_no_false_positives_on_english_prose_and_versions():
    cfg = {"RELATIVE_BASE": FIXED_BASE}

    found_meet = search_dates("Let us meet tomorrow at 5pm or next Friday.", settings=cfg)
    assert found_meet is not None
    texts = [t for t, _ in found_meet]
    assert "Let" not in texts
    assert len(found_meet) == 2
    assert found_meet[0][1] == datetime(2026, 11, 27, 17, 0)
    assert found_meet[1][1].date() == datetime(2026, 11, 27).date()

    # Common English words that collide with minor CLDR locales must not false-match
    assert search_dates("She can see her man, did you say?", settings=cfg) is None
    assert search_dates("Python 3.12.1 on 192.168.1.1 costs $12.99 (100% ok)", settings=cfg) is None

    # Bracketed log timestamps
    logs = "[2026-11-26 15:30:00] INFO started, next check 2026-11-27 09:00:00"
    found_logs = search_dates(logs, settings=cfg)
    assert found_logs is not None
    assert len(found_logs) == 2
    assert found_logs[0][1] == datetime(2026, 11, 26, 15, 30)
    assert found_logs[1][1] == datetime(2026, 11, 27, 9, 0)


def test_dst_and_timezone_conversions_and_abbrevs():
    dt_ny = dateoxide.parse(
        "2026-07-15 12:00:00",
        settings={"TIMEZONE": "America/New_York", "TO_TIMEZONE": "Europe/Istanbul"},
    )
    assert dt_ny is not None
    assert dt_ny.hour == 19  # EDT (-04:00) -> TRT (+03:00) = +7h

    dt_custom = dateoxide.parse(
        "2026-11-26 12:00:00 TRT",
        settings={"TIMEZONE_ABBREVIATIONS": {"TRT": "+03:00"}, "TO_TIMEZONE": "UTC"},
    )
    assert dt_custom is not None
    assert dt_custom.hour == 9


def test_boundary_and_setting_combinations():
    assert dateoxide.parse("0001-01-01") == datetime(1, 1, 1)
    assert dateoxide.parse("9999-12-31") == datetime(9999, 12, 31)
    assert dateoxide.parse("0000-01-01") is None
    assert dateoxide.parse("") is None
    assert dateoxide.parse("   \t\n  ") is None

    # REQUIRE_PARTS
    assert dateoxide.parse("December 2025", settings={"REQUIRE_PARTS": ["day"]}) is None
    assert dateoxide.parse("12 December 2025", settings={"REQUIRE_PARTS": ["day"]}) == datetime(
        2025, 12, 12
    )

    # RETURN_TIME_AS_PERIOD
    dd = DateDataParser(settings={"RETURN_TIME_AS_PERIOD": True}).get_date_data("2026-11-26 15:30")
    assert dd.period == "time"

    # SKIP_TOKENS
    assert dateoxide.parse(
        "circa 12 December 2025",
        settings={"SKIP_TOKENS": ["circa"]},
    ) == datetime(2025, 12, 12)


def test_conversational_fractional_and_search_improvements():
    cfg = {"RELATIVE_BASE": FIXED_BASE}
    assert dateoxide.parse("the day after tomorrow", settings=cfg) == datetime(2026, 11, 28, 15, 0)
    assert dateoxide.parse("the day before yesterday", settings=cfg) == datetime(2026, 11, 24, 15, 0)
    assert dateoxide.parse("two and a half hours ago", settings=cfg) == datetime(2026, 11, 26, 12, 30)
    assert dateoxide.parse("bir buçuk saat önce", settings=cfg) == datetime(2026, 11, 26, 13, 30)
    assert dateoxide.parse("in 2 and a half days", settings=cfg) == datetime(2026, 11, 29, 3, 0)
    assert dateoxide.parse("vor eineinhalb Stunden", settings=cfg) == datetime(2026, 11, 26, 13, 30)

    # Conversational clock phrases
    assert dateoxide.parse("quarter past 3", settings=cfg) == datetime(2026, 11, 26, 3, 15)
    assert dateoxide.parse("quarter to 4", settings=cfg) == datetime(2026, 11, 26, 3, 45)
    assert dateoxide.parse("half past 5", settings=cfg) == datetime(2026, 11, 26, 5, 30)
    assert dateoxide.parse("saat 3 buçuk", settings=cfg) == datetime(2026, 11, 26, 3, 30)
    assert dateoxide.parse("3'ü çeyrek geçe", settings=cfg) == datetime(2026, 11, 26, 3, 15)

    # Weekday-only aligns to 00:00:00 like dateparser
    assert dateoxide.parse("last Friday", settings=cfg) == datetime(2026, 11, 20, 0, 0)
    assert dateoxide.parse("geçen pazartesi", settings=cfg) == datetime(2026, 11, 23, 0, 0)

    # Search fixes: 'or on 12 December 2025' and 'Uhr' attachment
    res = search_dates("Let us meet tomorrow at 5pm or on 12 December 2025.", settings=cfg)
    assert res is not None and len(res) == 2
    assert res[0][1] == datetime(2026, 11, 27, 17, 0)
    assert res[1][1] == datetime(2025, 12, 12, 0, 0)

    res_de = search_dates("Morgen um 09:00 Uhr Rechnung bezahlen", settings=cfg)
    assert res_de is not None and len(res_de) == 1
    assert "09:00 Uhr" in res_de[0][0]
    assert res_de[0][1] == datetime(2026, 11, 27, 9, 0)
