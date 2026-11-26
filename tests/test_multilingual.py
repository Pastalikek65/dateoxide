from __future__ import annotations

import json
from datetime import datetime
from pathlib import Path
import pytest
import dateoxide

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)
LOCALES_JSON = Path(__file__).resolve().parent.parent / "crates" / "dateoxide-core" / "data" / "locales.json"


@pytest.mark.parametrize(
    "lang,text,expected",
    [
        # Turkish (native + diacritic-folded)
        ("tr", "bugün", (2026, 11, 26)),
        ("tr", "dün", (2026, 11, 25)),
        ("tr", "yarın", (2026, 11, 27)),
        ("tr", "yarin", (2026, 11, 27)),
        ("tr", "3 gün önce", (2026, 11, 23)),
        ("tr", "3 gun once", (2026, 11, 23)),
        ("tr", "2 hafta sonra", (2026, 12, 10)),
        ("tr", "bir saat önce", (2026, 11, 26)),
        ("tr", "5 dk önce", (2026, 11, 26)),
        ("tr", "10 sn sonra", (2026, 11, 26)),
        ("tr", "geçen hafta", (2026, 11, 19)),
        ("tr", "gecen hafta", (2026, 11, 19)),
        ("tr", "gelecek ay", (2026, 12, 26)),
        ("tr", "2 ay 3 gün önce", (2026, 9, 23)),
        ("tr", "12 Aralık 2025", (2025, 12, 12)),
        ("tr", "12 Aralik 2025", (2025, 12, 12)),
        ("tr", "15 Subat 2026", (2026, 2, 15)),
        ("tr", "20 Agustos 2025", (2025, 8, 20)),
        ("tr", "15 Ekim 2025 Çarşamba 14:30", (2025, 10, 15)),
        ("tr", "dün saat 18:30", (2026, 11, 25)),
        ("tr", "dun saat 14:30", (2026, 11, 25)),
        # English
        ("en", "3 days ago", (2026, 11, 23)),
        ("en", "3d ago", (2026, 11, 23)),
        ("en", "2h ago", (2026, 11, 26)),
        ("en", "2w ago", (2026, 11, 12)),
        ("en", "in 2 weeks", (2026, 12, 10)),
        ("en", "two days ago", (2026, 11, 24)),
        ("en", "1 year, 2 months, 3 days ago", (2025, 9, 23)),
        ("en", "yesterday at 5pm", (2026, 11, 25)),
        # German
        ("de", "vor 2 Wochen", (2026, 11, 12)),
        ("de", "in 3 Tagen", (2026, 11, 29)),
        ("de", "gestern", (2026, 11, 25)),
        ("de", "morgen", (2026, 11, 27)),
        ("de", "26. November 2026", (2026, 11, 26)),
        ("de", "15. März 2025", (2025, 3, 15)),
        ("de", "15 Marz 2025", (2025, 3, 15)),
        # French
        ("fr", "hier à 14h30", (2026, 11, 25)),
        ("fr", "demain", (2026, 11, 27)),
        ("fr", "dans 2 heures", (2026, 11, 26)),
        ("fr", "il y a 4 jours", (2026, 11, 22)),
        ("fr", "12 fevrier 2025", (2025, 2, 12)),
        ("fr", "15 aout 2025", (2025, 8, 15)),
        ("fr", "10 decembre 2025", (2025, 12, 10)),
        ("fr", "le 14 juillet 1789", (1789, 7, 14)),
        # Spanish
        ("es", "ayer", (2026, 11, 25)),
        ("es", "mañana", (2026, 11, 27)),
        ("es", "manana", (2026, 11, 27)),
        ("es", "hace 3 semanas", (2026, 11, 5)),
        ("es", "en 2 semanas", (2026, 12, 10)),
        ("es", "12 de diciembre de 2025", (2025, 12, 12)),
        # Italian & Portuguese
        ("it", "ieri", (2026, 11, 25)),
        ("it", "3 giorni fa", (2026, 11, 23)),
        ("it", "tra 2 settimane", (2026, 12, 10)),
        ("it", "15 ottobre 2025", (2025, 10, 15)),
        ("pt", "ontem", (2026, 11, 25)),
        ("pt", "amanhã", (2026, 11, 27)),
        ("pt", "amanha", (2026, 11, 27)),
        ("pt", "há 3 dias", (2026, 11, 23)),
        ("pt", "10 marco 2025", (2025, 3, 10)),
        ("pt", "10 de maio de 2025", (2025, 5, 10)),
        # Russian & Ukrainian
        ("ru", "вчера", (2026, 11, 25)),
        ("ru", "3 дня назад", (2026, 11, 23)),
        ("ru", "через 2 недели", (2026, 12, 10)),
        ("ru", "26 ноября 2026", (2026, 11, 26)),
        ("uk", "3 дні тому", (2026, 11, 23)),
        ("uk", "завтра", (2026, 11, 27)),
        # Japanese, Chinese, Korean
        ("ja", "3日前", (2026, 11, 23)),
        ("ja", "3日後", (2026, 11, 29)),
        ("ja", "2026年11月26日", (2026, 11, 26)),
        ("zh", "3天前", (2026, 11, 23)),
        ("zh", "2025年8月15日", (2025, 8, 15)),
        ("ko", "2026년 11월 26일", (2026, 11, 26)),
        ("ko", "3일 전", (2026, 11, 23)),
        ("ko", "어제", (2026, 11, 25)),
        # Arabic & Persian
        ("ar", "منذ 3 أيام", (2026, 11, 23)),
        ("ar", "غدا", (2026, 11, 27)),
        ("ar", "١٢ ديسمبر ٢٠٢٥", (2025, 12, 12)),
        ("fa", "دیروز", (2026, 11, 25)),
        ("fa", "۳ روز پیش", (2026, 11, 23)),
        # Polish, Czech, Dutch, Swedish, Greek, Hebrew, Hindi, Indonesian, Vietnamese, Thai
        ("pl", "2 dni temu", (2026, 11, 24)),
        ("pl", "15 maja 2025", (2025, 5, 15)),
        ("cs", "před 3 dny", (2026, 11, 23)),
        ("nl", "3 dagen geleden", (2026, 11, 23)),
        ("sv", "3 dagar sedan", (2026, 11, 23)),
        ("el", "πριν από 3 ημέρες", (2026, 11, 23)),
        ("he", "אתמול", (2026, 11, 25)),
        ("hi", "3 दिन पहले", (2026, 11, 23)),
        ("id", "3 hari yang lalu", (2026, 11, 23)),
        ("vi", "3 ngày trước", (2026, 11, 23)),
        ("th", "เมื่อวาน", (2026, 11, 25)),
    ],
)
def test_multilingual_auto_and_explicit(lang: str, text: str, expected: tuple[int, int, int]):
    dt_auto = dateoxide.parse(text, settings={"RELATIVE_BASE": FIXED_BASE})
    assert dt_auto is not None, f"[{lang}] auto-detect failed for '{text}'"
    assert (dt_auto.year, dt_auto.month, dt_auto.day) == expected

    dt_hint = dateoxide.parse(text, languages=[lang], settings={"RELATIVE_BASE": FIXED_BASE})
    assert dt_hint is not None, f"[{lang}] explicit lang failed for '{text}'"
    assert (dt_hint.year, dt_hint.month, dt_hint.day) == expected


