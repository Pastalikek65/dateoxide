from datetime import datetime, time, timedelta, timezone
import time as pytime
from types import MappingProxyType

import pytest

import dateoxide
from dateoxide import (
    DateData,
    PartOfDay,
    PartsOfDay,
    SettingValidationError,
    Settings,
    detect_time_span,
    generate_time_span,
    parse,
    parse_date_range,
    pop_tz_offset_from_string,
    search_dates,
)
from dateoxide.freshness_date_parser import FreshnessDateDataParser

BASE = datetime(2026, 11, 26, 15, 0, 0)
BASE_SETTINGS = {"RELATIVE_BASE": BASE}


def test_p0_range_iso_duration_overflow_no_panic():
    assert parse_date_range("2026-01-01/PT9999999999999999H") is None
    assert parse_date_range("2026-01-01/PT9999999999999999M") is None
    assert parse_date_range("2026-01-01/PT9999999999999999S") is None
    assert parse_date_range("2026-01-01/PT5000000000000000000S5000000000000000000S") is None


def test_p2_range_bidi_and_nbsp_surface():
    got = parse_date_range(
        "\u2066from\u00a0Monday\u2009to\u202fFriday\u2069",
        settings=BASE_SETTINGS,
    )
    assert got == (datetime(2026, 11, 23, 0, 0), datetime(2026, 11, 27, 0, 0))


def test_p0_nl_af_compound_dos_guard():
    payload = "\u00e9" + "en" * 15000
    t0 = pytime.perf_counter()
    assert detect_time_span(payload) is None
    assert (pytime.perf_counter() - t0) < 0.05

    t1 = pytime.perf_counter()
    assert parse(payload, languages=["nl"]) is None
    assert (pytime.perf_counter() - t1) < 0.05


def test_p0_currency_prefix_and_flight_year_fps():
    assert parse("EUR 2026", settings=BASE_SETTINGS) is None
    assert search_dates("Record EUR 2026 logged", settings=BASE_SETTINGS) is None
    assert search_dates("Record TRY 2025 logged", settings=BASE_SETTINGS) is None
    assert search_dates("Record USD 1999 logged", settings=BASE_SETTINGS) is None
    assert search_dates("Record TK 1923 logged", settings=BASE_SETTINGS) is None
    assert search_dates("Record LH 2025 logged", settings=BASE_SETTINGS) is None
    assert search_dates("Record UA 1999 logged", settings=BASE_SETTINGS) is None
    assert search_dates("ISO 2024 and PID 2024", settings=BASE_SETTINGS) is None

    # Legitimate dates remain unaffected
    assert parse("May 2026", settings=BASE_SETTINGS) == datetime(2026, 5, 26, 0, 0)
    found_2025 = search_dates("Released in 2025", settings=BASE_SETTINGS)
    assert found_2025 is not None and found_2025[0][1] == datetime(2025, 11, 26, 0, 0)
    found_may = search_dates("Meeting on MAY 2026", settings=BASE_SETTINGS)
    assert found_may is not None and found_may[0][1].year == 2026 and found_may[0][1].month == 5


def test_p0_roman_prose_and_minor_locale_2letter_fps():
    assert search_dates("Record DID I MIX logged", settings=BASE_SETTINGS) is None
    assert search_dates("Record CIVIL MIX I logged", settings=BASE_SETTINGS) is None
    assert parse("I MIX", settings=BASE_SETTINGS) is None
    assert parse("MIX I", settings=BASE_SETTINGS) is None
    assert parse("MD 2025", settings=BASE_SETTINGS) is None
    assert parse("OK 2025", settings=BASE_SETTINGS) is None

    # Valid Roman numeral dates continue to work
    assert parse("15.V.2024") == datetime(2024, 5, 15, 0, 0)
    assert parse("XV V MMXXIV") == datetime(2024, 5, 15, 0, 0)
    assert parse("XV May MMXXIV") == datetime(2024, 5, 15, 0, 0)
    roman_search = search_dates("Signed XV May MMXXIV in Rome.")
    assert roman_search is not None and roman_search[0][1] == datetime(2024, 5, 15, 0, 0)


