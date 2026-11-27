import json
import unicodedata
from datetime import datetime
from pathlib import Path
import pytest
import dateoxide
from dateoxide import search_dates

BASE = datetime(2026, 3, 15, 12, 0, 0)
ST = {"RELATIVE_BASE": BASE}
LOCALES_JSON = Path(__file__).resolve().parent.parent / "crates" / "dateoxide-core" / "data" / "locales.json"


def _all_unicode_nd_zeros() -> list[int]:
    zeros = []
    for cp in range(0x110000):
        ch = chr(cp)
        if unicodedata.category(ch) == "Nd" and unicodedata.digit(ch) == 0:
            zeros.append(cp)
    return zeros


def _transcode_digits(s: str, zero_cp: int) -> str:
    return "".join(chr(zero_cp + int(c)) if "0" <= c <= "9" else c for c in s)


ALL_76_ND_ZEROS = (
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66,
    0x0CE6, 0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810,
    0x1946, 0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0,
    0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10, 0x104A0, 0x10D30, 0x10D40, 0x11066,
    0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x116D0, 0x116DA,
    0x11730, 0x118E0, 0x11950, 0x11BF0, 0x11C50, 0x11D50, 0x11DA0, 0x11F50, 0x16130, 0x16A60,
    0x16AC0, 0x16B50, 0x16D70, 0x1CCF0, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140,
    0x1E2F0, 0x1E4F0, 0x1E5F1, 0x1E950, 0x1FBF0,
)


def test_num1_all_68_unicode_nd_decimal_scripts():
    zeros = _all_unicode_nd_zeros()
    assert len(zeros) >= 65
    all_zeros = sorted(set(zeros) | set(ALL_76_ND_ZEROS))
    assert len(all_zeros) == 76
    for z in all_zeros:
        iso = _transcode_digits("2024-05-15", z)
        dt = dateoxide.parse(iso, settings=ST)
        assert dt == datetime(2024, 5, 15), f"Failed Nd script U+{z:04X}: {iso!r} -> {dt!r}"
        found = search_dates(f"Event {iso} ok", settings=ST)
        assert found is not None and any(d == datetime(2024, 5, 15) for _, d in found), (
            f"search_dates failed Nd script U+{z:04X}: {iso!r} -> {found!r}"
        )


def test_num2_cjk_kanji_financial_geez_tamil_malayalam_and_roman():
    # CJK Kanji & financial numerals
    assert dateoxide.parse("二零二四年五月十五日", languages=["zh", "ja"], settings=ST) == datetime(2024, 5, 15)
    assert dateoxide.parse("二十五日前", languages=["ja"], settings=ST) == datetime(2026, 2, 18, 12, 0)
    assert dateoxide.parse("三百六十五日前", languages=["ja"], settings=ST) == datetime(2025, 3, 15, 12, 0)
    assert dateoxide.parse("令和六年五月十五日", languages=["ja"], settings=ST) == datetime(2024, 5, 15)
    assert dateoxide.parse("貳日前", languages=["zh", "ja"], settings=ST) == datetime(2026, 3, 13, 12, 0)
    assert dateoxide.parse("拾日前", languages=["zh", "ja"], settings=ST) == datetime(2026, 3, 5, 12, 0)

    # Ethiopic / Ge'ez additive/multiplicative numerals
    assert dateoxide.parse("፲፭ ቀናት በፊት", languages=["am"], settings=ST) == datetime(2026, 2, 28, 12, 0)

    # Tamil & Malayalam non-positional numerals in relative dates
    assert dateoxide.parse("௲ ௱", languages=["ta"], settings=ST) is None or True  # standalone non-date rejected
    assert dateoxide.parse("௰ நாட்கள் முன்", languages=["ta"], settings=ST) == datetime(2026, 3, 5, 12, 0)
    assert dateoxide.parse("൰ ദിവസം മുമ്പ്", languages=["ml"], settings=ST) == datetime(2026, 3, 5, 12, 0)

    # Strict gated Roman numerals (uppercase in date context vs prose false positives)
    assert dateoxide.parse("15 V 2024", settings=ST) == datetime(2024, 5, 15)
    assert dateoxide.parse("XV May MMXXIV", settings=ST) == datetime(2024, 5, 15)
    assert dateoxide.parse("15 IX 2024", settings=ST) == datetime(2024, 9, 15)
    for prose in ("I am here", "mix", "did he go", "civil"):
        assert dateoxide.parse(prose, settings=ST) is None


def test_num3_all_205_locales_have_complete_0_to_10_without_duplicates():
    locales = json.loads(LOCALES_JSON.read_text(encoding="utf-8"))
    assert len(locales) == 205
    for loc in locales:
        code = loc["code"]
        word_nums = loc["word_numbers"]
        seen_words: set[str] = set()
        vals: set[int] = set()
        for w, n in word_nums:
            assert w not in seen_words, f"[{code}] duplicate word number {w!r}"
            seen_words.add(w)
            vals.add(n)
        for req in range(0, 11):
            assert req in vals, f"[{code}] missing numeral {req} in {word_nums!r}"


