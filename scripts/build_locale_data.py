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
import unicodedata
from pathlib import Path
from typing import Any

from dateparser.data import date_translation_data

from locale_enrichment_205 import (
    EXTRA_EXTENDED_NUMBERS,
    EXTRA_ORDINALS,
    EXTRA_REL_MARKERS_205,
    EXTRA_TOD_205,
    EXTRA_UNITS_205,
    EXTRA_WORD_NUMBERS_205,
    EXTRA_ZERO_205,
    MIRROR_LOCALES,
    TL_EXTRA,
)

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
    "sl": [("zjutraj", 9), ("poldne", 12), ("popoldne", 15), ("zvečer", 18), ("ponoči", 20), ("polnoč", 0)],
    "lt": [("rytas", 9), ("ryte", 9), ("vidurdienis", 12), ("popietė", 15), ("vakaras", 18), ("naktis", 20), ("vidurnaktis", 0)],
    "lv": [("rīts", 9), ("no rīta", 9), ("pusdiena", 12), ("pēcpusdiena", 15), ("vakars", 18), ("nakts", 20), ("pusnakts", 0)],
    "et": [("hommik", 9), ("hommikul", 9), ("lõuna", 12), ("pärastlõuna", 15), ("õhtu", 18), ("öö", 20), ("kesköö", 0)],
    "ka": [("დილა", 9), ("შუადღე", 12), ("საღამო", 18), ("ღამე", 20), ("შუაღამე", 0)],
    "hy": [("առավոտ", 9), ("կեսօր", 12), ("երեկո", 18), ("գիշեր", 20), ("կեսգիշեր", 0)],
    "sq": [("mëngjes", 9), ("mesditë", 12), ("pasdite", 15), ("mbrëmje", 18), ("natë", 20), ("mesnatë", 0)],
    "mk": [("утро", 9), ("наутро", 9), ("пладне", 12), ("попладне", 15), ("вечер", 18), ("ноќ", 20), ("полноќ", 0)],
    "bs": [("ujutro", 9), ("podne", 12), ("poslijepodne", 15), ("navečer", 18), ("noću", 20), ("ponoć", 0)],
    "be": [("раніцай", 9), ("поўдзень", 12), ("удзень", 15), ("увечары", 18), ("ноччу", 20), ("поўнач", 0)],
    "is": [("morgunn", 9), ("hádegi", 12), ("síðdegi", 15), ("kvöld", 18), ("nótt", 20), ("miðnætti", 0)],
    "ga": [("maidin", 9), ("meán lae", 12), ("tráthnóna", 15), ("tráthnóna déanach", 18), ("oíche", 20), ("meán oíche", 0)],
    "cy": [("bore", 9), ("canol dydd", 12), ("prynhawn", 15), ("gyda'r nos", 18), ("nos", 20), ("hanner nos", 0)],
    "eu": [("goiza", 9), ("eguerdia", 12), ("arratsaldea", 15), ("iluntzea", 18), ("gaua", 20), ("gauerdia", 0)],
    "gl": [("mañá", 9), ("mediodía", 12), ("tarde", 15), ("noite", 20), ("medianoite", 0)],
    "af": [("oggend", 9), ("middag", 12), ("namiddag", 15), ("aand", 18), ("nag", 20), ("middernag", 0)],
    "sw": [("asubuhi", 9), ("adhuhuri", 12), ("alasiri", 15), ("jioni", 18), ("usiku", 20), ("usiku wa manane", 0)],
    "ta": [("காலை", 9), ("மதியம்", 12), ("மாலை", 18), ("இரவு", 20), ("நள்ளிரவு", 0)],
    "te": [("ఉదయం", 9), ("మధ్యాహ్నం", 12), ("సాయంత్రం", 18), ("రాత్రి", 20), ("అర్ధరాత్రి", 0)],
    "ml": [("രാവിലെ", 9), ("ഉച്ചയ്ക്ക്", 12), ("വൈകുന്നേരം", 18), ("രാത്രി", 20), ("അർദ്ധരാത്രി", 0)],
    "kn": [("ಬೆಳಿಗ್ಗೆ", 9), ("ಮಧ್ಯಾಹ್ನ", 12), ("ಸಂಜೆ", 18), ("ರಾತ್ರಿ", 20), ("ಮಧ್ಯರಾತ್ರಿ", 0)],
    "mr": [("सकाळ", 9), ("दुपार", 12), ("संध्याकाळ", 18), ("रात्र", 20), ("मध्यरात्र", 0)],
    "gu": [("સવાર", 9), ("બપોર", 12), ("સાંજ", 18), ("રાત", 20), ("મધરાત", 0)],
    "pa": [("ਸਵੇਰ", 9), ("ਦੁਪਹਿਰ", 12), ("ਸ਼ਾਮ", 18), ("ਰਾਤ", 20), ("ਅੱਧੀ ਰਾਤ", 0)],
    "ne": [("बिहान", 9), ("दिउँसो", 12), ("साँझ", 18), ("राति", 20), ("मध्यरात", 0)],
    "fil": [("umaga", 9), ("tanghali", 12), ("hapon", 15), ("gabi", 20), ("hatinggabi", 0)],
    "nn": [("morgon", 9), ("middag", 12), ("ettermiddag", 15), ("kveld", 18), ("natt", 20), ("midnatt", 0)],
    "my": [("နံနက်", 9), ("မွန်းတည့်", 12), ("ညနေ", 18), ("ည", 20), ("သန်းခေါင်", 0)],
    "km": [("ព្រឹក", 9), ("ថ្ងៃត្រង់", 12), ("ល្ងាច", 18), ("យប់", 20), ("អធ្រាត្រ", 0)],
    "si": [("උදේ", 9), ("දවල්", 12), ("හවස", 18), ("රෑ", 20), ("මධ්‍යම රාත්‍රිය", 0)],
    "mn": [("өглөө", 9), ("үд", 12), ("орой", 18), ("шөнө", 20), ("шөнө дунд", 0)],
    "ky": [("эртең менен", 9), ("түш", 12), ("кеч", 18), ("түн", 20), ("түн ортосу", 0)],
    "am": [("ጠዋት", 9), ("ቀን", 12), ("ማታ", 18), ("ሌሊት", 20), ("እኩለ ሌሊት", 0)],
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
    "sl": {"ena": 1, "dva": 2, "tri": 3, "štiri": 4, "pet": 5, "šest": 6, "sedem": 7, "osem": 8, "devet": 9, "deset": 10},
    "lt": {"vienas": 1, "du": 2, "trys": 3, "keturi": 4, "penki": 5, "šeši": 6, "septyni": 7, "aštuoni": 8, "devyni": 9, "dešimt": 10},
    "lv": {"viens": 1, "divi": 2, "trīs": 3, "četri": 4, "pieci": 5, "seši": 6, "septiņi": 7, "astoņi": 8, "deviņi": 9, "desmit": 10},
    "et": {"üks": 1, "kaks": 2, "kolm": 3, "neli": 4, "viis": 5, "kuus": 6, "seitse": 7, "kaheksa": 8, "üheksa": 9, "kümme": 10},
    "hy": {"մեկ": 1, "երկու": 2, "երեք": 3, "չորս": 4, "հինգ": 5, "վեց": 6, "յոթ": 7, "ութ": 8, "ինը": 9, "տասը": 10},
    "sq": {"një": 1, "dy": 2, "tre": 3, "katër": 4, "pesë": 5, "gjashtë": 6, "shtatë": 7, "tetë": 8, "nëntë": 9, "dhjetë": 10},
    "mk": {"еден": 1, "два": 2, "три": 3, "четири": 4, "пет": 5, "шест": 6, "седум": 7, "осум": 8, "девет": 9, "десет": 10},
    "bs": {"jedan": 1, "dva": 2, "tri": 3, "četiri": 4, "pet": 5, "šest": 6, "sedam": 7, "osam": 8, "devet": 9, "deset": 10},
    "be": {"адзін": 1, "два": 2, "тры": 3, "чатыры": 4, "пяць": 5, "шэсць": 6, "сем": 7, "восем": 8, "дзевяць": 9, "дзесяць": 10},
    "is": {"einn": 1, "tveir": 2, "þrír": 3, "fjórir": 4, "fimm": 5, "sex": 6, "sjö": 7, "átta": 8, "níu": 9, "tíu": 10},
    "ga": {"aon": 1, "dó": 2, "trí": 3, "ceathair": 4, "cúig": 5, "sé": 6, "seacht": 7, "ocht": 8, "naoi": 9, "deich": 10},
    "cy": {"un": 1, "dau": 2, "tri": 3, "pedwar": 4, "pump": 5, "chwech": 6, "saith": 7, "wyth": 8, "naw": 9, "deg": 10},
    "eu": {"bat": 1, "bi": 2, "hiru": 3, "lau": 4, "bost": 5, "sei": 6, "zazpi": 7, "zortzi": 8, "bederatzi": 9, "hamar": 10},
    "gl": {"un": 1, "dous": 2, "tres": 3, "catro": 4, "cinco": 5, "seis": 6, "sete": 7, "oito": 8, "nove": 9, "dez": 10},
    "af": {"een": 1, "twee": 2, "drie": 3, "vier": 4, "vyf": 5, "ses": 6, "sewe": 7, "agt": 8, "nege": 9, "tien": 10},
    "sw": {"moja": 1, "mbili": 2, "tatu": 3, "nne": 4, "tano": 5, "sita": 6, "saba": 7, "nane": 8, "tisa": 9, "kumi": 10},
    "ta": {"ஒன்று": 1, "இரண்டு": 2, "மூன்று": 3, "நான்கு": 4, "ஐந்து": 5, "ஆறு": 6, "ஏழு": 7, "எட்டு": 8, "ஒன்பது": 9, "பத்து": 10},
    "te": {"ఒకటి": 1, "రెండు": 2, "మూడు": 3, "నాలుగు": 4, "ఐదు": 5, "ఆరు": 6, "ఏడు": 7, "ఎనిమిది": 8, "తొమ్మిది": 9, "పది": 10},
    "ml": {"ഒന്ന്": 1, "രണ്ട്": 2, "മൂന്ന്": 3, "നാല്": 4, "അഞ്ച്": 5, "ആറ്": 6, "ഏഴ്": 7, "എട്ട്": 8, "ഒമ്പത്": 9, "പത്ത്": 10},
    "kn": {"ಒಂದು": 1, "ಎರಡು": 2, "ಮೂರು": 3, "ನಾಲ್ಕು": 4, "ಐದು": 5, "ಆರು": 6, "ಏಳು": 7, "ಎಂಟು": 8, "ಒಂಬತ್ತು": 9, "ಹತ್ತು": 10},
    "mr": {"एक": 1, "दोन": 2, "तीन": 3, "चार": 4, "पाच": 5, "सहा": 6, "सात": 7, "आठ": 8, "नऊ": 9, "दहा": 10},
    "gu": {"એક": 1, "બે": 2, "ત્રણ": 3, "ચાર": 4, "પાંચ": 5, "છ": 6, "સાત": 7, "આઠ": 8, "નવ": 9, "દસ": 10},
    "pa": {"ਇੱਕ": 1, "ਦੋ": 2, "ਤਿੰਨ": 3, "ਚਾਰ": 4, "ਪੰਜ": 5, "ਛੇ": 6, "ਸੱਤ": 7, "ਅੱਠ": 8, "ਨੌਂ": 9, "ਦਸ": 10},
    "ne": {"एक": 1, "दुई": 2, "तीन": 3, "चार": 4, "पाँच": 5, "छ": 6, "सात": 7, "आठ": 8, "नौ": 9, "दस": 10},
    "fil": {"isa": 1, "dalawa": 2, "tatlo": 3, "apat": 4, "lima": 5, "anim": 6, "pito": 7, "walo": 8, "siyam": 9, "sampu": 10},
    "nn": {"ein": 1, "to": 2, "tre": 3, "fire": 4, "fem": 5, "seks": 6, "sju": 7, "åtte": 8, "ni": 9, "ti": 10},
    "my": {"တစ်": 1, "နှစ်": 2, "သုံး": 3, "လေး": 4, "ငါး": 5, "ခြောက်": 6, "ခုနစ်": 7, "ရှစ်": 8, "ကိုး": 9, "တစ်ဆယ်": 10},
    "km": {"មួយ": 1, "ពីរ": 2, "បី": 3, "បួន": 4, "ប្រាំ": 5, "ប្រាំមួយ": 6, "ប្រាំពីរ": 7, "ប្រាំបី": 8, "ប្រាំបួន": 9, "ដប់": 10},
    "si": {"එක": 1, "දෙක": 2, "තුන": 3, "හතර": 4, "පහ": 5, "හය": 6, "හත": 7, "අට": 8, "නවය": 9, "දහය": 10},
    "mn": {"нэг": 1, "хоёр": 2, "гурав": 3, "дөрөв": 4, "тав": 5, "зургаа": 6, "долоо": 7, "найм": 8, "ес": 9, "арав": 10},
    "ky": {"бир": 1, "эки": 2, "үч": 3, "төрт": 4, "беш": 5, "алты": 6, "жети": 7, "сегиз": 8, "тогуз": 9, "он": 10},
    "am": {"አንድ": 1, "ሁለት": 2, "ሦስት": 3, "አራት": 4, "አምስት": 5, "ስድስት": 6, "ሰባት": 7, "ስምንት": 8, "ዘጠኝ": 9, "አስር": 10},
}

