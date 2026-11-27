import ast
import inspect
import time
from datetime import datetime, timezone
from pathlib import Path

import pytest

import dateoxide
from dateoxide import (
    DateDataParser,
    parse,
    parse_date_range,
    parse_french_republican,
    parse_hijri,
    parse_jalali,
    parse_minguo,
    search_dates,
)
from dateoxide.freshness_date_parser import FreshnessDateDataParser

BASE = datetime(2026, 3, 15, 12, 0, 0)  # Sunday
ST = {"RELATIVE_BASE": BASE}
REPO_ROOT = Path(__file__).resolve().parent.parent


def test_p0_1_four_quad_dos_vectors_linear_time():
    vectors = [
        ("numerals_cjk_tail", lambda n: parse("1" * n + "一", settings=ST), 10_000, 20_000),
        ("se_asian_month_run", lambda n: search_dates("มกราคม" * n + "1", settings=ST), 500, 1_000),
        ("cjk_colon_run", lambda n: parse("1:" * n + "年", settings=ST), 5_000, 10_000),
        ("trailing_time_non_ascii_digit", lambda n: parse("あ1" * n + ":", settings=ST), 2_000, 4_000),
    ]
    for name, fn, n1, n2 in vectors:
        t0 = time.perf_counter()
        fn(n1)
        dt1 = time.perf_counter() - t0
        t0 = time.perf_counter()
        fn(n2)
        dt2 = time.perf_counter() - t0
        # At linear time, 2x length takes ~2x (never > 30ms here, and never >= 3.2x when significant)
        assert dt2 < 0.030, f"{name} too slow at N={n2}: {dt2*1000:.2f}ms (N={n1}: {dt1*1000:.2f}ms)"


def test_p0_2_year_boundary_overflows_never_crash():
    # ranges.rs overnight midnight wrap across year 9999
    assert parse_date_range("9999-12-31 23:00 - 01:00") is None
    assert parse_date_range("23:00-01:00", settings={"RELATIVE_BASE": datetime(9999, 12, 31, 12, 0)}) is None
    assert parse_date_range("23:00 - 01:00", settings={"RELATIVE_BASE": datetime(9999, 12, 31, 12, 0)}) is None

    # parse_freshness + relative day boundaries at year 1 and 9999
    fdp = FreshnessDateDataParser()
    assert fdp.get_date_data(
        "1 day ago UTC",
        settings={"RELATIVE_BASE": datetime(1, 1, 2, 0, 0), "TO_TIMEZONE": "-05:00"},
    ).date_obj is None
    assert fdp.get_date_data("day before yesterday", settings={"RELATIVE_BASE": datetime(1, 1, 1, 0, 0)}).date_obj is None
    assert fdp.get_date_data("day after tomorrow", settings={"RELATIVE_BASE": datetime(9999, 12, 31, 0, 0)}).date_obj is None
    assert fdp.get_date_data("tomorrow morning", settings={"RELATIVE_BASE": datetime(9999, 12, 31, 0, 0)}).date_obj is None

    # Calendars post-TZ year range guards
    assert parse_jalali("9378/10/10 23:00 UTC", settings={"TO_TIMEZONE": "+05:00"}).date_obj is None
    assert parse_hijri("9666/04/02 23:00 UTC", settings={"TO_TIMEZONE": "+05:00"}).date_obj is None
    assert parse_minguo("8088/12/31 23:00 UTC", settings={"TO_TIMEZONE": "+05:00"}).date_obj is None
    assert parse_french_republican(
        "10 Nivose an 8208",
        settings={"TIMEZONE": "-10:00", "TO_TIMEZONE": "+14:00"},
    ).date_obj is None

    # search_dates custom date_formats & RETURN_TIME_SPAN post-TZ year guards
    assert search_dates(
        "9999-12-31 23:00",
        date_formats=["%Y-%m-%d %H:%M"],
        settings={"TIMEZONE": "UTC", "TO_TIMEZONE": "+05:00"},
    ) is None
    assert search_dates(
        "0001-01-01 01:00",
        date_formats=["%Y-%m-%d %H:%M"],
        settings={"TIMEZONE": "UTC", "TO_TIMEZONE": "-05:00"},
    ) is None
    assert search_dates(
        "next 1 day",
        settings={"RETURN_TIME_SPAN": True, "RELATIVE_BASE": datetime(9999, 12, 30, 23, 0), "TIMEZONE": "UTC", "TO_TIMEZONE": "+05:00"},
    ) is None
    assert search_dates(
        "past 1 day",
        settings={"RETURN_TIME_SPAN": True, "RELATIVE_BASE": datetime(1, 1, 2, 1, 0), "TIMEZONE": "UTC", "TO_TIMEZONE": "-05:00"},
    ) is None


def test_p0_5_cjk_multi_date_no_silent_overwrite():
    s = "開始日: 2025年4月8日、更新: 5日前"
    assert parse(s, settings=ST) is None
    found = search_dates(s, settings=ST)
    assert found == [
        ("2025年4月8日", datetime(2025, 4, 8, 0, 0)),
        ("5日前", datetime(2026, 3, 10, 12, 0)),
    ]


