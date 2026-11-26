from __future__ import annotations

from datetime import datetime, time, timedelta
import pytest

import dateoxide
from dateoxide import PartOfDay, PartsOfDay, Settings, SettingValidationError, apply_settings
from dateoxide.calendars import (
    FrenchRepublicanCalendar,
    HijriCalendar,
    JalaliCalendar,
    MinguoCalendar,
)
from dateoxide.date import DateDataParser
from dateoxide.freshness_date_parser import FreshnessDateDataParser, freshness_date_parser
from dateoxide.search import search_dates
from dateoxide.timezone_parser import pop_tz_offset_from_string
from dateoxide.utils import time_spans

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


def test_settings_validation_and_apply_settings():
    s = Settings(settings={"DATE_ORDER": "DMY", "STRICT_PARSING": True})
    assert s.DATE_ORDER == "DMY"
    s2 = s.replace(DATE_ORDER="MDY")
    assert s2.DATE_ORDER == "MDY"
    assert s.DATE_ORDER == "DMY"

    with pytest.raises(SettingValidationError):
        Settings(DATE_ORDER="INVALID")

    with pytest.raises(SettingValidationError):
        Settings(STRICT_DATE_ORDER="numeric")

    with pytest.raises(SettingValidationError):
        Settings(UNKNOWN_KEY=123)

    @apply_settings
    def helper(text: str, settings: Settings | None = None) -> datetime | None:
        return dateoxide.parse(text, settings=settings)

    assert helper("05/11/2026", settings={"DATE_ORDER": "DMY"}) == datetime(2026, 11, 5)
    assert helper("05/11/2026", settings={"DATE_ORDER": "MDY"}) == datetime(2026, 5, 11)


def test_datedata_parts_part_of_day_and_custom_parts_of_day():
    pod_cfg = PartsOfDay(morning=time(8, 30), evening=time(19, 15))
    ddp = DateDataParser(settings={"RELATIVE_BASE": FIXED_BASE, "PARTS_OF_DAY": pod_cfg})

    dd = ddp.get_date_data("yesterday morning")
    assert dd.date_obj == datetime(2026, 11, 25, 8, 30)
    assert dd.period == "day"
    assert dd.parts == ("year", "month", "day", "time")
    assert dd.part_of_day == PartOfDay.MORNING
    assert dd["part_of_day"] == PartOfDay.MORNING
    assert dd["parts"] == ("year", "month", "day", "time")

    dd_ev = ddp.get_date_data("this evening")
    assert dd_ev.date_obj == datetime(2026, 11, 26, 19, 15)
    assert dd_ev.part_of_day == PartOfDay.EVENING

    dd_abs = ddp.get_date_data("12 December 2025 morning")
    assert dd_abs.date_obj == datetime(2025, 12, 12, 8, 30)
    assert dd_abs.part_of_day == PartOfDay.MORNING

    tup = ddp.get_date_tuple("12 December 2025")
    assert tup == (datetime(2025, 12, 12, 0, 0), "day", "en")


def test_strict_date_order_and_ignore_surrounding_text():
    assert (
        dateoxide.parse(
            "26/11/2026",
            settings={"DATE_ORDER": "MDY", "STRICT_DATE_ORDER": "none"},
        )
        == datetime(2026, 11, 26)
    )
    assert (
        dateoxide.parse(
            "26/11/2026",
            settings={"DATE_ORDER": "MDY", "STRICT_DATE_ORDER": "all"},
        )
        is None
    )
    assert (
        dateoxide.parse(
            "2026/11/26",
            settings={"DATE_ORDER": "MDY", "STRICT_DATE_ORDER": "all"},
        )
        == datetime(2026, 11, 26)
    )

    assert dateoxide.parse("Published: 12 December 2025") is None
    assert (
        dateoxide.parse(
            "Published: 12 December 2025",
            settings={"IGNORE_SURROUNDING_TEXT": True},
        )
        == datetime(2025, 12, 12)
    )
    assert (
        dateoxide.parse(
            "Updated on 2026-11-26 by Admin",
            settings={"IGNORE_SURROUNDING_TEXT": True},
        )
        == datetime(2026, 11, 26)
    )


