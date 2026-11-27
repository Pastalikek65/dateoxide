"""v0.7.1 P1 and P2 regression tests (TDD baseline)."""
from __future__ import annotations

from datetime import datetime, timedelta
import json
from pathlib import Path

import pytest

import dateoxide

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)
LOCALES_JSON = Path(__file__).resolve().parent.parent / "crates" / "dateoxide-core" / "data" / "locales.json"


UNICODE_16_ZERO_CP = [
    ("Garay", 0x10D40),
    ("Myanmar Pa'O", 0x116D0),
    ("Myanmar Eastern Pwo Karen", 0x116DA),
    ("Sunuwar", 0x11BF0),
    ("Gurung Khema", 0x16130),
    ("Kirat Rai", 0x16D70),
    ("Outlined", 0x1CCF0),
    ("Ol Onal", 0x1E5F1),
]


def _encode_script_date(zero_cp: int, date_str: str = "2026-05-15") -> str:
    return "".join(chr(zero_cp + int(ch)) if ch.isdigit() else ch for ch in date_str)


@pytest.mark.parametrize("script_name,zero_cp", UNICODE_16_ZERO_CP)
def test_p1_unicode16_nd_scripts(script_name: str, zero_cp: int) -> None:
    s = _encode_script_date(zero_cp, "2026-05-15")
    assert dateoxide.parse(s) == datetime(2026, 5, 15), f"Failed parse for {script_name}"
    found = dateoxide.search_dates(f"Event {s} confirmed")
    assert found and any(dt == datetime(2026, 5, 15) for _, dt in found), (
        f"Failed search_dates for {script_name}"
    )


@pytest.mark.parametrize(
    "phrase",
    ["这一时间", "這一小時", "這一分鐘", "这一分钟", "这一小时"],
)
def test_p1_chinese_relative_exact_zero_offset(phrase: str) -> None:
    res = dateoxide.parse(phrase, languages=["zh"], settings={"RELATIVE_BASE": FIXED_BASE})
    assert res == FIXED_BASE


@pytest.mark.parametrize(
    "lang,phrase,expected",
    [
        ("ar", "قبل واحد وعشرون يوم", FIXED_BASE - timedelta(days=21)),
        ("ar", "قبل خمسة وثلاثون يوم", FIXED_BASE - timedelta(days=35)),
        ("sv", "för tjugo en dagar sedan", FIXED_BASE - timedelta(days=21)),
        ("da", "for tyve en dage siden", FIXED_BASE - timedelta(days=21)),
        ("nl", "eenentwintig dagen geleden", FIXED_BASE - timedelta(days=21)),
        ("nl", "tweeëntwintig dagen geleden", FIXED_BASE - timedelta(days=22)),
        ("ko", "스물한 일 전", FIXED_BASE - timedelta(days=21)),
        ("ko", "이십오 일 전", FIXED_BASE - timedelta(days=25)),
        ("vi", "hai mươi một ngày trước", FIXED_BASE - timedelta(days=21)),
    ],
)
def test_p1_multilingual_compound_cardinals(lang: str, phrase: str, expected: datetime) -> None:
    res = dateoxide.parse(phrase, languages=[lang], settings={"RELATIVE_BASE": FIXED_BASE})
    assert res == expected


def test_p1_spanish_ascii_ordinal() -> None:
    assert dateoxide.parse("2o mayo 2024", languages=["es"]) == datetime(2024, 5, 2)


@pytest.mark.parametrize("fp", ["2.4.1", "1.2.3", "0.7.1", "TK 1923", "DID I MIX"])
def test_p1_false_positives_must_return_none(fp: str) -> None:
    assert dateoxide.parse(fp, settings={"RELATIVE_BASE": FIXED_BASE}) is None


def test_p1_valid_roman_numeral_dates() -> None:
    assert dateoxide.parse("15.V.2024") == datetime(2024, 5, 15)
    assert dateoxide.parse("XV V MMXXIV") == datetime(2024, 5, 15)
    found = dateoxide.search_dates("Dated XV May MMXXIV in Rome.")
    assert found and any(dt == datetime(2024, 5, 15) for _, dt in found)