# Exact +/-2 day and colloquial relative idioms omitted by CLDR
EXTRA_RELATIVE_EXACT: dict[str, list[tuple[str, int, int, int]]] = {
    "tr": [("evvelsi gün", -1, 2, 3), ("dünden önceki gün", -1, 2, 3), ("öbür gün", 1, 2, 3), ("öbürsü gün", 1, 2, 3)],
    "es": [("anteayer", -1, 2, 3), ("pasado mañana", 1, 2, 3)],
    "pt": [("anteontem", -1, 2, 3), ("depois de amanhã", 1, 2, 3)],
    "it": [("l'altro ieri", -1, 2, 3), ("dopodomani", 1, 2, 3)],
    "pl": [("przedwczoraj", -1, 2, 3), ("pojutrze", 1, 2, 3)],
    "ko": [("그제", -1, 2, 3), ("그저께", -1, 2, 3), ("모레", 1, 2, 3)],
    "ar": [("أول أمس", -1, 2, 3), ("أول امس", -1, 2, 3), ("بعد غد", 1, 2, 3)],
    "fa": [("پریروز", -1, 2, 3), ("پس‌فردا", 1, 2, 3), ("پس فردا", 1, 2, 3)],
    "he": [("שלשום", -1, 2, 3), ("מחרתיים", 1, 2, 3)],
    "el": [("προχθές", -1, 2, 3), ("μεθαύριο", 1, 2, 3)],
    "ro": [("alaltăieri", -1, 2, 3), ("poimâine", 1, 2, 3)],
    "hu": [("tegnapelőtt", -1, 2, 3), ("holnapután", 1, 2, 3)],
    "sv": [("i förrgår", -1, 2, 3), ("förrgår", -1, 2, 3), ("i övermorgon", 1, 2, 3), ("övermorgon", 1, 2, 3)],
    "fi": [("toissapäivänä", -1, 2, 3), ("ylihuomenna", 1, 2, 3)],
    "da": [("i forgårs", -1, 2, 3), ("i overmorgen", 1, 2, 3)],
    "nb": [("i forgårs", -1, 2, 3), ("i overmorgen", 1, 2, 3)],
    "id": [("kemarin lusa", -1, 2, 3), ("lusa", 1, 2, 3)],
    "vi": [("hôm kia", -1, 2, 3), ("ngày kia", 1, 2, 3)],
    "th": [("เมื่อวานซืน", -1, 2, 3), ("มะรืน", 1, 2, 3)],
    "az": [("srağagün", -1, 2, 3), ("birigün", 1, 2, 3)],
    "uk": [("позавчора", -1, 2, 3), ("післязавтра", 1, 2, 3)],
    "bg": [("завчера", -1, 2, 3), ("вдругиден", 1, 2, 3)],
    "hr": [("prekjučer", -1, 2, 3), ("prekosutra", 1, 2, 3)],
    "sr": [("прекјуче", -1, 2, 3), ("прекосутра", 1, 2, 3)],
    "sk": [("predvčerom", -1, 2, 3), ("pozajtra", 1, 2, 3)],
    "cs": [("předevčírem", -1, 2, 3), ("pozítří", 1, 2, 3)],
}