def test_bare_relative_durations_and_week_period():
    assert dateoxide.parse("2 days", settings={"RELATIVE_BASE": FIXED_BASE}) == datetime(
        2026, 11, 24, 15, 0
    )
    assert dateoxide.parse(
        "2 days",
        settings={"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "future"},
    ) == datetime(2026, 11, 28, 15, 0)
    assert dateoxide.parse("3 hours", settings={"RELATIVE_BASE": FIXED_BASE}) == datetime(
        2026, 11, 26, 12, 0
    )
    assert dateoxide.parse("2 gün", settings={"RELATIVE_BASE": FIXED_BASE}) == datetime(
        2026, 11, 24, 15, 0
    )

    dd = DateDataParser(settings={"RELATIVE_BASE": FIXED_BASE}).get_date_data("last week")
    assert dd.period == "week"


def test_time_spans_and_search_add_period():
    span = time_spans.detect_time_span("Show errors from past 3 days please")
    assert span is not None
    assert span["type"] == "days"
    assert span["direction"] == "past"
    assert span["number"] == 3
    start, end = time_spans.generate_time_span(span, base_date=FIXED_BASE)
    assert start == datetime(2026, 11, 23, 15, 0)
    assert end == FIXED_BASE

    span_week = time_spans.detect_time_span("last week")
    assert span_week is not None
    s_mon, e_mon = time_spans.generate_time_span(
        span_week,
        base_date=FIXED_BASE,
        settings={"DEFAULT_START_OF_WEEK": "monday"},
    )
    s_sun, e_sun = time_spans.generate_time_span(
        span_week,
        base_date=FIXED_BASE,
        settings={"DEFAULT_START_OF_WEEK": "sunday"},
    )
    assert s_mon == datetime(2026, 11, 16, 15, 0)
    assert e_mon == datetime(2026, 11, 22, 15, 0)
    assert s_sun == datetime(2026, 11, 15, 15, 0)
    assert e_sun == datetime(2026, 11, 21, 15, 0)

    found_span = search_dates(
        "Show logs from last week",
        settings={"RELATIVE_BASE": FIXED_BASE, "RETURN_TIME_SPAN": True},
    )
    assert found_span is not None and len(found_span) == 2

    found_full = search_dates(
        "Event in December 2025; follow-up 12 Jan 2026",
        add_period=True,
        add_detected_language=True,
    )
    assert found_full is not None and len(found_full) == 2
    assert len(found_full[0]) == 4
    assert found_full[0][2] == "month"
    assert found_full[0][3] == "en"
    assert found_full[1][2] == "day"
    assert found_full[1][3] == "en"


def test_custom_timezone_abbreviations_and_timezone_parser():
    cleaned, tz = pop_tz_offset_from_string("2026-11-26 15:00 EST")
    assert cleaned == "2026-11-26 15:00"
    assert tz is not None

    dt = dateoxide.parse(
        "2026-11-26 10:00 FOO",
        settings={"TIMEZONE_ABBREVIATIONS": {"FOO": "+04:30"}},
    )
    assert dt is not None
    assert dt.hour == 10
    assert dt.utcoffset() == timedelta(hours=4, minutes=30)


def test_freshness_parser_and_detect_languages_callback():
    fp = FreshnessDateDataParser()
    assert fp.parse("3 days ago", settings={"RELATIVE_BASE": FIXED_BASE}) == datetime(
        2026, 11, 23, 15, 0
    )
    assert fp.parse("12 December 2025") is None
    assert freshness_date_parser.get_date_data(
        "yarın", settings={"RELATIVE_BASE": FIXED_BASE}
    ).locale == "tr"

    called = []

    def custom_detector(text: str, threshold: float = 0.5) -> list[str]:
        called.append((text, threshold))
        return ["tr"]

    dt = dateoxide.parse(
        "12 Aralık 2025",
        detect_languages_function=custom_detector,
    )
    assert dt == datetime(2025, 12, 12)
    assert len(called) == 1


def test_non_gregorian_calendars():
    # Jalali (Persian Solar)
    j1 = JalaliCalendar("1395/01/01").get_date()
    assert j1 is not None and j1.date_obj == datetime(2016, 3, 20)
    j2 = JalaliCalendar("1400/01/01").get_date()
    assert j2 is not None and j2.date_obj == datetime(2021, 3, 21)
    j3 = JalaliCalendar("۵ آذر ۱۴۰۴").get_date()
    assert j3 is not None and j3.date_obj == datetime(2025, 11, 26)
    assert JalaliCalendar("not a jalali date").get_date() is None

    # Hijri (Islamic)
    h1 = HijriCalendar("1445/01/01").get_date()
    assert h1 is not None and h1.date_obj == datetime(2023, 7, 19)
    h2 = HijriCalendar("1 Ramadan 1446").get_date()
    assert h2 is not None and h2.date_obj == datetime(2025, 3, 1)

    # Minguo (Taiwan ROC)
    m1 = MinguoCalendar("115年11月26日").get_date()
    assert m1 is not None and m1.date_obj == datetime(2026, 11, 26)
    m2 = MinguoCalendar("民國115/11/26").get_date()
    assert m2 is not None and m2.date_obj == datetime(2026, 11, 26)

    # French Republican
    f1 = FrenchRepublicanCalendar("1 Vendémiaire an I").get_date()
    assert f1 is not None and f1.date_obj == datetime(1792, 9, 22)
    f2 = FrenchRepublicanCalendar("14 Germinal an III").get_date()
    assert f2 is not None and f2.date_obj == datetime(1795, 4, 3)
