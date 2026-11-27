import time
from datetime import datetime, timezone
import pytest
import dateoxide
from dateoxide import DateDataParser, Settings, search_dates, parse_date_range, parse_batch
from dateoxide.calendars.jalali import JalaliCalendar
from dateoxide.calendars.hijri import HijriCalendar
from dateoxide.calendars.minguo import MinguoCalendar
from dateoxide.calendars.french_republican import FrenchRepublicanCalendar
from dateoxide.timezone_parser import pop_tz_offset_from_string
from dateoxide.utils.time_spans import detect_time_span

BASE = datetime(2026, 5, 13, 14, 0, 0)  # Wednesday
ST = {"RELATIVE_BASE": BASE}


def test_p0_non_ascii_whitespace_no_panic():
    assert dateoxide.parse("2024-03-15\u00A014:30") == datetime(2024, 3, 15, 14, 30)
    assert dateoxide.parse("15\u00A0March\u00A02024") == datetime(2024, 3, 15, 0, 0)
    assert dateoxide.parse("2\u00A0days\u00A0ago", settings=ST) == datetime(2026, 5, 11, 14, 0)
    assert dateoxide.parse("15\u2009March\u20092024") == datetime(2024, 3, 15, 0, 0)
    assert dateoxide.parse("15\u202Fmars\u202F2024", languages=["fr"]) == datetime(2024, 3, 15, 0, 0)
    assert dateoxide.parse("2026-11-26 3:30\u202FPM") == datetime(2026, 11, 26, 15, 30)
    assert dateoxide.parse("2026-11-26\u00A0UTC") == datetime(2026, 11, 26, 0, 0, tzinfo=timezone.utc)
    assert dateoxide.parse("2024年3月15日\u300014時30分") == datetime(2024, 3, 15, 14, 30)
    assert dateoxide.parse("15\u2007March\u20072024") == datetime(2024, 3, 15, 0, 0)
    assert dateoxide.parse("hello\u00A0world") is None
    assert search_dates("Date: 2024-03-15\u00A014:30") == [("2024-03-15\u00A014:30", datetime(2024, 3, 15, 14, 30))]
    assert parse_batch(["2024-03-15", "15\u00A0March\u00A02024"]) == [
        datetime(2024, 3, 15, 0, 0),
        datetime(2024, 3, 15, 0, 0),
    ]
    cleaned, tz = pop_tz_offset_from_string("2026-11-26\u00A0UTC")
    assert cleaned == "2026-11-26" and tz is not None


def test_p0_parse_date_range_utf8_no_panic_and_turkish_cap_i():
    r1 = parse_date_range("3月10-14日", settings=ST)
    assert r1 is None or r1 == (datetime(2026, 3, 10), datetime(2026, 3, 14))
    assert parse_date_range("2月-5日", settings=ST) is None
    r2 = parse_date_range("March\u00A010-14, 2026")
    assert r2 == (datetime(2026, 3, 10), datetime(2026, 3, 14))

    # Turkish capital İ lowercase char expansion bug
    assert parse_date_range("12 NİSAN - 15 MAYIS 2026") == (datetime(2026, 4, 12), datetime(2026, 5, 15))
    assert parse_date_range("12 HAZİRAN - 15 TEMMUZ 2026") == (datetime(2026, 6, 12), datetime(2026, 7, 15))
    assert parse_date_range("12 EKİM - 15 KASIM 2026") == (datetime(2026, 10, 12), datetime(2026, 11, 15))
    assert parse_date_range("İle 2026-01-01 to 2026-01-05") is not None


def test_p0_search_dates_cjk_no_truncation_and_no_quadratic_dos():
    assert search_dates("Meeting: 2026年5月10日") == [("2026年5月10日", datetime(2026, 5, 10))]
    assert search_dates("Date: 令和6年5月15日") == [("令和6年5月15日", datetime(2024, 5, 15))]
    assert search_dates("Date: 2024년5월15일") == [("2024년5월15일", datetime(2024, 5, 15))]

    payload = "1" * 20000 + "a年"
    t0 = time.perf_counter()
    _ = search_dates(payload)
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    assert elapsed_ms < 50.0, f"Quadratic scan detected: {elapsed_ms:.1f} ms"


def test_p0_japanese_and_chinese_12h_ampm():
    assert dateoxide.parse("2026年3月15日 午後2時30分") == datetime(2026, 3, 15, 14, 30)
    assert dateoxide.parse("2026年3月15日午後2時30分") == datetime(2026, 3, 15, 14, 30)
    assert dateoxide.parse("2026年3月15日 午前12時30分") == datetime(2026, 3, 15, 0, 30)
    assert dateoxide.parse("2026年3月15日 晚上8点30分") == datetime(2026, 3, 15, 20, 30)


