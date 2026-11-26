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

# Standard dateparser time-of-day hour anchors
TOD_KEYS: list[tuple[str, int]] = [
    ("early_morning", 6),
    ("morning", 9),
    ("afternoon", 15),
    ("evening", 18),
    ("night", 20),
]

EXTRA_TOD: dict[str, list[tuple[str, int]]] = {
    "en": [("noon", 12), ("midday", 12), ("midnight", 0), ("morning", 9), ("afternoon", 15), ("evening", 18), ("night", 20)],
    "tr": [("sabah", 9), ("öğle", 12), ("öğlen", 12), ("öğleden sonra", 15), ("akşam", 18), ("gece", 20), ("gece yarısı", 0)],
    "de": [("morgen", 9), ("morgens", 9), ("mittag", 12), ("mittags", 12), ("nachmittag", 15), ("abend", 18), ("abends", 18), ("nacht", 20), ("mitternacht", 0)],
    "fr": [("matin", 9), ("midi", 12), ("après-midi", 15), ("soir", 18), ("nuit", 20), ("minuit", 0)],
    "es": [("mañana", 9), ("mediodía", 12), ("tarde", 15), ("noche", 20), ("medianoche", 0)],
    "pt": [("manhã", 9), ("meio-dia", 12), ("tarde", 15), ("noite", 20), ("meia-noite", 0)],
    "it": [("mattina", 9), ("mezzogiorno", 12), ("pomeriggio", 15), ("sera", 18), ("notte", 20), ("mezzanotte", 0)],
    "nl": [("ochtend", 9), ("middag", 15), ("avond", 18), ("nacht", 20), ("middernacht", 0)],
    "ru": [("утром", 9), ("утро", 9), ("полдень", 12), ("днём", 15), ("вечером", 18), ("вечер", 18), ("ночью", 20), ("полночь", 0)],
    "pl": [("rano", 9), ("południe", 12), ("popołudnie", 15), ("wieczorem", 18), ("noc", 20), ("północ", 0)],
    "uk": [("вранці", 9), ("ранок", 9), ("опівдні", 12), ("вдень", 15), ("увечері", 18), ("вечір", 18), ("вночі", 20), ("опівночі", 0)],
    "cs": [("ráno", 9), ("poledne", 12), ("odpoledne", 15), ("večer", 18), ("noc", 20), ("půlnoc", 0)],
    "sk": [("ráno", 9), ("poludnie", 12), ("popoludní", 15), ("večer", 18), ("noc", 20), ("polnoc", 0)],
    "sv": [("morgon", 9), ("middag", 12), ("eftermiddag", 15), ("kväll", 18), ("natt", 20), ("midnatt", 0)],
    "da": [("morgen", 9), ("middag", 12), ("eftermiddag", 15), ("aften", 18), ("nat", 20), ("midnat", 0)],
    "nb": [("morgen", 9), ("middag", 12), ("ettermiddag", 15), ("kveld", 18), ("natt", 20), ("midnatt", 0)],
    "fi": [("aamu", 9), ("aamulla", 9), ("keskipäivä", 12), ("iltapäivä", 15), ("ilta", 18), ("illalla", 18), ("yö", 20), ("keskiyö", 0)],
    "ro": [("dimineață", 9), ("amiază", 12), ("după-amiază", 15), ("seară", 18), ("noapte", 20), ("miezul nopții", 0)],
    "hu": [("reggel", 9), ("dél", 12), ("délután", 15), ("este", 18), ("éjszaka", 20), ("éjfél", 0)],
    "el": [("πρωί", 9), ("μεσημέρι", 12), ("απόγευμα", 15), ("βράδυ", 18), ("νύχτα", 20), ("μεσάνυχτα", 0)],
    "he": [("בוקר", 9), ("בבוקר", 9), ("צהריים", 12), ("אחר הצהריים", 15), ("ערב", 18), ("בערב", 18), ("לילה", 20), ("חצות", 0)],
    "ar": [("صباحا", 9), ("الصباح", 9), ("ظهرا", 12), ("عصرا", 15), ("مساء", 18), ("المساء", 18), ("ليلا", 20), ("منتصف الليل", 0)],
    "fa": [("صبح", 9), ("ظهر", 12), ("بعدازظهر", 15), ("عصر", 18), ("شب", 20), ("نیمه‌شب", 0)],
    "ur": [("صبح", 9), ("دوپہر", 12), ("سہ پہر", 15), ("شام", 18), ("رات", 20), ("آدھی رات", 0)],
    "hi": [("सुबह", 9), ("दोपहर", 12), ("तीसरा पहर", 15), ("शाम", 18), ("रात", 20), ("आधी रात", 0)],
    "bn": [("সকাল", 9), ("দুপুর", 12), ("বিকাল", 15), ("সন্ধ্যা", 18), ("রাত", 20), ("মধ্যরাত", 0)],
    "id": [("pagi", 9), ("siang", 12), ("sore", 15), ("malam", 20), ("tengah malam", 0)],
    "ms": [("pagi", 9), ("tengah hari", 12), ("petang", 15), ("malam", 20), ("tengah malam", 0)],
    "vi": [("sáng", 9), ("trưa", 12), ("chiều", 15), ("tối", 18), ("đêm", 20), ("nửa đêm", 0)],
    "th": [("เช้า", 9), ("เที่ยง", 12), ("บ่าย", 15), ("เย็น", 18), ("กลางคืน", 20), ("เที่ยงคืน", 0)],
    "zh": [("早上", 9), ("上午", 9), ("中午", 12), ("下午", 15), ("傍晚", 18), ("晚上", 20), ("午夜", 0)],
    "ja": [("朝", 9), ("午前", 9), ("正午", 12), ("午後", 15), ("夕方", 18), ("夜", 20), ("真夜中", 0)],
    "ko": [("아침", 9), ("오전", 9), ("정오", 12), ("오후", 15), ("저녁", 18), ("밤", 20), ("자정", 0)],
    "az": [("səhər", 9), ("günorta", 12), ("axşam", 18), ("gecə", 20), ("gecəyarısı", 0)],
    "uz": [("ertalab", 9), ("peshin", 12), ("kechqurun", 18), ("tun", 20), ("yarim tun", 0)],
    "kk": [("таңертең", 9), ("түс", 12), ("кеш", 18), ("түн", 20), ("түн ортасы", 0)],
    "bg": [("сутрин", 9), ("обяд", 12), ("следобед", 15), ("вечер", 18), ("нощ", 20), ("полунощ", 0)],
    "hr": [("ujutro", 9), ("podne", 12), ("poslijepodne", 15), ("navečer", 18), ("noću", 20), ("ponoć", 0)],
    "sr": [("ујутру", 9), ("подне", 12), ("поподне", 15), ("увече", 18), ("ноћу", 20), ("поноћ", 0)],
    "ca": [("matí", 9), ("migdia", 12), ("tarda", 15), ("vespre", 18), ("nit", 20), ("mitjanit", 0)],
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
    "pl": {
        "jeden": 1, "jedna": 1, "jedno": 1, "dwa": 2, "dwie": 2, "trzy": 3, "cztery": 4,
        "pięć": 5, "sześć": 6, "siedem": 7, "osiem": 8, "dziewięć": 9, "dziesięć": 10,
    },
    "uk": {
        "один": 1, "одна": 1, "два": 2, "дві": 2, "три": 3, "чотири": 4,
        "п'ять": 5, "шість": 6, "сім": 7, "вісім": 8, "дев'ять": 9, "десять": 10,
    },
    "cs": {
        "jeden": 1, "jedna": 1, "dva": 2, "dvě": 2, "tři": 3, "čtyři": 4,
        "pět": 5, "šest": 6, "sedm": 7, "osm": 8, "devět": 9, "deset": 10,
    },
    "sk": {
        "jeden": 1, "jedna": 1, "dva": 2, "dve": 2, "tri": 3, "štyri": 4,
        "päť": 5, "šesť": 6, "sedem": 7, "osem": 8, "deväť": 9, "desať": 10,
    },
    "fi": {
        "yksi": 1, "kaksi": 2, "kolme": 3, "neljä": 4, "viisi": 5,
        "kuusi": 6, "seitsemän": 7, "kahdeksan": 8, "yhdeksän": 9, "kymmenen": 10,
    },
    "ro": {
        "unu": 1, "una": 1, "doi": 2, "două": 2, "trei": 3, "patru": 4, "cinci": 5,
        "șase": 6, "șapte": 7, "opt": 8, "nouă": 9, "zece": 10,
    },
    "el": {
        "ένα": 1, "μία": 1, "δύο": 2, "τρία": 3, "τέσσερα": 4, "πέντε": 5,
        "έξι": 6, "επτά": 7, "οκτώ": 8, "εννέα": 9, "δέκα": 10,
    },
    "he": {
        "אחד": 1, "אחת": 1, "שניים": 2, "שתיים": 2, "שלושה": 3, "שלוש": 3,
        "ארבעה": 4, "חמישה": 5, "שישה": 6, "שבעה": 7, "שמונה": 8, "תשעה": 9, "עשרה": 10,
    },
    "ar": {
        "واحد": 1, "واحدة": 1, "اثنان": 2, "ثلاثة": 3, "أربعة": 4, "خمسة": 5,
        "ستة": 6, "سبعة": 7, "ثمانية": 8, "تسعة": 9, "عشرة": 10,
    },
    "fa": {
        "یک": 1, "دو": 2, "سه": 3, "چهار": 4, "پنج": 5,
        "شش": 6, "هفت": 7, "هشت": 8, "نه": 9, "ده": 10,
    },
    "ur": {
        "ایک": 1, "دو": 2, "تین": 3, "چار": 4, "پانچ": 5,
        "چھ": 6, "سات": 7, "آٹھ": 8, "نو": 9, "دس": 10,
    },
    "hi": {
        "एक": 1, "दो": 2, "तीन": 3, "चार": 4, "पांच": 5, "पाँच": 5,
        "छह": 6, "सात": 7, "आठ": 8, "नौ": 9, "दस": 10,
    },
    "bn": {
        "এক": 1, "দুই": 2, "তিন": 3, "চার": 4, "পাঁচ": 5,
        "ছয়": 6, "সাত": 7, "আট": 8, "নয়": 9, "দশ": 10,
    },
    "id": {
        "satu": 1, "dua": 2, "tiga": 3, "empat": 4, "lima": 5,
        "enam": 6, "tujuh": 7, "delapan": 8, "sembilan": 9, "sepuluh": 10,
    },
    "ms": {
        "satu": 1, "dua": 2, "tiga": 3, "empat": 4, "lima": 5,
        "enam": 6, "tujuh": 7, "lapan": 8, "sembilan": 9, "sepuluh": 10,
    },
    "vi": {
        "một": 1, "hai": 2, "ba": 3, "bốn": 4, "năm": 5,
        "sáu": 6, "bảy": 7, "tám": 8, "chín": 9, "mười": 10,
    },
    "th": {
        "หนึ่ง": 1, "สอง": 2, "สาม": 3, "สี่": 4, "ห้า": 5,
        "หก": 6, "เจ็ด": 7, "แปด": 8, "เก้า": 9, "สิบ": 10,
    },
    "zh": {
        "一": 1, "两": 2, "二": 2, "三": 3, "四": 4, "五": 5,
        "六": 6, "七": 7, "八": 8, "九": 9, "十": 10, "十二": 12,
    },
    "ja": {
        "一": 1, "二": 2, "三": 3, "四": 4, "五": 5,
        "六": 6, "七": 7, "八": 8, "九": 9, "十": 10,
    },
    "ko": {
        "한": 1, "하나": 1, "일": 1, "두": 2, "둘": 2, "이": 2, "세": 3, "셋": 3, "삼": 3,
        "네": 4, "넷": 4, "사": 4, "다섯": 5, "오": 5, "여섯": 6, "일곱": 7, "여덟": 8, "아홉": 9, "열": 10,
    },
    "az": {
        "bir": 1, "iki": 2, "üç": 3, "dörd": 4, "beş": 5,
        "altı": 6, "yeddi": 7, "səkkiz": 8, "doqquz": 9, "on": 10,
    },
    "uz": {
        "bir": 1, "ikki": 2, "uch": 3, "to'rt": 4, "besh": 5,
        "olti": 6, "yetti": 7, "sakkiz": 8, "to'qqiz": 9, "o'n": 10,
    },
    "kk": {
        "бір": 1, "екі": 2, "үш": 3, "төрт": 4, "бес": 5,
        "алты": 6, "жеті": 7, "сегіз": 8, "тоғыз": 9, "он": 10,
    },
    "hr": {
        "jedan": 1, "dva": 2, "tri": 3, "četiri": 4, "pet": 5,
        "šest": 6, "sedam": 7, "osam": 8, "devet": 9, "deset": 10,
    },
    "sr": {
        "један": 1, "два": 2, "три": 3, "четири": 4, "пет": 5,
        "шест": 6, "седам": 7, "осам": 8, "девет": 9, "десет": 10,
    },
}

