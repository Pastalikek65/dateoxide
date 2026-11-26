"""Compile scrapinghub/dateparser's 205 locale definitions into a compact JSON
source consumed by dateoxide-core's build.rs (which serializes it to postcard).

Attribution: Lexical data is derived from Unicode CLDR and scrapinghub/dateparser
(BSD-3-Clause license; see NOTICE).
"""

from __future__ import annotations

import glob
import importlib
import json
import os
import re
from pathlib import Path
from typing import Any

from dateparser.data import date_translation_data

MONTH_KEYS = [
    "january", "february", "march", "april", "may", "june",
    "july", "august", "september", "october", "november", "december",
]
WEEKDAY_KEYS = [
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
]
UNIT_MAP = {
    "year": 0,
    "month": 1,
    "week": 2,
    "day": 3,
    "hour": 4,
    "minute": 5,
    "second": 6,
    "decade": 7,
    "century": 8,
    "fortnight": 9,
}

# Match structured numeric capture patterns in relative-type-regex
PAT_NUM = re.compile(r"^(.*?)\(\\d\+\+(?:\[\.,\]\?\\d\*\+)?\)(.*)$")
PAT_NUM_SIMPLE = re.compile(r"^(.*?)\(\\d\+\)(.*)$")

# Parse target specs like "1 day ago", "in 2 week", "\\1 hour ago", "in \\1 year"
PAT_TARGET_EXACT = re.compile(
    r"^(?:(?P<in>in)\s+)?(?P<num>\d+)\s+(?P<unit>[a-z]+)(?:\s+(?P<ago>ago))?$"
)
PAT_TARGET_REGEX = re.compile(
    r"^(?:(?P<in>in)\s+)?\\1\s+(?P<unit>[a-z]+)(?:\s+(?P<ago>ago))?$"
)

# Extra high-value word numbers for major languages where dateparser omits them
EXTRA_WORD_NUMBERS: dict[str, dict[str, int]] = {
    "en": {
        "a": 1, "an": 1, "one": 1, "two": 2, "three": 3, "four": 4, "five": 5,
        "six": 6, "seven": 7, "eight": 8, "nine": 9, "ten": 10, "eleven": 11,
        "twelve": 12, "fifteen": 15, "twenty": 20, "thirty": 30, "couple": 2,
    },
    "tr": {
        "bir": 1, "iki": 2, "üç": 3, "dört": 4, "beş": 5, "altı": 6,
        "yedi": 7, "sekiz": 8, "dokuz": 9, "on": 10, "on iki": 12, "yirmi": 20,
    },
    "es": {
        "un": 1, "una": 1, "uno": 1, "dos": 2, "tres": 3, "cuatro": 4, "cinco": 5,
        "seis": 6, "siete": 7, "ocho": 8, "nueve": 9, "diez": 10, "doce": 12,
    },
    "fr": {
        "un": 1, "une": 1, "deux": 2, "trois": 3, "quatre": 4, "cinq": 5,
        "six": 6, "sept": 7, "huit": 8, "neuf": 9, "dix": 10, "douze": 12,
    },
    "de": {
        "ein": 1, "eine": 1, "einem": 1, "einen": 1, "einer": 1,
        "zwei": 2, "drei": 3, "vier": 4, "fünf": 5, "sechs": 6,
        "sieben": 7, "acht": 8, "neun": 9, "zehn": 10, "elf": 11, "zwölf": 12,
    },
    "pt": {
        "um": 1, "uma": 1, "dois": 2, "duas": 2, "três": 3, "quatro": 4, "cinco": 5,
        "seis": 6, "sete": 7, "oito": 8, "nove": 9, "dez": 10,
    },
    "it": {
        "un": 1, "uno": 1, "una": 1, "due": 2, "tre": 3, "quattro": 4, "cinque": 5,
        "sei": 6, "sette": 7, "otto": 8, "nove": 9, "dieci": 10,
    },
    "nl": {
        "een": 1, "twee": 2, "drie": 3, "vier": 4, "vijf": 5, "zes": 6,
        "zeven": 7, "acht": 8, "negen": 9, "tien": 10,
    },
    "ru": {
        "один": 1, "одна": 1, "одну": 1, "два": 2, "две": 2, "три": 3,
        "четыре": 4, "пять": 5, "шесть": 6, "семь": 7, "восемь": 8, "девять": 9, "десять": 10,
    },
}

# Priority ordering: highest-traffic global languages come first so ambiguous
# single-token matches resolve predictably and match dateparser conventions.
PRIORITY_LANGS = [
    "en", "es", "fr", "de", "pt", "it", "tr", "nl", "ru", "pl",
    "ja", "zh", "ko", "ar", "hi", "id", "vi", "th", "sv", "da",
    "nb", "fi", "cs", "ro", "hu", "uk", "el", "he", "fa", "ca",
]


def norm(s: str) -> str:
    return " ".join(s.strip().lower().split())


def clean_regex_literal(s: str) -> str:
    """Turn simple regex literal fragments from dateparser into plain lowercase strings."""
    s = s.replace("\\.", ".").replace("\\-", "-").replace("\\s+", " ").replace("\\s*", " ")
    s = s.replace("\\(", "(").replace("\\)", ")").replace("\\+", "+")
    # Remove simple non-capturing boundary/regex escapes if any remain
    s = re.sub(r"\\b", "", s)
    return s.lower()


