"""Differential tests vs upstream dateparser, round-trip checks, and adversarial fuzzing."""
from __future__ import annotations

import calendar
from datetime import datetime, timedelta, timezone
import random
import pytest

import dateoxide
from dateoxide.search import search_dates

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


def _random_naive_dt(rng: random.Random, min_year: int = 1970, max_year: int = 2040) -> datetime:
    year = rng.randint(min_year, max_year)
    month = rng.randint(1, 12)
    day = rng.randint(1, calendar.monthrange(year, month)[1])
    hour = rng.randint(0, 23)
    minute = rng.randint(0, 59)
    second = rng.randint(0, 59)
    return datetime(year, month, day, hour, minute, second)


def test_differential_iso_and_numeric_vs_dateparser():
    dateparser = pytest.importorskip("dateparser")
    rng = random.Random(20261126)

    for i in range(120):
        dt = _random_naive_dt(rng)
        mode = i % 6
        if mode == 0:
            s = dt.strftime("%Y-%m-%d")
            cfg = {"RELATIVE_BASE": FIXED_BASE}
            expected_kind = "naive"
        elif mode == 1:
            s = dt.strftime("%Y-%m-%d %H:%M:%S")
            cfg = {"RELATIVE_BASE": FIXED_BASE}
            expected_kind = "naive"
        elif mode == 2:
            s = dt.strftime("%Y-%m-%dT%H:%M:%SZ")
            cfg = {"RELATIVE_BASE": FIXED_BASE}
            expected_kind = "aware_utc"
        elif mode == 3:
            s = dt.strftime("%d/%m/%Y")
            cfg = {"RELATIVE_BASE": FIXED_BASE, "DATE_ORDER": "DMY"}
            expected_kind = "naive"
        elif mode == 4:
            s = dt.strftime("%m/%d/%Y")
            cfg = {"RELATIVE_BASE": FIXED_BASE, "DATE_ORDER": "MDY"}
            expected_kind = "naive"
        else:
            s = dt.strftime("%Y.%m.%d")
            cfg = {"RELATIVE_BASE": FIXED_BASE, "DATE_ORDER": "YMD"}
            expected_kind = "naive"

        ox = dateoxide.parse(s, settings=cfg)
        dp = dateparser.parse(s, settings=cfg)
        if expected_kind == "aware_utc":
            assert ox is not None and dp is not None
            assert ox.astimezone(timezone.utc) == dp.astimezone(timezone.utc), f"{s!r}: {ox} != {dp}"
        else:
            assert ox == dp, f"{s!r} ({cfg}): ox={ox!r} != dp={dp!r}"


def test_differential_multilingual_absolute_and_relative_vs_dateparser():
    dateparser = pytest.importorskip("dateparser")
    rng = random.Random(20261127)

    months_by_lang: dict[str, list[str]] = {
        "en": ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"],
        "tr": ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"],
        "de": ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
        "fr": ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
        "es": ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"],
        "it": ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"],
        "pt": ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"],
        "ru": ["января", "февраля", "марта", "апреля", "мая", "июня", "июля", "августа", "сентября", "октября", "ноября", "декабря"],
        "pl": ["stycznia", "lutego", "marca", "kwietnia", "maja", "czerwca", "lipca", "sierpnia", "września", "października", "listopada", "grudnia"],
        "nl": ["januari", "februari", "maart", "april", "mei", "juni", "juli", "augustus", "september", "oktober", "november", "december"],
    }

    lang_keys = list(months_by_lang.keys())
    for i in range(80):
        lang = lang_keys[i % len(lang_keys)]
        m = rng.randint(1, 12)
        y = rng.randint(1990, 2035)
        d = rng.randint(1, calendar.monthrange(y, m)[1])
        m_name = months_by_lang[lang][m - 1]
        if lang in ("es", "pt"):
            s = f"{d} de {m_name} de {y}"
        elif lang == "de":
            s = f"{d}. {m_name} {y}"
        else:
            s = f"{d} {m_name} {y}"
        ox = dateoxide.parse(s, languages=[lang], settings={"RELATIVE_BASE": FIXED_BASE})
        dp = dateparser.parse(s, languages=[lang], settings={"RELATIVE_BASE": FIXED_BASE})
        assert ox == dp, f"[{lang}] {s!r}: ox={ox!r} != dp={dp!r}"

    rel_templates = [
        ("en", lambda n: f"{n} days ago"),
        ("en", lambda n: f"{n} hours ago"),
        ("en", lambda n: f"{n} minutes ago"),
        ("en", lambda n: f"{n} weeks ago"),
        ("en", lambda n: f"{n} months ago"),
        ("en", lambda n: f"{n} years ago"),
        ("en", lambda n: f"in {n} days"),
        ("en", lambda n: f"in {n} hours"),
        ("tr", lambda n: f"{n} gün önce"),
        ("tr", lambda n: f"{n} saat sonra"),
        ("de", lambda n: f"vor {n} Tagen"),
        ("fr", lambda n: f"il y a {n} jours"),
        ("es", lambda n: f"hace {n} días"),
    ]
    for i in range(65):
        lang, tmpl = rel_templates[i % len(rel_templates)]
        n = rng.randint(1, 30)
        s = tmpl(n)
        cfg = {"RELATIVE_BASE": FIXED_BASE}
        ox = dateoxide.parse(s, languages=[lang], settings=cfg)
        dp = dateparser.parse(s, languages=[lang], settings=cfg)
        assert ox == dp, f"[{lang}] {s!r}: ox={ox!r} != dp={dp!r}"