def test_p0_locale_scoring_dominance_no_minority_hijack():
    dd_jun = DateDataParser(settings=ST).get_date_data("Jun")
    assert dd_jun.date_obj == datetime(2026, 6, 13, 0, 0), f"Got {dd_jun!r}"
    assert dd_jun.locale == "en"

    assert DateDataParser().get_date_data("15 mar 2026 alle 14:30").date_obj == datetime(2026, 3, 15, 14, 30)
    assert DateDataParser().get_date_data("15 giu 2026 alle 14:30").date_obj == datetime(2026, 6, 15, 14, 30)
    assert DateDataParser().get_date_data("15 mai 2026 kell 14:30").date_obj == datetime(2026, 5, 15, 14, 30)
    assert DateDataParser().get_date_data("15 jun 2026 pukul 14:30").date_obj == datetime(2026, 6, 15, 14, 30)

    for s in ("15 Mar 2026", "15 Apr 2026", "15 Jun 2026", "15 Nov 2026", "15 April 2026"):
        dd = DateDataParser().get_date_data(s)
        assert dd.date_obj is not None
        assert dd.locale == "en", f"Expected locale='en' for {s!r}, got {dd.locale!r}"


def test_p0_calendar_bounds_overflows_and_fast_french_republican():
    assert JalaliCalendar("11767034/01/01").get_date() is None
    assert JalaliCalendar("9500/01/01").get_date() is None
    assert HijriCalendar("6066335/01/01").get_date() is None
    assert MinguoCalendar("9000/01/01").get_date() is None
    assert dateoxide.parse("0000-W01-1") is None
    assert dateoxide.parse("9999-12-31 23:00:00 UTC", settings={"TO_TIMEZONE": "Asia/Tokyo"}) is None
    assert dateoxide.parse("0001-01-01 01:00:00 UTC", settings={"TO_TIMEZONE": "America/New_York"}) is None

    t0 = time.perf_counter()
    fr_far = FrenchRepublicanCalendar("14 Germinal 50000000").get_date()
    assert (time.perf_counter() - t0) * 1000.0 < 10.0
    assert fr_far is None


def test_p0_parse_date_range_semantics_and_bare_time():
    r_wd = parse_date_range("from Monday to Friday", settings=ST)
    assert r_wd == (datetime(2026, 5, 11, 0, 0), datetime(2026, 5, 8, 0, 0)) or (
        r_wd is not None and r_wd[0].year == 2026 and r_wd[1].year == 2026 and r_wd[0].weekday() == 0 and r_wd[1].weekday() == 4
    ), f"Bad weekday range: {r_wd}"

    assert parse_date_range("May 10, 2026 14:00 - 16:30", settings=ST) == (
        datetime(2026, 5, 10, 14, 0),
        datetime(2026, 5, 10, 16, 30),
    )
    assert parse_date_range("2026年5月10日〜15日", settings=ST) == (
        datetime(2026, 5, 10, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )


def test_p0_settings_defaults_ignore_surrounding_and_timespan_char_offsets():
    # Settings() must not clobber regional/locale date order
    assert dateoxide.parse("05/06/2026", languages=["en-GB"]) == datetime(2026, 6, 5)
    assert dateoxide.parse("05/06/2026", languages=["en-GB"], settings=Settings()) == datetime(2026, 6, 5)
    assert dateoxide.parse("05/06/2026", region="GB", settings=Settings()) == datetime(2026, 6, 5)

    # IGNORE_SURROUNDING_TEXT=True must not pollute search_dates or clobber parts/part_of_day
    res_sur = search_dates("Call 555-1234 on May 15, 2026 please", settings={"IGNORE_SURROUNDING_TEXT": True})
    assert res_sur == [("May 15, 2026", datetime(2026, 5, 15))]

    dd_m = DateDataParser(settings={"IGNORE_SURROUNDING_TEXT": True}).get_date_data("Published: December 2025")
    assert dd_m.period == "month"
    assert dd_m.parts == ("year", "month")

    dd_tod = DateDataParser(settings={**ST, "IGNORE_SURROUNDING_TEXT": True}).get_date_data("Call yesterday morning")
    assert dd_tod.part_of_day == dateoxide.conf.PartOfDay.MORNING
    assert "time" in dd_tod.parts

    # Python char offsets (not UTF-8 byte offsets) in detect_time_span
    txt = "Türkçe son 3 gün"
    sp = detect_time_span(txt)
    assert sp is not None
    assert txt[sp["start_pos"]:sp["end_pos"]] == sp["matched_text"] == "son 3 gün"
