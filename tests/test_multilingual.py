from __future__ import annotations

from datetime import datetime
import pytest
import dateoxide

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


@pytest.mark.parametrize(
    "lang,text,expected",
    [
        # Turkish
        ("tr", "bugün", (2026, 11, 26)),
        ("tr", "dün", (2026, 11, 25)),
        ("tr", "yarın", (2026, 11, 27)),
        ("tr", "3 gün önce", (2026, 11, 23)),
        ("tr", "2 hafta sonra", (2026, 12, 10)),
        ("tr", "bir saat önce", (2026, 11, 26)),
        ("tr", "12 Aralık 2025", (2025, 12, 12)),
        ("tr", "15 Ocak 2026", (2026, 1, 15)),
        ("tr", "dün saat 18:30", (2026, 11, 25)),
        # English
        ("en", "3 days ago", (2026, 11, 23)),
        ("en", "in 2 weeks", (2026, 12, 10)),
        ("en", "two days ago", (2026, 11, 24)),
        ("en", "1 year, 2 months, 3 days ago", (2025, 9, 23)),
        ("en", "yesterday at 5pm", (2026, 11, 25)),
        # German
        ("de", "vor 2 Wochen", (2026, 11, 12)),
        ("de", "gestern", (2026, 11, 25)),
        ("de", "morgen", (2026, 11, 27)),
        ("de", "26. November 2026", (2026, 11, 26)),
        ("de", "15. März 2025", (2025, 3, 15)),
        # French
        ("fr", "hier à 14h30", (2026, 11, 25)),
        ("fr", "demain", (2026, 11, 27)),
        ("fr", "il y a 4 jours", (2026, 11, 22)),
        ("fr", "le 14 juillet 1789", (1789, 7, 14)),
        # Spanish
        ("es", "ayer", (2026, 11, 25)),
        ("es", "mañana", (2026, 11, 27)),
        ("es", "hace 3 semanas", (2026, 11, 5)),
        ("es", "12 de diciembre de 2025", (2025, 12, 12)),
        # Italian
        ("it", "ieri", (2026, 11, 25)),
        ("it", "domani", (2026, 11, 27)),
        ("it", "15 ottobre 2025", (2025, 10, 15)),
        # Portuguese
        ("pt", "ontem", (2026, 11, 25)),
        ("pt", "amanhã", (2026, 11, 27)),
        ("pt", "10 de maio de 2025", (2025, 5, 10)),
        # Russian
        ("ru", "вчера", (2026, 11, 25)),
        ("ru", "завтра", (2026, 11, 27)),
        ("ru", "26 ноября 2026", (2026, 11, 26)),
        # Japanese & Chinese
        ("ja", "3日前", (2026, 11, 23)),
        ("ja", "2026年11月26日", (2026, 11, 26)),
        ("zh", "2025年8月15日", (2025, 8, 15)),
        # Dutch & Polish
        ("nl", "gisteren", (2026, 11, 25)),
        ("pl", "wczoraj", (2026, 11, 25)),
    ],
)
def test_multilingual_auto_and_explicit(lang: str, text: str, expected: tuple[int, int, int]):
    # Auto-detect language
    dt_auto = dateoxide.parse(text, settings={"RELATIVE_BASE": FIXED_BASE})
    assert dt_auto is not None, f"[{lang}] auto-detect failed for '{text}'"
    assert (dt_auto.year, dt_auto.month, dt_auto.day) == expected

    # Explicit language hint
    dt_hint = dateoxide.parse(text, languages=[lang], settings={"RELATIVE_BASE": FIXED_BASE})
    assert dt_hint is not None, f"[{lang}] explicit lang failed for '{text}'"
    assert (dt_hint.year, dt_hint.month, dt_hint.day) == expected


def test_205_languages_loaded():
    langs = dateoxide.supported_languages()
    assert len(langs) == 205
    for code in ["en", "tr", "de", "fr", "es", "ru", "ja", "zh", "ar", "hi", "ko", "pt", "it"]:
        assert code in langs