def test_differential_preferences_vs_dateparser():
    dateparser = pytest.importorskip("dateparser")
    rng = random.Random(998877)

    # Time-only with PREFER_DATES_FROM
    for _ in range(30):
        hh = rng.randint(0, 23)
        mm = rng.randint(0, 59)
        s = f"{hh:02d}:{mm:02d}"
        for pref in ("past", "future", "current_period"):
            cfg = {"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": pref}
            ox = dateoxide.parse(s, settings=cfg)
            dp = dateparser.parse(s, settings=cfg)
            assert ox == dp, f"{s!r} ({pref}): ox={ox!r} != dp={dp!r}"

    # Month-Day partial dates with PREFER_DATES_FROM
    months = [
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
    ]
    for _ in range(24):
        m_idx = rng.randint(1, 12)
        d = rng.randint(1, 28)
        s = f"{months[m_idx - 1]} {d}"
        for pref in ("past", "future", "current_period"):
            cfg = {"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": pref}
            ox = dateoxide.parse(s, settings=cfg)
            dp = dateparser.parse(s, settings=cfg)
            assert ox == dp, f"{s!r} ({pref}): ox={ox!r} != dp={dp!r}"

    # Month-Year partial dates with PREFER_DAY_OF_MONTH
    for _ in range(18):
        m_idx = rng.randint(1, 12)
        y = rng.randint(2000, 2032)
        s = f"{months[m_idx - 1]} {y}"
        for pref_dom in ("first", "last", "current"):
            cfg = {"RELATIVE_BASE": FIXED_BASE, "PREFER_DAY_OF_MONTH": pref_dom}
            ox = dateoxide.parse(s, settings=cfg)
            dp = dateparser.parse(s, settings=cfg)
            assert ox == dp, f"{s!r} ({pref_dom}): ox={ox!r} != dp={dp!r}"


def test_roundtrip_iso_rfc3339_and_iso_week_200_random_datetimes():
    rng = random.Random(13579)
    for _ in range(200):
        dt = _random_naive_dt(rng, 1900, 2100)
        assert dateoxide.parse(dt.strftime("%Y-%m-%d %H:%M:%S")) == dt
        assert dateoxide.parse(dt.strftime("%Y-%m-%dT%H:%M:%SZ")) == dt.replace(
            tzinfo=timezone.utc
        )
        iso_week_str = dt.strftime("%G-W%V-%u")
        parsed_week = dateoxide.parse(iso_week_str)
        assert parsed_week == dt.replace(hour=0, minute=0, second=0)


