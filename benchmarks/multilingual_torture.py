"""Standalone extreme multilingual comparison: dateoxide vs dateparser across 87 hard cases."""
from __future__ import annotations

from datetime import datetime
import time

import dateoxide
import dateparser

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)

EXTREME_CASES: list[tuple[str, str, datetime]] = [
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


def _strict_eq(got: datetime | None, exp: datetime) -> bool:
    return got is not None and got.tzinfo is None and got == exp


def main() -> None:
    cfg = {"RELATIVE_BASE": FIXED_BASE}
    for w in ["3 gün önce", "12 December 2025", "١٢ ديسمبر ٢٠٢٥", "前天"]:
        dateoxide.parse(w, settings=cfg)
        dateparser.parse(w, settings=cfg)

    t0 = time.perf_counter()
    ox_auto = sum(1 for _, txt, exp in EXTREME_CASES if _strict_eq(dateoxide.parse(txt, settings=cfg), exp))
    t_ox_auto = (time.perf_counter() - t0) * 1000.0

    t0 = time.perf_counter()
    ox_exp = sum(
        1 for lang, txt, exp in EXTREME_CASES if _strict_eq(dateoxide.parse(txt, languages=[lang], settings=cfg), exp)
    )
    t_ox_exp = (time.perf_counter() - t0) * 1000.0

    t0 = time.perf_counter()
    dp_exp = sum(
        1 for lang, txt, exp in EXTREME_CASES if _strict_eq(dateparser.parse(txt, languages=[lang], settings=cfg), exp)
    )
    t_dp_exp = (time.perf_counter() - t0) * 1000.0

    n = len(EXTREME_CASES)
    print("=" * 72)
    print(f"EXTREME MULTILINGUAL BENCHMARK ({n} cases across 30+ languages & 10+ scripts)")
    print("=" * 72)
    print(f"dateoxide (auto-detect)    : {ox_auto}/{n} ({t_ox_auto:.2f} ms)")
    print(f"dateoxide (explicit lang)  : {ox_exp}/{n} ({t_ox_exp:.2f} ms)")
    print(f"dateparser (explicit lang) : {dp_exp}/{n} ({t_dp_exp:.2f} ms)")


if __name__ == "__main__":
    main()
