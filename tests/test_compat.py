from __future__ import annotations

from datetime import datetime, timezone
import pytest
import dateparser
import dateoxide
from dateoxide.date import DateDataParser
from dateoxide.search import search_dates

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


@pytest.mark.parametrize(
    "text,expected_date",
    [
        ("2026-11-26", (2026, 11, 26)),
        ("2026-11-26T15:30:00Z", (2026, 11, 26)),
        ("2026-11-26 09:45:12+03:00", (2026, 11, 26)),
        ("Thu, 26 Nov 2026 15:30:00 +0300", (2026, 11, 26)),
        ("26.11.2026", (2026, 11, 26)),
        ("2026/11/26 08:15", (2026, 11, 26)),
        ("20261126", (2026, 11, 26)),
        ("12 December 2025", (2025, 12, 12)),
        ("December 12, 2025 3:45 PM", (2025, 12, 12)),
        ("15th of May 2025", (2025, 5, 15)),
        ("Fri Nov 26 15:30:00 2026", (2026, 11, 26)),
    ],
)
def test_absolute_dates_match_expected(text: str, expected_date: tuple[int, int, int]):
    dt = dateoxide.parse(text, settings={"RELATIVE_BASE": FIXED_BASE})
    assert dt is not None, f"Failed to parse: {text}"
    assert (dt.year, dt.month, dt.day) == expected_date


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
    ],
)
def test_cross_validation_against_dateparser(text: str):
    settings = {"RELATIVE_BASE": FIXED_BASE}
    ref = dateparser.parse(text, settings=settings)
    got = dateoxide.parse(text, settings=settings)
    assert ref is not None
    assert got is not None, f"dateoxide returned None for '{text}' (dateparser returned {ref})"
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
        "." * 2000,  # ReDoS stress check
        "a" * 5000,
    ]:
        assert dateoxide.parse(bad) is None