def test_fuzz_parse_2000_randomized_and_mutated_inputs():
    rng = random.Random(42)
    alphabet = (
        list("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")
        + list(" \t\n.,:/-+TZ[](){}@#!?$%*^&~`'\"\x00")
        + list("çğıöşüÇĞİÖŞÜäöüßéèàùñáíóú")
        + list("ابتثجحخدذرزسشصضطظعغفقكلمنهوي٠١٢٣٤٥٦٧٨٩")
        + list("абвгдеёжзийклмнопрстуфхцчшщъыьэюя")
        + list("年月日时分秒天周月년월일시분초")
    )
    seeds = [
        "2026-11-26T15:30:00Z",
        "12 December 2025",
        "3 gün önce",
        "2026-W48-4",
        "143025",
        "vor 2 Wochen",
        "15 Muharram 1447",
        "26/11/2026 14:30",
    ]

    for i in range(2000):
        kind = i % 4
        if kind == 0:
            length = rng.randint(0, 80)
            s = "".join(rng.choice(alphabet) for _ in range(length))
        elif kind == 1:
            base = list(rng.choice(seeds))
            for _ in range(rng.randint(1, 5)):
                pos = rng.randrange(len(base))
                base[pos] = rng.choice(alphabet)
            s = "".join(base)
        elif kind == 2:
            # Extreme numeric & separator patterns
            nums = [str(rng.choice([0, 1, 13, 31, 32, 99, 999, 2026, 9999, 10000, 9999999999])) for _ in range(rng.randint(1, 5))]
            sep = rng.choice(["-", "/", ".", ":", " ", "T", "W"])
            s = sep.join(nums)
        else:
            tok = rng.choice([".", "-", ":", "/", "9", " ", "ago", "gün", "年"])
            s = tok * rng.randint(1, 200)

        res = dateoxide.parse(s, settings={"RELATIVE_BASE": FIXED_BASE})
        if res is not None:
            assert isinstance(res, datetime)
            assert 1 <= res.year <= 9999


def test_fuzz_search_dates_500_documents():
    rng = random.Random(777)
    prose_words = [
        "alpha", "beta", "release", "version", "server", "let", "can", "see",
        "her", "man", "did", "say", "status", "ok", "price", "check", "update",
    ]
    date_snippets = [
        "2026-11-26",
        "12 December 2025",
        "yesterday at 14:00",
        "3 gün önce",
        "tomorrow",
        "2026-W48-4",
        "192.168.1.1",
        "Python 3.12.1",
        "$19.99",
        "v0.1.0",
    ]

    for _ in range(500):
        parts = []
        for _ in range(rng.randint(2, 10)):
            if rng.random() < 0.7:
                parts.append(rng.choice(prose_words))
            else:
                parts.append(rng.choice(date_snippets))
        doc = " ".join(parts)
        matches = search_dates(doc, settings={"RELATIVE_BASE": FIXED_BASE})
        if matches is not None:
            assert isinstance(matches, list)
            for txt, dt in matches:
                assert isinstance(txt, str) and len(txt) > 0
                assert isinstance(dt, datetime)
                assert 1 <= dt.year <= 9999


def test_fuzz_parse_batch_matches_sequential():
    rng = random.Random(2026)
    samples = [
        "2026-11-26T15:30:00Z",
        "2026-W48-4",
        "12 December 2025",
        "3 gün önce",
        "0000-01-01",
        "9999-12-31",
        "not a date",
        "",
        "   ",
        "1 year, 2 months ago",
    ]
    for _ in range(390):
        if rng.random() < 0.5:
            dt = _random_naive_dt(rng)
            samples.append(dt.strftime(rng.choice(["%Y-%m-%d", "%Y-%m-%d %H:%M", "%d/%m/%Y"])))
        else:
            samples.append("".join(rng.choice("abc0123456789-/: \x00") for _ in range(rng.randint(1, 25))))

    cfg = {"RELATIVE_BASE": FIXED_BASE, "DATE_ORDER": "DMY"}
    batch_res = dateoxide.parse_batch(samples, settings=cfg)
    seq_res = [dateoxide.parse(s, settings=cfg) for s in samples]
    assert batch_res == seq_res