def test_p1_dense_search_dates_performance():
    t0 = pytime.perf_counter()
    r_cjk = search_dates("2026年1月1日 " * 1500)
    assert r_cjk is not None and len(r_cjk) == 1500
    assert (pytime.perf_counter() - t0) < 0.25

    t1 = pytime.perf_counter()
    assert search_dates("EST " * 5000) is None
    assert (pytime.perf_counter() - t1) < 0.05

    t2 = pytime.perf_counter()
    r_dm = search_dates("12-15 Dec 2026 " * 500)
    assert r_dm is not None and len(r_dm) >= 500
    assert (pytime.perf_counter() - t2) < 0.25

    t3 = pytime.perf_counter()
    assert search_dates("Monday " * 2000, settings=BASE_SETTINGS) is None
    assert (pytime.perf_counter() - t3) < 0.05


def test_p1_multilingual_numerals_and_clocks():
    # Arabic 70..99 genitive (-in)
    assert parse("قبل ثلاثة وسبعين يوماً", languages=["ar"], settings=BASE_SETTINGS) == datetime(2026, 9, 14, 15, 0)
    assert parse("قبل ثمانية وثمانين يوما", languages=["ar"], settings=BASE_SETTINGS) == datetime(2026, 8, 30, 15, 0)
    assert parse("قبل تسعة وتسعين يوما", languages=["ar"], settings=BASE_SETTINGS) == datetime(2026, 8, 19, 15, 0)

    # French 17..19 and 97..99
    assert parse("il y a dix-sept jours", languages=["fr"], settings=BASE_SETTINGS) == datetime(2026, 11, 9, 15, 0)
    assert parse("il y a dix-neuf jours", languages=["fr"], settings=BASE_SETTINGS) == datetime(2026, 11, 7, 15, 0)
    assert parse("il y a quatre-vingt-dix-sept jours", languages=["fr"], settings=BASE_SETTINGS) == datetime(2026, 8, 21, 15, 0)
    assert parse("il y a quatre-vingt-dix-neuf jours", languages=["fr"], settings=BASE_SETTINGS) == datetime(2026, 8, 19, 15, 0)

    # Vietnamese tens + mốt/tư/lăm + preserve Wednesday ("thứ tư")
    assert parse("hai mươi mốt ngày trước", languages=["vi"], settings=BASE_SETTINGS) == datetime(2026, 11, 5, 15, 0)
    assert parse("hai mươi tư ngày trước", languages=["vi"], settings=BASE_SETTINGS) == datetime(2026, 11, 2, 15, 0)
    assert parse("ba mươi lăm ngày trước", languages=["vi"], settings=BASE_SETTINGS) == datetime(2026, 10, 22, 15, 0)
    wed = parse("thứ tư", languages=["vi"], settings=BASE_SETTINGS)
    assert wed is not None and wed.weekday() == 2

    # Turkish conversational clocks 13..23
    assert parse("on dörde çeyrek kala", languages=["tr"], settings=BASE_SETTINGS) == datetime(2026, 11, 26, 13, 45)
    assert parse("on beşi yirmi beş geçe", languages=["tr"], settings=BASE_SETTINGS) == datetime(2026, 11, 26, 15, 25)
    assert parse("yirmi bire çeyrek kala", languages=["tr"], settings=BASE_SETTINGS) == datetime(2026, 11, 26, 20, 45)
    assert parse("yirmi üçü on geçe", languages=["tr"], settings=BASE_SETTINGS) == datetime(2026, 11, 26, 23, 10)

    # Korean standalone native tens 40..90
    assert parse("마흔 일 전", languages=["ko"], settings=BASE_SETTINGS) == BASE - timedelta(days=40)
    assert parse("쉰 일 전", languages=["ko"], settings=BASE_SETTINGS) == BASE - timedelta(days=50)
    assert parse("예순 일 전", languages=["ko"], settings=BASE_SETTINGS) == BASE - timedelta(days=60)
    assert parse("일흔 일 전", languages=["ko"], settings=BASE_SETTINGS) == BASE - timedelta(days=70)
    assert parse("여든 일 전", languages=["ko"], settings=BASE_SETTINGS) == BASE - timedelta(days=80)
    assert parse("아흔 일 전", languages=["ko"], settings=BASE_SETTINGS) == BASE - timedelta(days=90)

    # Spaceless Thai digits
    assert parse("เมื่อ10นาทีที่แล้ว", settings=BASE_SETTINGS) == datetime(2026, 11, 26, 14, 50)
    assert parse("เมื่อ๑๐นาทีที่แล้ว", settings=BASE_SETTINGS) == datetime(2026, 11, 26, 14, 50)
    assert search_dates("เมื่อ๑๐นาทีที่แล้วพบปัญหา", settings=BASE_SETTINGS) == [
        ("เมื่อ๑๐นาทีที่แล้ว", datetime(2026, 11, 26, 14, 50))
    ]


