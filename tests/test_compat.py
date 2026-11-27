from __future__ import annotations

from datetime import datetime, timezone
import pytest
import dateparser
import dateoxide
from dateoxide.date import DateDataParser
from dateoxide.search import search_dates

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)
_DP_VER = tuple(int(p) for p in getattr(dateparser, "__version__", "0.0").split(".")[:2])


@pytest.mark.parametrize(
    "text,expected_date",
    [
        ("2026-11-26", (2026, 11, 26)),
        ("2026-11-26T15:30:00Z", (2026, 11, 26)),
        ("20261126T153000Z", (2026, 11, 26)),
        ("2026-11-26 09:45:12+03:00", (2026, 11, 26)),
        ("Thu, 26 Nov 2026 15:30:00 +0300", (2026, 11, 26)),
        ("26.11.2026", (2026, 11, 26)),
        ("26-Nov-2026", (2026, 11, 26)),
        ("26/Nov/2026:15:30:00 +0300", (2026, 11, 26)),
        ("2026/11/26 08:15", (2026, 11, 26)),
        ("20261126", (2026, 11, 26)),
        ("12 December 2025", (2025, 12, 12)),
        ("December 12, 2025 3:45 PM", (2025, 12, 12)),
        ("Nov 26, 2026 at 3:30 PM EST", (2026, 11, 26)),
        ("15th of May 2025", (2025, 5, 15)),
        ("Fri Nov 26 15:30:00 2026", (2026, 11, 26)),
    ],
)
def test_absolute_dates_match_expected(text: str, expected_date: tuple[int, int, int]):
    dt = dateoxide.parse(text, settings={"RELATIVE_BASE": FIXED_BASE})
    assert dt is not None, f"Failed to parse: {text}"
    assert (dt.year, dt.month, dt.day) == expected_date


@pytest.mark.parametrize(
    "text,expected_hms",
    [
        ("5:30p.m.", (17, 30, 0)),
        ("09h15", (9, 15, 0)),
        ("yesterday at noon", (12, 0, 0)),
        ("tomorrow at midnight", (0, 0, 0)),
        ("tonight", (20, 0, 0)),
        ("last night", (20, 0, 0)),
        ("today morning", (9, 0, 0)),
        ("this evening", (18, 0, 0)),
        ("dün gece", (20, 0, 0)),
        ("bu akşam", (18, 0, 0)),
    ],
)
def test_compact_times_and_time_of_day(text: str, expected_hms: tuple[int, int, int]):
    dt = dateoxide.parse(text, settings={"RELATIVE_BASE": FIXED_BASE})
    assert dt is not None, f"Failed to parse: {text}"
    assert (dt.hour, dt.minute, dt.second) == expected_hms


@pytest.mark.parametrize(
    "text",
    [
        "12 December 2025",
        "3 days ago",
        "in 2 weeks",
        "yesterday",
        "tomorrow",
        "3 gün önce",
        "yarın",
        "vor 2 Wochen",
        "12 de diciembre de 2025",
        "le 14 juillet 1789",
        "26. November 2026",
        "26 ноября 2026",
        "2026年11月26日",
        "2026년 11월 26일",
        "غدا",
        "١٢ ديسمبر ٢٠٢٥",
        "कल",
    ],
)
def test_cross_validation_against_dateparser(text: str):
    settings = {"RELATIVE_BASE": FIXED_BASE}
    ref = dateparser.parse(text, settings=settings)
    got = dateoxide.parse(text, settings=settings)
    assert got is not None, f"dateoxide returned None for '{text}' (dateparser returned {ref})"
    if ref is None and _DP_VER < (1, 3):
        # dateparser < 1.3 (Python 3.9) fails to parse Korean "2026년 11월 26일"
        assert (got.year, got.month, got.day) == (2026, 11, 26)
        return
    assert ref is not None
    assert (got.year, got.month, got.day) == (ref.year, ref.month, ref.day), (
        f"Mismatch for '{text}': dateoxide={got} vs dateparser={ref}"
    )


