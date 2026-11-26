"""Regression tests targeting known scrapinghub/dateparser bugs, edge cases,
ReDoS linear-time guarantees, and full 3,792-date Umm al-Qura Hijri verification.
"""
from __future__ import annotations

import time
from datetime import datetime, timezone
import pytest

import dateoxide
from dateoxide.calendars import HijriCalendar
from dateoxide.search import search_dates

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


def test_bug_compact_iso_basic_that_dateparser_misparses():
    # dateparser 1.5.1 misparses '20261126T153000Z' as year 1608; dateoxide parses it accurately
    dt = dateoxide.parse("20261126T153000Z")
    assert dt == datetime(2026, 11, 26, 15, 30, 0, tzinfo=timezone.utc)


def test_bug_redos_linear_time_on_pathological_inputs():
    # dateparser GHSA-rx9v-94ff-9cc2 / #426: long punctuation & non-date runs
    pathological = [
        "." * 5000,
        "-" * 5000,
        "،" * 5000,
        ":" * 5000,
        " " * 5000,
        "not a date " * 500,
        "(" * 2000 + "hello" + ")" * 2000,
    ]
    t0 = time.perf_counter()
    for s in pathological:
        assert dateoxide.parse(s) is None
    elapsed_ms = (time.perf_counter() - t0) * 1000
    assert elapsed_ms < 100.0, f"Pathological inputs took too long: {elapsed_ms:.1f}ms"


def test_bug_457_and_1140_repeated_calls_no_slowdown():
    # dateparser #457 / #1140: repeated calls degrading due to cache / previous_locales growth
    samples = [
        "12 December 2025",
        "3 gün önce",
        "vor 2 Wochen",
        "hier à 14h30",
        "2026年11月26日",
        "not a date at all",
    ]
    t0 = time.perf_counter()
    for i in range(1000):
        for s in samples:
            dateoxide.parse(s, settings={"RELATIVE_BASE": FIXED_BASE})
    elapsed_ms = (time.perf_counter() - t0) * 1000
    assert elapsed_ms < 500.0, f"6000 calls took {elapsed_ms:.1f}ms"


def test_bug_968_search_dates_keeps_timestamp_intact():
    # dateparser #968: search_dates splitting '2026-11-26 15:30:00 UTC' into separate date & time
    text = "Server rebooted at 2026-11-26 15:30:00 UTC successfully."
    found = search_dates(text)
    assert found is not None
    assert len(found) == 1, f"Expected 1 combined match, got: {found}"
    assert found[0][1].year == 2026
    assert found[0][1].month == 11
    assert found[0][1].day == 26
    assert found[0][1].hour == 15
    assert found[0][1].minute == 30


def test_bug_turkish_uppercase_dotted_dotless_i():
    settings = {"RELATIVE_BASE": FIXED_BASE}
    assert dateoxide.parse("15 OCAK 2026", settings=settings) == datetime(2026, 1, 15)
    assert dateoxide.parse("15 EKİM 2025", settings=settings) == datetime(2025, 10, 15)
    assert dateoxide.parse("3 GÜN ÖNCE", settings=settings) == datetime(2026, 11, 23, 15, 0)
    assert dateoxide.parse("DÜN SAAT 14:30", settings=settings) == datetime(2026, 11, 25, 14, 30)
    assert dateoxide.parse("YARIN", settings=settings) == datetime(2026, 11, 27, 15, 0)
    assert dateoxide.parse("İSTANBUL", settings=settings) is None


def test_bug_leap_year_month_end_and_day_of_year_bounds():
    assert dateoxide.parse("2024-02-29") == datetime(2024, 2, 29)
    assert dateoxide.parse("2025-02-29") is None

    # Month-end clamping: 1 month ago from March 31 -> Feb 28 (non-leap) or Feb 29 (leap)
    assert dateoxide.parse(
        "1 month ago", settings={"RELATIVE_BASE": datetime(2026, 3, 31, 10, 0)}
    ) == datetime(2026, 2, 28, 10, 0)
    assert dateoxide.parse(
        "1 year ago", settings={"RELATIVE_BASE": datetime(2024, 2, 29, 10, 0)}
    ) == datetime(2023, 2, 28, 10, 0)

    # %Y-%j day-of-year bounds (dateparser 1.4.2 fix)
    assert dateoxide.parse("2025-366", date_formats=["%Y-%j"]) is None
    assert dateoxide.parse("2024-366", date_formats=["%Y-%j"]) == datetime(2024, 12, 31)


def test_bug_extreme_overflow_and_malformed_inputs_never_crash():
    malformed = [
        "+999999999999999 days ago",
        "5000 years ago",
        "123456789 hour",
        "1234567 days",
        "99/99/9999",
        "2026-13-40",
        "25:99:00",
        "12\x00December 2025",
    ]
    for s in malformed:
        assert dateoxide.parse(s, settings={"RELATIVE_BASE": FIXED_BASE}) is None


def test_official_umm_al_qura_hijri_full_table_3792_dates():
    hijridate = pytest.importorskip("hijridate")
    from hijridate import Hijri

    # Verify first day AND last day of all 1,896 Umm al-Qura months (1343 AH .. 1500 AH)
    for hy in range(1343, 1501):
        for hm in range(1, 13):
            # Day 1
            ref_first = Hijri(hy, hm, 1).to_gregorian()
            got_first = HijriCalendar(f"{hy:04d}/{hm:02d}/01").get_date()
            assert got_first is not None and got_first.date_obj is not None
            assert got_first.date_obj.date() == ref_first, (
                f"Hijri {hy}-{hm:02d}-01 mismatch: {got_first.date_obj.date()} vs {ref_first}"
            )

            # Last day of month (29 or 30)
            month_len = Hijri(hy, hm, 1).month_length()
            ref_last = Hijri(hy, hm, month_len).to_gregorian()
            got_last = HijriCalendar(f"{hy:04d}/{hm:02d}/{month_len:02d}").get_date()
            assert got_last is not None and got_last.date_obj is not None
            assert got_last.date_obj.date() == ref_last, (
                f"Hijri {hy}-{hm:02d}-{month_len:02d} mismatch"
            )
            # Day 30 must be rejected cleanly on 29-day months
            if month_len == 29:
                assert HijriCalendar(f"{hy:04d}/{hm:02d}/30").get_date() is None