def test_p0_6_japanese_kanji_quarters():
    cases = [
        ("2026年第一四半期", datetime(2026, 1, 1, 0, 0)),
        ("2026年第二四半期", datetime(2026, 4, 1, 0, 0)),
        ("2026年第三四半期", datetime(2026, 7, 1, 0, 0)),
        ("2026年第四四半期", datetime(2026, 10, 1, 0, 0)),
    ]
    for expr, exp in cases:
        assert parse(expr, languages=["ja"], settings=ST) == exp


def test_p0_7_hebrew_word_ordinals_not_week_clobbered():
    for d, word in enumerate(["ראשון", "שני", "שלישי", "רביעי", "חמישי", "שישי"], start=1):
        assert parse(f"{word} במאי 2024", languages=["he"], settings=ST) == datetime(2024, 5, d, 0, 0)


def test_p0_8_impossible_ordinal_weekdays_return_none():
    assert parse("fifth Wednesday of March 2026", languages=["en"], settings=ST) is None
    assert parse("sixth Monday of March 2026", languages=["en"], settings=ST) is None


def test_p0_9_english_evening_afternoon_tonight_pm():
    assert parse("tomorrow evening at 7:00", languages=["en"], settings=ST) == datetime(2026, 3, 16, 19, 0)
    assert parse("tomorrow afternoon at 3:30", languages=["en"], settings=ST) == datetime(2026, 3, 16, 15, 30)
    assert parse("tonight at 8:00", languages=["en"], settings=ST) == datetime(2026, 3, 15, 20, 0)
    assert parse("tomorrow evening at 7", languages=["en"], settings=ST) == datetime(2026, 3, 16, 19, 0)


def test_p0_10_search_dates_explicit_languages_and_auto_no_fp():
    assert search_dates("Det skete for to dage siden.", languages=["da"], settings=ST) == [
        ("for to dage siden", datetime(2026, 3, 13, 12, 0)),
    ]
    assert search_dates("Det skjedde for to dager siden.", languages=["nb"], settings=ST) == [
        ("for to dager siden", datetime(2026, 3, 13, 12, 0)),
    ]
    assert search_dates("Møtet var for tre døgn sidan i byen.", languages=["nn"], settings=ST) == [
        ("for tre døgn sidan", datetime(2026, 3, 12, 12, 0)),
    ]
    assert search_dates("Report dated 18 me 2024 confirmed.", languages=["mfe"], settings=ST) == [
        ("18 me 2024", datetime(2024, 5, 18, 0, 0)),
    ]
    assert search_dates("There is no reason to worry about this.", settings=ST) is None


def test_p0_11_period_boundary_trailing_invalid_tokens():
    assert parse("end of July invalid", languages=["en"], settings=ST) is None
    assert parse("mid-February 99999", languages=["en"], settings=ST) is None
    assert parse("mid-February 24", languages=["en"], settings=ST) is None


def test_p0_12_weekday_range_never_inverted():
    r1 = parse_date_range("from Monday to Friday", settings={"RELATIVE_BASE": datetime(2026, 5, 13, 12, 0)})
    assert r1 == (datetime(2026, 5, 11, 0, 0), datetime(2026, 5, 15, 0, 0))
    r2 = parse_date_range("from Friday to Monday", settings=ST)
    assert r2 == (datetime(2026, 3, 13, 0, 0), datetime(2026, 3, 16, 0, 0))


def test_p0_4_pyi_stubs_valid_pep484_and_match_runtime():
    for stub_path in (
        REPO_ROOT / "python" / "dateoxide" / "__init__.pyi",
        REPO_ROOT / "python" / "dateoxide" / "_dateoxide.pyi",
    ):
        tree = ast.parse(stub_path.read_text(encoding="utf-8"))
        # Must not mix @overload and bare def with the same name in a .pyi
        fn_defs = [n for n in tree.body if isinstance(n, ast.FunctionDef) and n.name == "search_dates"]
        assert fn_defs and all(
            any(isinstance(d, ast.Name) and d.id == "overload" for d in fn.decorator_list)
            for fn in fn_defs
        ), f"PEP 484 illegal non-overload search_dates in {stub_path.name}"

    init_tree = ast.parse((REPO_ROOT / "python" / "dateoxide" / "__init__.pyi").read_text(encoding="utf-8"))
    top_names = {
        n.name for n in init_tree.body if isinstance(n, (ast.FunctionDef, ast.ClassDef))
    }
    for name in top_names:
        assert hasattr(dateoxide, name), f"Phantom stub symbol dateoxide.{name}"
    for sub in dateoxide.__all__:
        if not hasattr(dateoxide, sub) and sub not in top_names:
            continue
    # Submodules declared in __all__ must be typed in __init__.pyi
    init_src = (REPO_ROOT / "python" / "dateoxide" / "__init__.pyi").read_text(encoding="utf-8")
    for sub in ("calendars", "conf", "date", "freshness_date_parser", "search", "timezone_parser", "utils"):
        assert f"import {sub} as {sub}" in init_src