# Extra shorthand units often used in real-world data (3d ago, 5 dk önce, etc.)
EXTRA_UNITS: dict[str, list[tuple[str, int]]] = {
    "en": [("y", 0), ("yr", 0), ("yrs", 0), ("mo", 1), ("mos", 1), ("mon", 1), ("mons", 1), ("w", 2), ("wk", 2), ("wks", 2), ("d", 3), ("h", 4), ("hr", 4), ("hrs", 4), ("m", 5), ("min", 5), ("mins", 5), ("s", 6), ("sec", 6), ("secs", 6)],
    "tr": [("yıl", 0), ("sene", 0), ("ay", 1), ("hafta", 2), ("hf", 2), ("gün", 3), ("saat", 4), ("sa", 4), ("dakika", 5), ("dk", 5), ("saniye", 6), ("sn", 6)],
}

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
    for tok, unit_id in EXTRA_UNITS.get(code, []):
        key = (norm(tok), unit_id)
        if key not in seen_units:
            seen_units.add(key)
            units.append([key[0], unit_id])

    ago = dedup_list(info.get("ago", []))
    in_words = dedup_list(info.get("in", []))
    skip = dedup_list(list(info.get("skip", [])) + list(info.get("pertain", [])))

    # Relative exact phrases: preserve multi-sense entries (e.g. Hindi कल = yesterday AND tomorrow)
    relative_exact: list[list[Any]] = []
    seen_rel_exact: set[tuple[str, int, int, int]] = set()
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
            key = (phrase, sign, amount, unit_id)
            if key not in seen_rel_exact:
                seen_rel_exact.add(key)
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
            if any(ch in prefix + suffix for ch in "[](){}|?*^$\\"):
                continue
            key = (prefix, suffix, sign, unit_id)
            if key not in seen_affix and (prefix or suffix):
                seen_affix.add(key)
                relative_affix.append([prefix, suffix, sign, unit_id])

    # Word numbers
    word_numbers: dict[str, int] = dict(EXTRA_WORD_NUMBERS.get(code, {}))
    for simp_dict in info.get("simplifications", []):
        for k, v in simp_dict.items():
            v_str = str(v).strip()
            if v_str.isdigit():
                k_clean = norm(str(k))
                if k_clean and not any(ch in k_clean for ch in "[](){}|?*^$\\+"):
                    word_numbers.setdefault(k_clean, int(v_str))

    # Time-of-day phrases
    time_of_day: dict[str, int] = {}
    for key_name, hour in TOD_KEYS:
        for phrase in dedup_list(info.get(key_name, [])):
            time_of_day.setdefault(phrase, hour)
    for phrase, hour in EXTRA_TOD.get(code, []):
        time_of_day.setdefault(norm(phrase), hour)

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
        "time_of_day": sorted([[k, v] for k, v in time_of_day.items()]),
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
    total_tod = sum(len(l["time_of_day"]) for l in locales)
    print(
        f"Wrote {len(locales)} locales to {out_path} ({size_kb:.1f} KB), "
        f"relative_exact={total_exact}, relative_affix={total_affixes}, time_of_day={total_tod}"
    )


if __name__ == "__main__":
    main()