def test_settings_and_date_order():
    d_dmy = dateoxide.parse("05/11/2026", settings={"DATE_ORDER": "DMY"})
    d_mdy = dateoxide.parse("05/11/2026", settings={"DATE_ORDER": "MDY"})
    assert d_dmy == datetime(2026, 11, 5)
    assert d_mdy == datetime(2026, 5, 11)

    d_first = dateoxide.parse(
        "December 2025",
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DAY_OF_MONTH": "first"},
    )
    d_last = dateoxide.parse(
        "February 2024",
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DAY_OF_MONTH": "last"},
    )
    assert d_first == datetime(2025, 12, 1)
    assert d_last == datetime(2024, 2, 29)

    assert dateoxide.parse("December 2025", settings={"STRICT_PARSING": True}) is None
    assert (
        dateoxide.parse(
            "12 December",
            settings={"RELATIVE_BASE": FIXED_BASE, "REQUIRE_PARTS": ["year", "month", "day"]},
        )
        is None
    )


def test_advanced_settings_region_normalize_parsers_skip_and_period():
    # region controls ambiguous numeric dates
    assert dateoxide.parse("05-11-2026", region="US") == datetime(2026, 5, 11)
    assert dateoxide.parse("05-11-2026", region="GB") == datetime(2026, 11, 5)

    # NORMALIZE=False requires exact diacritics
    assert dateoxide.parse("15 Subat 2026", settings={"NORMALIZE": True}) == datetime(2026, 2, 15)
    assert dateoxide.parse("15 Subat 2026", settings={"NORMALIZE": False}) is None

    # PARSERS filtering
    assert (
        dateoxide.parse(
            "3 days ago",
            settings={"RELATIVE_BASE": FIXED_BASE, "PARSERS": ["absolute-time", "base-formats"]},
        )
        is None
    )
    assert (
        dateoxide.parse(
            "1700000000",
            settings={"PARSERS": ["absolute-time"]},
        )
        is None
    )

    # SKIP_TOKENS
    dt_skip = dateoxide.parse(
        "Published: 12 December 2025",
        settings={"SKIP_TOKENS": ["Published:"]},
    )
    assert dt_skip == datetime(2025, 12, 12)

    # RETURN_TIME_AS_PERIOD
    ddp = DateDataParser(settings={"RETURN_TIME_AS_PERIOD": True})
    assert ddp.get_date_data("2026-11-26 14:30").period == "time"
    assert ddp.get_date_data("2026-11-26").period == "day"

    # Multi-sense relative word (Hindi कल: tomorrow by default, yesterday with PREFER_DATES_FROM='past')
    assert dateoxide.parse(
        "कल",
        languages=["hi"],
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "past"},
    ).date() == datetime(2026, 11, 25).date()
    assert dateoxide.parse(
        "कल",
        languages=["hi"],
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "future"},
    ).date() == datetime(2026, 11, 27).date()


def test_timezone_handling():
    dt_utc = dateoxide.parse("2026-11-26T12:00:00Z")
    assert dt_utc is not None and dt_utc.tzinfo is not None

    dt_ist = dateoxide.parse(
        "2026-11-26T12:00:00Z",
        settings={"TO_TIMEZONE": "Europe/Istanbul", "RETURN_AS_TIMEZONE_AWARE": True},
    )
    assert dt_ist is not None
    assert dt_ist.hour == 15
    assert dt_ist.tzinfo is not None

    dt_naive = dateoxide.parse(
        "2026-11-26T12:00:00Z",
        settings={"TO_TIMEZONE": "Europe/Istanbul", "RETURN_AS_TIMEZONE_AWARE": False},
    )
    assert dt_naive == datetime(2026, 11, 26, 15, 0, 0)
    assert dt_naive.tzinfo is None


def test_custom_date_formats():
    dt = dateoxide.parse("26--11--2026", date_formats=["%d--%m--%Y"])
    assert dt == datetime(2026, 11, 26)


def test_unix_timestamps():
    dt = dateoxide.parse("1700000000")
    assert dt is not None
    assert dt.astimezone(timezone.utc) == datetime(2023, 11, 14, 22, 13, 20, tzinfo=timezone.utc)


def test_date_data_parser_and_dict_access():
    ddp = DateDataParser(languages=["tr", "en"], settings={"RELATIVE_BASE": FIXED_BASE})
    dd = ddp.get_date_data("12 Aralık 2025")
    assert dd.date_obj == datetime(2025, 12, 12)
    assert dd["date_obj"] == datetime(2025, 12, 12)
    assert dd.period == "day"
    assert dd["locale"] == "tr"
    assert dd.get("missing", "fallback") == "fallback"


def test_invalid_inputs_return_none():
    for bad in [
        "",
        "   ",
        "not a date at all xyz",
        "lorem ipsum dolor sit amet",
        "." * 2000,
        "a" * 5000,
    ]:
        assert dateoxide.parse(bad) is None