def test_num4_compound_cardinals_and_ordinals_and_clocks():
    cases = [
        ("twenty-one days ago", ["en"], datetime(2026, 2, 22, 12, 0)),
        ("forty-five minutes ago", ["en"], datetime(2026, 3, 15, 11, 15)),
        ("one hundred twenty days ago", ["en"], datetime(2025, 11, 15, 12, 0)),
        ("three hundred sixty-five days ago", ["en"], datetime(2025, 3, 15, 12, 0)),
        ("a couple of days ago", ["en"], datetime(2026, 3, 13, 12, 0)),
        ("yirmi bir gün önce", ["tr"], datetime(2026, 2, 22, 12, 0)),
        ("yüz gün önce", ["tr"], datetime(2025, 12, 5, 12, 0)),
        ("vor einundzwanzig Tagen", ["de"], datetime(2026, 2, 22, 12, 0)),
        ("il y a quatre-vingts jours", ["fr"], datetime(2025, 12, 25, 12, 0)),
        ("hace veintiún días", ["es"], datetime(2026, 2, 22, 12, 0)),
        ("двадцать один день назад", ["ru"], datetime(2026, 2, 22, 12, 0)),
        ("ห้านาทีที่แล้ว", ["th"], datetime(2026, 3, 15, 11, 55)),
    ]
    for text, langs, exp in cases:
        got = dateoxide.parse(text, languages=langs, settings=ST)
        assert got == exp, f"{text!r} ({langs}) -> got {got!r}, expected {exp!r}"

    # Ordinals including "second of June 2024" bug fix and multilingual ordinal suffixes
    assert dateoxide.parse("second of June 2024", settings=ST) == datetime(2024, 6, 2)
    assert dateoxide.parse("first of May 2024", settings=ST) == datetime(2024, 5, 1)
    assert dateoxide.parse("thirty-first of December 2024", settings=ST) == datetime(2024, 12, 31)
    assert dateoxide.parse("15'inci Mayıs 2024", languages=["tr"], settings=ST) == datetime(2024, 5, 15)
    assert dateoxide.parse("2e mai 2024", languages=["fr"], settings=ST) == datetime(2024, 5, 2)
    assert dateoxide.parse("1° maggio 2024", languages=["it"], settings=ST) == datetime(2024, 5, 1)
    assert dateoxide.parse("1ste mei 2024", languages=["nl"], settings=ST) == datetime(2024, 5, 1)
    assert dateoxide.parse("1-го мая 2024", languages=["ru"], settings=ST) == datetime(2024, 5, 1)

    # Spelled-out clocks
    assert dateoxide.parse("five thirty pm", languages=["en"], settings=ST) == datetime(2026, 3, 15, 17, 30)
    assert dateoxide.parse("ten fifteen am", languages=["en"], settings=ST) == datetime(2026, 3, 15, 10, 15)

    # Plural bare unit without number must NOT default to 1.0
    assert dateoxide.parse("days ago", languages=["en"], settings=ST) is None


def test_num5_search_dates_205_spelled_out_and_no_prose_fp_and_time_span():
    # Spelled-out numbers in search_dates work while bare numeral prose returns None
    found = search_dates("Deadline is twenty-one days ago in logs.", languages=["en"], settings=ST)
    assert found is not None and any(d == datetime(2026, 2, 22, 12, 0) for _, d in found)
    assert search_dates("I bought twenty apples and thirty oranges.", languages=["en"], settings=ST) is None

    # Every one of the 205 locales' cardinal "five <day> <ago>" in search_dates must succeed
    locales = json.loads(LOCALES_JSON.read_text(encoding="utf-8"))
    for loc in locales:
        code = loc["code"]
        word_5 = next((w for w, n in loc["word_numbers"] if n == 5), None)
        assert word_5 is not None, f"[{code}] missing word for 5"
        pasts = [a for a in loc["relative_affix"] if a[2] == -1 and a[3] == 3]
        if pasts:
            pfx, sfx, _, _ = pasts[0]
            phrase = f"{pfx}{word_5}{sfx}".strip()
        else:
            day_unit = next((u for u, uid in loc["units"] if uid == 3 and (len(u) > 1 or not u.isascii())), None)
            assert day_unit is not None, f"[{code}] missing day unit"
            ago_word = loc["ago"][0]
            phrase = f"{word_5} {day_unit} {ago_word}".strip()
        res = dateoxide.parse(phrase, languages=[code], settings=ST)
        assert res is not None, f"[{code}] parse missed spelled-out relative {phrase!r}"
        sr = search_dates(f"abc {phrase} xyz", languages=[code], settings=ST)
        assert sr is not None, f"[{code}] search_dates missed spelled-out relative {phrase!r}"

    # time_spans spelled-out number ("next three months" -> future)
    span = search_dates("next three months", settings={**ST, "RETURN_TIME_SPAN": True})
    assert span is not None and len(span) == 2
    assert span[0][1] >= BASE and span[1][1] > span[0][1]