def dedup_list(items: list[str]) -> list[str]:
    seen: set[str] = set()
    out: list[str] = []
    for item in items:
        n = norm(item)
        if not n:
            continue
        if n not in seen:
            seen.add(n)
            out.append(n)
        # Also add dot-stripped form for abbreviations like "jan." -> "jan"
        if n.endswith(".") and len(n) > 2:
            stripped = n[:-1].strip()
            if stripped and stripped not in seen:
                seen.add(stripped)
                out.append(stripped)
    return out


def extract_locale(code: str, info: dict[str, Any]) -> dict[str, Any]:
    date_order = str(info.get("date_order", "DMY")).upper()
    if date_order not in ("DMY", "MDY", "YMD", "YDM", "MYD", "DYM"):
        date_order = "DMY"

    months = [dedup_list(info.get(k, [])) for k in MONTH_KEYS]
    weekdays = [dedup_list(info.get(k, [])) for k in WEEKDAY_KEYS]
    am = dedup_list(info.get("am", []))
    pm = dedup_list(info.get("pm", []))

    units: list[list[Any]] = []
    seen_units: set[tuple[str, int]] = set()
    for unit_name, unit_id in UNIT_MAP.items():
        for tok in dedup_list(info.get(unit_name, [])):
            key = (tok, unit_id)
            if key not in seen_units:
                seen_units.add(key)
                units.append([tok, unit_id])

    ago = dedup_list(info.get("ago", []))
    in_words = dedup_list(info.get("in", []))
    skip = dedup_list(list(info.get("skip", [])) + list(info.get("pertain", [])))

    # Relative exact phrases: [phrase, sign (-1 past / +1 future), amount, unit_id]
    relative_exact: list[list[Any]] = []
    seen_rel_exact: set[str] = set()
    for target, phrases in info.get("relative-type", {}).items():
        m = PAT_TARGET_EXACT.match(target.strip().lower())
        if not m:
            continue
        unit_id = UNIT_MAP.get(m.group("unit"))
        if unit_id is None:
            continue
        amount = int(m.group("num"))
        sign = 1 if m.group("in") else -1
        for phrase in dedup_list(phrases):
            if phrase not in seen_rel_exact:
                seen_rel_exact.add(phrase)
                relative_exact.append([phrase, sign, amount, unit_id])

    # Relative affix patterns: [prefix, suffix, sign, unit_id]
    relative_affix: list[list[Any]] = []
    seen_affix: set[tuple[str, str, int, int]] = set()
    for target, patterns in info.get("relative-type-regex", {}).items():
        m_t = PAT_TARGET_REGEX.match(target.strip().lower())
        if not m_t:
            continue
        unit_id = UNIT_MAP.get(m_t.group("unit"))
        if unit_id is None:
            continue
        sign = 1 if m_t.group("in") else -1
        for pat in patterns:
            m_p = PAT_NUM.match(pat) or PAT_NUM_SIMPLE.match(pat)
            if not m_p:
                continue
            prefix = clean_regex_literal(m_p.group(1)).lstrip()
            suffix = clean_regex_literal(m_p.group(2)).rstrip()
            # Skip patterns that still contain complex regex metacharacters
            if any(ch in prefix + suffix for ch in "[](){}|?*^$\\"):
                continue
            key = (prefix, suffix, sign, unit_id)
            if key not in seen_affix and (prefix or suffix):
                seen_affix.add(key)
                relative_affix.append([prefix, suffix, sign, unit_id])

    # Word numbers from simplifications + curated extras
    word_numbers: dict[str, int] = dict(EXTRA_WORD_NUMBERS.get(code, {}))
    for simp_dict in info.get("simplifications", []):
        for k, v in simp_dict.items():
            v_str = str(v).strip()
            if v_str.isdigit():
                k_clean = norm(str(k))
                if k_clean and not any(ch in k_clean for ch in "[](){}|?*^$\\+"):
                    word_numbers.setdefault(k_clean, int(v_str))

    return {
        "code": code,
        "date_order": date_order,
        "months": months,
        "weekdays": weekdays,
        "am": am,
        "pm": pm,
        "units": units,
        "ago": ago,
        "in_words": in_words,
        "skip": skip,
        "relative_exact": relative_exact,
        "relative_affix": relative_affix,
        "word_numbers": sorted([[k, v] for k, v in word_numbers.items()]),
    }


def main() -> None:
    data_dir = os.path.dirname(date_translation_data.__file__)
    all_codes = sorted(
        os.path.splitext(os.path.basename(f))[0]
        for f in glob.glob(os.path.join(data_dir, "*.py"))
        if not f.endswith("__init__.py")
    )

    priority_set = set(PRIORITY_LANGS)
    ordered_codes = [c for c in PRIORITY_LANGS if c in set(all_codes)] + [
        c for c in all_codes if c not in priority_set
    ]

    locales = []
    for code in ordered_codes:
        mod = importlib.import_module(f"dateparser.data.date_translation_data.{code}")
        locales.append(extract_locale(code, mod.info))

    out_path = Path(__file__).resolve().parent.parent / "crates" / "dateoxide-core" / "data" / "locales.json"
    out_path.parent.mkdir(parents=True, exist_ok=True)
    with out_path.open("w", encoding="utf-8") as f:
        json.dump(locales, f, ensure_ascii=False, separators=(",", ":"))

    size_kb = out_path.stat().st_size / 1024
    total_affixes = sum(len(l["relative_affix"]) for l in locales)
    total_exact = sum(len(l["relative_exact"]) for l in locales)
    print(
        f"Wrote {len(locales)} locales to {out_path} ({size_kb:.1f} KB), "
        f"relative_exact={total_exact}, relative_affix={total_affixes}"
    )


if __name__ == "__main__":
    main()