EXTRA_AGO: dict[str, list[str]] = {
    "tr": ["evvel"],
    "az": ["əvvəl"],
    "ko": ["전"],
}

EXTRA_IN_WORDS: dict[str, list[str]] = {
    "ar": ["بعد"],
    "fa": ["دیگر", "بعد"],
    "pt": ["daqui a"],
    "ro": ["peste"],
    "es": ["dentro de"],
    "fr": ["dans"],
    "it": ["tra", "fra"],
    "nl": ["over"],
    "pl": ["za"],
    "sv": ["om"],
    "el": ["σε"],
    "he": ["בעוד"],
    "fi": ["kuluttua"],
    "ko": ["후", "뒤"],
    "gsw": ["innert"],
    "lb": ["bannent"],
    "zu": ["ngemva kwa"],
}

ENGLISH_STOPWORDS = {
    "in", "at", "on", "to", "a", "an", "is", "be", "or", "so", "do", "no", "as", "by",
    "my", "we", "us", "he", "me", "it", "of", "go", "up", "if", "let", "set", "get",
    "can", "man", "see", "her", "his", "she", "did", "say", "was", "are", "not", "but",
    "for", "all", "new", "old", "one", "two", "may", "year", "month", "week", "day",
    "hour", "minute", "second",
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
    for tok, unit_id in EXTRA_UNITS.get(code, []) + EXTRA_UNITS_205.get(code, []):
        key = (norm(tok), unit_id)
        if key not in seen_units:
            seen_units.add(key)
            units.append([key[0], unit_id])

    am = am or dedup_list(TL_EXTRA["am"]) if code == "tl" else am
    pm = pm or dedup_list(TL_EXTRA["pm"]) if code == "tl" else pm

    ago = dedup_list(list(info.get("ago", [])) + EXTRA_AGO.get(code, []))
    in_words = dedup_list(list(info.get("in", [])) + EXTRA_IN_WORDS.get(code, []))
    skip = dedup_list(list(info.get("skip", [])) + list(info.get("pertain", [])))

    # Relative exact phrases: preserve multi-sense entries (e.g. Hindi कल = yesterday AND tomorrow)
    relative_exact: list[list[Any]] = []
    seen_rel_exact: set[tuple[str, int, int, int]] = set()
    for phrase, sign, amount, unit_id in EXTRA_RELATIVE_EXACT.get(code, []):
        p_norm = norm(phrase)
        key = (p_norm, sign, amount, unit_id)
        if p_norm and key not in seen_rel_exact:
            seen_rel_exact.add(key)
            relative_exact.append([p_norm, sign, amount, unit_id])
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

    # Word numbers (complete 0..10 + teens/tens/hundreds + oblique forms)
    word_numbers: dict[str, int] = {norm(k): v for k, v in EXTRA_WORD_NUMBERS.get(code, {}).items()}
    word_numbers.update({norm(k): v for k, v in EXTRA_WORD_NUMBERS_205.get(code, {}).items()})
    word_numbers.update({norm(k): v for k, v in EXTRA_EXTENDED_NUMBERS.get(code, {}).items()})
    if 0 not in word_numbers.values():
        zero_w = norm(EXTRA_ZERO_205.get(code, "0"))
        if zero_w and zero_w not in word_numbers:
            word_numbers[zero_w] = 0
        elif "0" not in word_numbers:
            word_numbers["0"] = 0
    for simp_dict in info.get("simplifications", []):
        for k, v in simp_dict.items():
            v_str = str(v).strip()
            if v_str.isdigit():
                k_clean = norm(str(k))
                if k_clean and not any(ch in k_clean for ch in "[](){}|?*^$\\+"):
                    word_numbers.setdefault(k_clean, int(v_str))

    ordinals: dict[str, int] = {norm(k): v for k, v in EXTRA_ORDINALS.get(code, {}).items()}

    # Time-of-day phrases
    time_of_day: dict[str, int] = {}
    for key_name, hour in TOD_KEYS:
        for phrase in dedup_list(info.get(key_name, [])):
            time_of_day.setdefault(phrase, hour)
    for phrase, hour in EXTRA_TOD.get(code, []) + EXTRA_TOD_205.get(code, []):
        p_norm = norm(phrase)
        p_ascii = "".join(
            ch for ch in unicodedata.normalize("NFKD", p_norm) if not unicodedata.combining(ch)
        )
        if code != "en" and ((len(p_ascii) <= 2 and p_ascii.isascii()) or p_norm in ENGLISH_STOPWORDS):
            continue
        time_of_day.setdefault(p_norm, hour)

    if code == "tl" and not relative_exact:
        for phrase, sign, amount, unit_id in TL_EXTRA["relative_exact"]:
            relative_exact.append([norm(phrase), sign, amount, unit_id])

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
        "ordinals": sorted([[k, v] for k, v in ordinals.items()]),
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

    locales = [extract_locale(code, importlib.import_module(f"dateparser.data.date_translation_data.{code}").info) for code in ordered_codes]
    by_code = {loc["code"]: loc for loc in locales}

    # Apply script/sibling mirrors (tl <- fil, az-Latn <- az, etc.) for empty or partial NLP fields
    for target_code, donor_code in MIRROR_LOCALES.items():
        tgt = by_code.get(target_code)
        src = by_code.get(donor_code)
        if not tgt or not src:
            continue
        for fld in ("am", "pm", "relative_exact", "relative_affix", "ago", "in_words", "time_of_day"):
            if not tgt[fld] and src[fld]:
                tgt[fld] = [list(item) if isinstance(item, list) else item for item in src[fld]]
        for fld in ("word_numbers", "ordinals"):
            existing = {k: v for k, v in tgt[fld]}
            for k, v in src[fld]:
                existing.setdefault(k, v)
            tgt[fld] = sorted([[k, v] for k, v in existing.items()])

    # Synthesize structured relative_affix patterns for any locale lacking past or future affixes
    for loc in locales:
        code = loc["code"]
        has_past = any(a[2] == -1 for a in loc["relative_affix"])
        has_fut = any(a[2] == 1 for a in loc["relative_affix"])
        if (not has_past or not has_fut) and code in EXTRA_REL_MARKERS_205:
            past_markers, fut_markers = EXTRA_REL_MARKERS_205[code]
            units_by_id: dict[int, list[str]] = {}
            for u_tok, u_id in loc["units"]:
                if u_id <= 6:
                    units_by_id.setdefault(u_id, []).append(u_tok)

            seen_aff = {tuple(a) for a in loc["relative_affix"]}
            gen_affixes: list[list[Any]] = list(loc["relative_affix"])
            sign_lists = []
            if not has_past:
                sign_lists.append((-1, past_markers))
            if not has_fut:
                sign_lists.append((1, fut_markers))
            for u_id in range(7):
                u_toks = units_by_id.get(u_id, [])
                native_toks = [u for u in u_toks if u not in ENGLISH_STOPWORDS]
                chosen_toks = native_toks or u_toks
                for u_tok in chosen_toks[:4]:
                    for sign, marker_list in sign_lists:
                        for placement, marker in marker_list:
                            if placement == "prefix":
                                pfx = marker.lstrip()
                                sfx = f" {u_tok}"
                            else:
                                pfx = ""
                                sfx = f" {u_tok}{marker}" if marker.startswith(" ") else f" {u_tok} {marker.strip()}"
                            key = (pfx, sfx, sign, u_id)
                            if key not in seen_aff:
                                seen_aff.add(key)
                                gen_affixes.append([pfx, sfx, sign, u_id])
            loc["relative_affix"] = gen_affixes

        # Populate ago / in_words for all 205 locales (skipping English stopwords & 1-char ASCII)
        def _keep_marker(m: str) -> bool:
            m = norm(m).strip(".,;:-/")
            if not m:
                return False
            if code != "en" and ((len(m) == 1 and m.isascii()) or m in ENGLISH_STOPWORDS):
                return False
            return True

        ago_set = list(loc["ago"])
        in_set = list(loc["in_words"])
        if code in EXTRA_REL_MARKERS_205:
            past_markers, fut_markers = EXTRA_REL_MARKERS_205[code]
            for _, m in past_markers:
                mc = norm(m)
                if mc and mc not in ago_set and _keep_marker(mc):
                    ago_set.append(mc)
            for _, m in fut_markers:
                mc = norm(m)
                if mc and mc not in in_set and _keep_marker(mc):
                    in_set.append(mc)

        unit_toks_sorted = sorted({u_tok for u_tok, _ in loc["units"]}, key=len, reverse=True)
        for pfx, sfx, sign, _ in loc["relative_affix"]:
            rem_pfx = pfx.strip()
            rem_sfx = sfx.strip()
            for u_tok in unit_toks_sorted:
                if rem_sfx == u_tok:
                    rem_sfx = ""
                elif rem_sfx.startswith(u_tok + " "):
                    rem_sfx = rem_sfx[len(u_tok) + 1 :].strip()
                elif rem_sfx.endswith(" " + u_tok):
                    rem_sfx = rem_sfx[: -len(u_tok) - 1].strip()
                elif not u_tok.isascii() and rem_sfx.startswith(u_tok):
                    rem_sfx = rem_sfx[len(u_tok) :].strip()
            for cand in (rem_pfx, rem_sfx):
                cand_c = norm(cand).strip(".,;:-/")
                if _keep_marker(cand_c):
                    target = ago_set if sign < 0 else in_set
                    if cand_c not in target:
                        target.append(cand_c)

        loc["ago"] = ago_set
        loc["in_words"] = in_set

    # Verify strict 205/205 completeness across every single NLP field
    required_fields = (
        "months", "weekdays", "am", "pm", "units",
        "ago", "in_words", "relative_exact", "relative_affix",
        "word_numbers", "time_of_day",
    )
    incomplete = [
        (loc["code"], [f for f in required_fields if not loc.get(f)])
        for loc in locales
        if any(not loc.get(f) for f in required_fields)
    ]
    if incomplete:
        raise RuntimeError(f"Incomplete locales remain ({len(incomplete)}): {incomplete[:15]}")

    # Build-time numeral integrity validation across all 205 locales
    num_errors: list[str] = []
    for loc in locales:
        code = loc["code"]
        seen_keys: set[str] = set()
        vals: set[int] = set()
        for w, n in loc["word_numbers"]:
            if w in seen_keys:
                num_errors.append(f"[{code}] duplicate word number key {w!r}")
            seen_keys.add(w)
            vals.add(n)
        missing = [d for d in range(0, 11) if d not in vals]
        if missing:
            num_errors.append(f"[{code}] missing numerals {missing}")
    if num_errors:
        raise RuntimeError(f"Numeral validation failed ({len(num_errors)}): {num_errors[:15]}")

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