def test_p1_iana_dst_iso_interval() -> None:
    rng = dateoxide.parse_date_range(
        "2026-03-07T10:00:00/P2D",
        settings={"TIMEZONE": "America/New_York", "RETURN_AS_TIMEZONE_AWARE": True},
    )
    assert rng is not None
    start, end = rng
    assert start.hour == 10 and end.hour == 10
    assert start.utcoffset() == timedelta(hours=-5)
    assert end.utcoffset() == timedelta(hours=-4)


def test_p1_turkish_nlp_seasons_and_clocks() -> None:
    assert dateoxide.parse("KIŞ 2026") == datetime(2026, 12, 1)
    assert dateoxide.parse(
        "on biri yirmi beş geçe", languages=["tr"], settings={"RELATIVE_BASE": FIXED_BASE}
    ) == datetime(2026, 11, 26, 11, 25)
    assert dateoxide.parse(
        "on ikiye on beş kala", languages=["tr"], settings={"RELATIVE_BASE": FIXED_BASE}
    ) == datetime(2026, 11, 26, 11, 45)


def test_p1_thai_buddhist_era_and_spaceless_search() -> None:
    be_only = dateoxide.parse("2567", languages=["th"])
    assert be_only is not None and be_only.year == 2024
    non_thai = dateoxide.parse("15 May 2567")
    assert non_thai == datetime(2567, 5, 15)
    found = dateoxide.search_dates(
        "เมื่อห้านาทีที่แล้วมีข้อความ",
        languages=["th"],
        settings={"RELATIVE_BASE": FIXED_BASE},
    )
    assert found and any(dt == FIXED_BASE - timedelta(minutes=5) for _, dt in found)


def test_p2_time_span_exact_byte_slices_and_word_numbers() -> None:
    from dateoxide.utils.time_spans import detect_time_span

    s1 = "Show me next three months please"
    m1 = detect_time_span(s1)
    assert m1 is not None
    assert m1["number"] == 3
    assert m1["matched_text"] == "next three months"
    assert s1[m1["start_pos"] : m1["end_pos"]] == m1["matched_text"]

    s2 = "Türkçe son üç gün içinde"
    m2 = detect_time_span(s2)
    assert m2 is not None
    assert m2["number"] == 3
    assert m2["matched_text"] == "son üç gün"
    assert s2[m2["start_pos"] : m2["end_pos"]] == m2["matched_text"]

    s3 = "In the past twenty days"
    m3 = detect_time_span(s3)
    assert m3 is not None
    assert m3["number"] == 20
    assert m3["matched_text"].lower() == "in the past twenty days"
    assert s3[m3["start_pos"] : m3["end_pos"]] == m3["matched_text"]


def test_p2_calendars_minguo_yuan_and_strict_republican_roman() -> None:
    mg = dateoxide.parse_minguo("民國元年1月1日")
    assert mg.date_obj == datetime(1912, 1, 1)
    fr_bad = dateoxide.parse_french_republican("14 Germinal an IIII")
    assert fr_bad.date_obj is None


def test_p1_tigrinya_no_duplicate_opposing_affix() -> None:
    locales = json.loads(LOCALES_JSON.read_text(encoding="utf-8"))
    ti = next(loc for loc in locales if loc["code"] == "ti")
    seen: dict[tuple[str, str, int], set[int]] = {}
    for pref, suf, sign, unit in ti.get("relative_affix", []):
        seen.setdefault((pref, suf, unit), set()).add(sign)
    dups = {k: v for k, v in seen.items() if len(v) > 1}
    assert not dups, f"Conflicting duplicate signs in ti relative_affix: {dups}"
    assert dateoxide.parse("ኣብ 3 መዓልቲ", languages=["ti"], settings={"RELATIVE_BASE": FIXED_BASE}) == (
        FIXED_BASE + timedelta(days=3)
    )
