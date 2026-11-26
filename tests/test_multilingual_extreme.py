"""Extreme multilingual torture test suite covering 73 Tier-1 languages,
multiple Unicode numeral scripts, +/-2-day idioms, post-positional & CJK weekdays,
and future prepositions (verified under both auto-detect and explicit languages).
"""
from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path
import pytest

import dateoxide

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)
CFG = {"RELATIVE_BASE": FIXED_BASE}
ROOT = Path(__file__).resolve().parent.parent

EXTREME_CASES: list[tuple[str, str, datetime]] = [
    # Unicode decimal scripts
    ("hi", "३ दिन पहले", datetime(2026, 11, 23, 15, 0)),
    ("bn", "৩ দিন আগে", datetime(2026, 11, 23, 15, 0)),
    ("th", "๓ วันที่แล้ว", datetime(2026, 11, 23, 15, 0)),
    ("ja", "３日前", datetime(2026, 11, 23, 15, 0)),
    ("fa", "۳ ماه دیگر", datetime(2027, 2, 26, 15, 0)),
    ("ar", "بعد ٣ أيام", datetime(2026, 11, 29, 15, 0)),
    ("ar", "١٢ ديسمبر ٢٠٢٥", datetime(2025, 12, 12)),
    ("mr", "१५ डिसेंबर २०२५", datetime(2025, 12, 15)),
    ("ta", "15 டிசம்பர் 2025", datetime(2025, 12, 15)),
    ("ta", "௧௫ டிசம்பர் ௨௦௨௫", datetime(2025, 12, 15)),
    ("my", "၁၅ ဒီဇင်ဘာ ၂၀၂၅", datetime(2025, 12, 15)),
    ("km", "១៥ ធ្នូ ២០២៥", datetime(2025, 12, 15)),
    ("kn", "೧೫ ಡಿಸೆಂಬರ್ ೨೦೨೫", datetime(2025, 12, 15)),
    ("te", "౧౫ డిసెంబర్ ౨౦౨౫", datetime(2025, 12, 15)),
    # +/- 2 day idioms across 20+ languages
    ("tr", "evvelsi gün", datetime(2026, 11, 24, 15, 0)),
    ("tr", "öbür gün", datetime(2026, 11, 28, 15, 0)),
    ("tr", "2 gün evvel", datetime(2026, 11, 24, 15, 0)),
    ("de", "vorgestern", datetime(2026, 11, 24, 15, 0)),
    ("de", "übermorgen", datetime(2026, 11, 28, 15, 0)),
    ("fr", "avant-hier", datetime(2026, 11, 24, 15, 0)),
    ("fr", "après-demain", datetime(2026, 11, 28, 15, 0)),
    ("es", "anteayer", datetime(2026, 11, 24, 15, 0)),
    ("es", "pasado mañana", datetime(2026, 11, 28, 15, 0)),
    ("pt", "anteontem", datetime(2026, 11, 24, 15, 0)),
    ("pt", "depois de amanhã", datetime(2026, 11, 28, 15, 0)),
    ("it", "dopodomani", datetime(2026, 11, 28, 15, 0)),
    ("nl", "eergisteren", datetime(2026, 11, 24, 15, 0)),
    ("nl", "overmorgen", datetime(2026, 11, 28, 15, 0)),
    ("ru", "позавчера", datetime(2026, 11, 24, 15, 0)),
    ("ru", "послезавтра", datetime(2026, 11, 28, 15, 0)),
    ("pl", "przedwczoraj", datetime(2026, 11, 24, 15, 0)),
    ("pl", "pojutrze", datetime(2026, 11, 28, 15, 0)),
    ("ja", "一昨日", datetime(2026, 11, 24, 15, 0)),
    ("ja", "明後日", datetime(2026, 11, 28, 15, 0)),
    ("zh", "前天", datetime(2026, 11, 24, 15, 0)),
    ("zh", "后天", datetime(2026, 11, 28, 15, 0)),
    ("ko", "그제", datetime(2026, 11, 24, 15, 0)),
    ("ko", "모레", datetime(2026, 11, 28, 15, 0)),
    ("ar", "أول أمس", datetime(2026, 11, 24, 15, 0)),
    ("ar", "بعد غد", datetime(2026, 11, 28, 15, 0)),
    ("fa", "پریروز", datetime(2026, 11, 24, 15, 0)),
    ("fa", "پس‌فردا", datetime(2026, 11, 28, 15, 0)),
    ("he", "שלשום", datetime(2026, 11, 24, 15, 0)),
    ("he", "מחרתיים", datetime(2026, 11, 28, 15, 0)),
    ("el", "προχθές", datetime(2026, 11, 24, 15, 0)),
    ("el", "μεθαύριο", datetime(2026, 11, 28, 15, 0)),
    ("uk", "позавчора", datetime(2026, 11, 24, 15, 0)),
    ("uk", "післязавтра", datetime(2026, 11, 28, 15, 0)),
    ("ro", "alaltăieri", datetime(2026, 11, 24, 15, 0)),
    ("ro", "poimâine", datetime(2026, 11, 28, 15, 0)),
    ("hu", "tegnapelőtt", datetime(2026, 11, 24, 15, 0)),
    ("hu", "holnapután", datetime(2026, 11, 28, 15, 0)),
    ("cs", "předevčírem", datetime(2026, 11, 24, 15, 0)),
    ("cs", "pozítří", datetime(2026, 11, 28, 15, 0)),
    ("sv", "i förrgår", datetime(2026, 11, 24, 15, 0)),
    ("sv", "i övermorgon", datetime(2026, 11, 28, 15, 0)),
    ("fi", "toissapäivänä", datetime(2026, 11, 24, 15, 0)),
    ("fi", "ylihuomenna", datetime(2026, 11, 28, 15, 0)),
    # Post-positional & CJK/Korean relative weekdays
    ("fr", "vendredi dernier", datetime(2026, 11, 20, 0, 0)),
    ("fr", "lundi prochain", datetime(2026, 11, 30, 0, 0)),
    ("es", "viernes pasado", datetime(2026, 11, 20, 0, 0)),
    ("it", "venerdì scorso", datetime(2026, 11, 20, 0, 0)),
    ("it", "lunedì prossimo", datetime(2026, 11, 30, 0, 0)),
    ("pt", "sexta-feira passada", datetime(2026, 11, 20, 0, 0)),
    ("nl", "vorige vrijdag", datetime(2026, 11, 20, 0, 0)),
    ("nl", "volgende maandag", datetime(2026, 11, 30, 0, 0)),
    ("ko", "지난 금요일", datetime(2026, 11, 20, 0, 0)),
    ("ko", "다음 월요일", datetime(2026, 11, 30, 0, 0)),
    ("ja", "先週の金曜日", datetime(2026, 11, 20, 0, 0)),
    ("zh", "上周五", datetime(2026, 11, 20, 0, 0)),
    ("zh", "下周一", datetime(2026, 11, 30, 0, 0)),
    # Multi-word & post-positional prepositions across languages
    ("es", "dentro de 3 días", datetime(2026, 11, 29, 15, 0)),
    ("pt", "há 3 dias", datetime(2026, 11, 23, 15, 0)),
    ("pt", "daqui a 2 semanas", datetime(2026, 12, 10, 15, 0)),
    ("it", "3 giorni fa", datetime(2026, 11, 23, 15, 0)),
    ("it", "tra 2 settimane", datetime(2026, 12, 10, 15, 0)),
    ("nl", "over 2 weken", datetime(2026, 12, 10, 15, 0)),
    ("pl", "za 3 dni", datetime(2026, 11, 29, 15, 0)),
    ("sv", "om 3 dagar", datetime(2026, 11, 29, 15, 0)),
    ("hu", "3 napja", datetime(2026, 11, 23, 15, 0)),
    ("hu", "2 hét múlva", datetime(2026, 12, 10, 15, 0)),
    ("ro", "acum 3 zile", datetime(2026, 11, 23, 15, 0)),
    ("ro", "peste 2 săptămâni", datetime(2026, 12, 10, 15, 0)),
    ("el", "σε 3 ημέρες", datetime(2026, 11, 29, 15, 0)),
    ("he", "לפני 3 ימים", datetime(2026, 11, 23, 15, 0)),
    ("he", "בעוד 2 שבועות", datetime(2026, 12, 10, 15, 0)),
    ("fi", "2 viikon kuluttua", datetime(2026, 12, 10, 15, 0)),
]


def test_tier1_73_languages_complete():
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
    assert len(tier1) >= 73, f"Expected >= 73 Tier-1 languages, got {len(tier1)}"


@pytest.mark.parametrize("lang,text,expected", EXTREME_CASES)
def test_extreme_multilingual_auto_and_explicit(lang: str, text: str, expected: datetime):
    dt_explicit = dateoxide.parse(text, languages=[lang], settings=CFG)
    assert dt_explicit == expected, f"[{lang}] explicit {text!r}: {dt_explicit!r} != {expected!r}"

    dt_auto = dateoxide.parse(text, settings=CFG)
    assert dt_auto == expected, f"[{lang}] auto {text!r}: {dt_auto!r} != {expected!r}"