def test_p1_python_api_validation_mapping_and_repr():
    # PARTS_OF_DAY validation
    with pytest.raises(SettingValidationError):
        Settings({"PARTS_OF_DAY": "morning"})
    with pytest.raises(SettingValidationError):
        Settings({"PARTS_OF_DAY": {"morning": "09:00"}})
    with pytest.raises(SettingValidationError):
        Settings({"PARTS_OF_DAY": {"bad": time(9, 0)}})
    with pytest.raises(SettingValidationError):
        parse("2026-05-15 morning", settings={"PARTS_OF_DAY": "bad"})
    assert Settings({"PARTS_OF_DAY": PartsOfDay(morning=time(8, 30))}).PARTS_OF_DAY.morning == time(8, 30)

    # DEFAULT_LANGUAGES and TIMEZONE_ABBREVIATIONS validation
    with pytest.raises(SettingValidationError):
        Settings({"DEFAULT_LANGUAGES": ["xx_invalid_lang"]})
    with pytest.raises(SettingValidationError):
        parse("2026-01-01", settings={"DEFAULT_LANGUAGES": ["xx_invalid_lang"]})
    with pytest.raises(SettingValidationError):
        Settings({"TIMEZONE_ABBREVIATIONS": {"FOO": 123}})
    with pytest.raises(SettingValidationError):
        Settings({"TIMEZONE_ABBREVIATIONS": {123: "UTC"}})

    # MappingProxyType acceptance
    assert parse("01/02/2026", settings=MappingProxyType({"DATE_ORDER": "DMY"})) == datetime(2026, 2, 1)
    assert pop_tz_offset_from_string(
        "2026-05-15 12:00 FOO",
        timezone_abbreviations=MappingProxyType({"FOO": "+04:30"}),
    )[0] == "2026-05-15 12:00"
    span = generate_time_span(
        MappingProxyType({"type": "days", "direction": "past", "number": 3}),
        base_date=BASE,
    )
    assert span == (datetime(2026, 11, 23, 15, 0), BASE)

    # generate_time_span unknown type raises ValueError
    with pytest.raises(ValueError):
        generate_time_span({"type": "decades", "direction": "past", "number": 1}, base_date=BASE)

    # FreshnessDateDataParser rejects bare str languages
    with pytest.raises(TypeError):
        FreshnessDateDataParser().get_date_data("2 days ago", languages="en")  # type: ignore[arg-type]

    # DateData.__repr__ single-element parts comma & part_of_day
    dd = DateData(parts=("2026",), part_of_day=PartOfDay.EVENING)
    r = repr(dd)
    assert "('2026',)" in r
    assert "part_of_day=" in r
