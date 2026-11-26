"""Verification suite for v0.3.0 features:
- 40 Tier-1 full-featured languages (word_numbers + time_of_day + relative_affix + exact + calendar)
- ISO 639-1 legacy aliases (no -> nb, iw -> he, in -> id)
- Natural & numeric date ranges (parse_date_range)
- 13-language time_spans (EN/TR/DE/FR/ES/IT/PT/NL/PL/RU/JA/ZH/KO)
- Nullable parse_batch support (list containing None)
"""
from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path
import pytest

import dateoxide
from dateoxide.date import DateDataParser
from dateoxide.utils import time_spans

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)
ROOT = Path(__file__).resolve().parent.parent


def test_tier1_40_languages_complete_coverage():
    locales = json.loads(
        (ROOT / "crates" / "dateoxide-core" / "data" / "locales.json").read_text(encoding="utf-8")
    )
    tier1 = [
        l["code"]
        for l in locales
        if len([m for m in l["months"] if m]) == 12
        and len([w for w in l["weekdays"] if w]) == 7
        and l["relative_exact"]
        and l["relative_affix"]
        and l["word_numbers"]
        and l["time_of_day"]
    ]
    assert len(tier1) >= 40, f"Expected >= 40 Tier-1 languages, got {len(tier1)}: {tier1}"

    expected_major = {
        "en", "es", "fr", "de", "pt", "it", "tr", "nl", "ru", "pl", "uk", "cs", "sk",
        "sv", "da", "nb", "fi", "ro", "hu", "el", "he", "ar", "fa", "ur", "hi", "bn",
        "id", "ms", "vi", "th", "zh", "ja", "ko", "az", "uz", "kk", "bg", "hr", "sr", "ca",
    }
    assert expected_major.issubset(set(tier1))


@pytest.mark.parametrize(
    "lang,text,expected",
    [
        ("pl", "dwa dni temu", datetime(2026, 11, 24, 15, 0)),
        ("cs", "před tři dny", datetime(2026, 11, 23, 15, 0)),
        ("uk", "три дні тому", datetime(2026, 11, 23, 15, 0)),
        ("fi", "kaksi päivää sitten", datetime(2026, 11, 24, 15, 0)),
        ("ar", "قبل ثلاثة أيام", datetime(2026, 11, 23, 15, 0)),
        ("id", "tiga hari yang lalu", datetime(2026, 11, 23, 15, 0)),
        ("az", "üç gün öncə", datetime(2026, 11, 23, 15, 0)),
        ("ja", "15 12月 2025 朝", datetime(2025, 12, 15, 9, 0)),
        ("zh", "2026年11月26日 晚上", datetime(2026, 11, 26, 20, 0)),
        ("ko", "2026년 11월 26일 아침", datetime(2026, 11, 26, 9, 0)),
        ("pl", "15 maja 2025 rano", datetime(2025, 5, 15, 9, 0)),
        ("he", "אתמול בערב", datetime(2026, 11, 25, 18, 0)),
    ],
)
def test_newly_expanded_tier1_word_numbers_and_tod(lang: str, text: str, expected: datetime):
    dt = dateoxide.parse(text, languages=[lang], settings={"RELATIVE_BASE": FIXED_BASE})
    assert dt == expected, f"[{lang}] {text!r} -> got {dt!r} vs {expected!r}"


def test_iso_639_legacy_aliases():
    cfg = {"RELATIVE_BASE": FIXED_BASE}
    assert dateoxide.parse("3 dager siden", languages=["no"], settings=cfg) == datetime(2026, 11, 23, 15, 0)
    assert dateoxide.parse("אתמול", languages=["iw"], settings=cfg) == datetime(2026, 11, 25, 15, 0)
    assert dateoxide.parse("3 hari yang lalu", languages=["in"], settings=cfg) == datetime(2026, 11, 23, 15, 0)


def test_parse_date_range_comprehensive():
    cfg = {"RELATIVE_BASE": FIXED_BASE}

    assert dateoxide.parse_date_range("12-15 Ocak 2026", settings=cfg) == (
        datetime(2026, 1, 12),
        datetime(2026, 1, 15),
    )
    assert dateoxide.parse_date_range("March 10-14, 2026", settings=cfg) == (
        datetime(2026, 3, 10),
        datetime(2026, 3, 14),
    )
    assert dateoxide.parse_date_range("12 Ocak - 15 Şubat 2026", settings=cfg) == (
        datetime(2026, 1, 12),
        datetime(2026, 2, 15),
    )
    # Year rollover across Dec -> Jan
    assert dateoxide.parse_date_range("Dec 28 - Jan 3, 2026", settings=cfg) == (
        datetime(2025, 12, 28),
        datetime(2026, 1, 3),
    )
    assert dateoxide.parse_date_range("from 2026-11-01 to 2026-11-15", settings=cfg) == (
        datetime(2026, 11, 1),
        datetime(2026, 11, 15),
    )
    assert dateoxide.parse_date_range("10:00 - 12:30", settings=cfg) == (
        datetime(2026, 11, 26, 10, 0),
        datetime(2026, 11, 26, 12, 30),
    )
    assert dateoxide.parse_date_range("von 2026-05-01 bis 2026-05-10", settings=cfg) == (
        datetime(2026, 5, 1),
        datetime(2026, 5, 10),
    )

    ddp = DateDataParser(settings=cfg)
    assert ddp.parse_date_range("12–15 December 2025") == (
        datetime(2025, 12, 12),
        datetime(2025, 12, 15),
    )

    # Single dates & non-ranges must return None
    assert dateoxide.parse_date_range("2026-11-26", settings=cfg) is None
    assert dateoxide.parse_date_range("not a range", settings=cfg) is None


@pytest.mark.parametrize(
    "text,exp_type,exp_dir,exp_num",
    [
        ("ultimi 5 giorni", "days", "past", 5),
        ("settimana scorsa", "week", "past", None),
        ("próximos 3 dias", "days", "future", 3),
        ("semana passada", "week", "past", None),
        ("afgelopen 4 dagen", "days", "past", 4),
        ("ostatnie 7 dni", "days", "past", 7),
        ("последние 3 дня", "days", "past", 3),
        ("過去7日間", "days", "past", 7),
        ("过去3天", "days", "past", 3),
        ("지난 5일", "days", "past", 5),
    ],
)
def test_13_language_time_spans(text: str, exp_type: str, exp_dir: str, exp_num: int | None):
    sp = time_spans.detect_time_span(text)
    assert sp is not None, f"Failed time_span on {text!r}"
    assert (sp["type"], sp["direction"], sp.get("number")) == (exp_type, exp_dir, exp_num)


def test_nullable_parse_batch():
    cfg = {"RELATIVE_BASE": FIXED_BASE}
    items = ["2026-11-26", None, "3 gün önce", None, "not a date"]
    expected = [datetime(2026, 11, 26), None, datetime(2026, 11, 23, 15, 0), None, None]

    assert dateoxide.parse_batch(items, settings=cfg) == expected
    ddp = DateDataParser(settings=cfg)
    assert ddp.parse_batch(items) == expected