def test_all_205_languages_smoke():
    langs = dateoxide.supported_languages()
    assert len(langs) == 205
    with LOCALES_JSON.open(encoding="utf-8") as f:
        locales = json.load(f)

    settings = {"RELATIVE_BASE": FIXED_BASE}
    for loc in locales:
        code = loc["code"]
        # 1. Verify month parsing
        for m_idx, names in enumerate(loc["months"]):
            if names:
                expr = f"15 {names[0]} 2025"
                dt = dateoxide.parse(expr, languages=[code], settings=settings)
                assert dt is not None and (dt.year, dt.month, dt.day) == (2025, m_idx + 1, 15), (
                    f"[{code}] month failed: {expr!r} -> {dt}"
                )
                break
        # 2. Verify exact relative phrase
        if loc["relative_exact"]:
            phrase = loc["relative_exact"][0][0]
            assert dateoxide.parse(phrase, languages=[code], settings=settings) is not None, (
                f"[{code}] rel_exact failed: {phrase!r}"
            )
        # 3. Verify relative affix pattern
        if loc["relative_affix"]:
            pref, suff, _, _ = loc["relative_affix"][0]
            expr = f"{pref}3{suff}"
            assert dateoxide.parse(expr, languages=[code], settings=settings) is not None, (
                f"[{code}] rel_affix failed: {expr!r}"
            )
