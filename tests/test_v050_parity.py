import sys
import types
import warnings
from datetime import datetime, timezone

import pytest

import dateoxide
from dateoxide import DateDataParser, SettingValidationError


def test_locale_region_date_order():
    assert dateoxide.parse("05/06/2026", locales=["en-GB"]) == datetime(2026, 6, 5, 0, 0)
    assert dateoxide.parse("05/06/2026", locales=["en_GB"]) == datetime(2026, 6, 5, 0, 0)
    assert dateoxide.parse("05/06/2026", locales=["en-AU"]) == datetime(2026, 6, 5, 0, 0)
    assert dateoxide.parse("05/06/2026", locales=["en-CA"]) == datetime(2026, 6, 5, 0, 0)
    assert dateoxide.parse("05/06/2026", locales=["en-US"]) == datetime(2026, 5, 6, 0, 0)
    assert dateoxide.parse("05/06/2026", locales=["tr"]) == datetime(2026, 6, 5, 0, 0)

    # Explicit region/DATE_ORDER overrides locale tag
    assert dateoxide.parse(
        "05/06/2026",
        locales=["en-GB"],
        settings={"DATE_ORDER": "MDY"},
    ) == datetime(2026, 5, 6, 0, 0)

    # PREFER_LOCALE_DATE_ORDER=False falls back to MDY
    assert dateoxide.parse(
        "05/06/2026",
        locales=["en-GB"],
        settings={"PREFER_LOCALE_DATE_ORDER": False},
    ) == datetime(2026, 5, 6, 0, 0)


def test_try_previous_locales_stateful_parser():
    ddp = DateDataParser(try_previous_locales=True)

    r1 = ddp.get_date_data("15 Ocak 2026")
    assert r1.date_obj == datetime(2026, 1, 15, 0, 0)
    assert r1.locale == "tr"

    # Remembers Turkish DMY order & locale on ambiguous numeric date
    r2 = ddp.get_date_data("05/06/2026")
    assert r2.date_obj == datetime(2026, 6, 5, 0, 0)
    assert r2.locale == "tr"

    # Switching to an English date promotes 'en' to front
    r3 = ddp.get_date_data("15 January 2026")
    assert r3.date_obj == datetime(2026, 1, 15, 0, 0)
    assert r3.locale == "en"

    r4 = ddp.get_date_data("05/06/2026")
    assert r4.date_obj == datetime(2026, 5, 6, 0, 0)
    assert r4.locale == "en"


def test_apostrophe_two_digit_years():
    assert dateoxide.parse("Nov 26 '26") == datetime(2026, 11, 26, 0, 0)
    assert dateoxide.parse("15 Jan ’99") == datetime(1999, 1, 15, 0, 0)
    dd = DateDataParser().get_date_data("Nov '26")
    assert dd.period == "month"
    assert dd.date_obj is not None
    assert (dd.date_obj.year, dd.date_obj.month) == (2026, 11)


def test_japanese_imperial_era_years():
    ddp = DateDataParser()
    dd = ddp.get_date_data("令和8年11月26日")
    assert dd.date_obj == datetime(2026, 11, 26, 0, 0)
    assert dd.locale == "ja"

    assert dateoxide.parse("令和元年5月1日") == datetime(2019, 5, 1, 0, 0)
    assert dateoxide.parse("平成31年4月30日") == datetime(2019, 4, 30, 0, 0)
    assert dateoxide.parse("昭和64年1月7日") == datetime(1989, 1, 7, 0, 0)
    assert dateoxide.parse("大正15年12月25日") == datetime(1926, 12, 25, 0, 0)
    assert dateoxide.parse("明治45年7月30日") == datetime(1912, 7, 30, 0, 0)
    assert dateoxide.parse("令和0年11月26日") is None


def test_italian_and_european_connectors():
    dd = DateDataParser().get_date_data("il 15 gennaio 2026 alle 14:30")
    assert dd.date_obj == datetime(2026, 1, 15, 14, 30)
    assert dd.locale == "it"

    assert dateoxide.parse("el 15 de enero de 2026 a las 14:30") == datetime(2026, 1, 15, 14, 30)


def test_negative_timestamp_and_microsecond_timestamp():
    assert dateoxide.parse("-86400") is None

    res_sec = dateoxide.parse("-86400", settings={"PARSERS": ["negative-timestamp"]})
    assert res_sec == datetime(1969, 12, 31, 0, 0, tzinfo=timezone.utc)

    res_frac = dateoxide.parse("-86400.5", settings={"PARSERS": ["negative-timestamp"]})
    assert res_frac == datetime(1969, 12, 30, 23, 59, 59, 500000, tzinfo=timezone.utc)

    res_neg_ms = dateoxide.parse("-8640000000000", settings={"PARSERS": ["negative-timestamp"]})
    assert res_neg_ms == datetime(1696, 3, 17, 0, 0, tzinfo=timezone.utc)

    res_pos_us = dateoxide.parse("1700000000123456")
    assert res_pos_us == datetime.fromtimestamp(1700000000.123456, tz=timezone.utc)


def test_localized_custom_date_formats():
    dd_tr = DateDataParser(settings={"PARSERS": ["custom-formats"]}).get_date_data(
        "15 Ocak 2026",
        date_formats=["%d %B %Y"],
    )
    assert dd_tr.date_obj == datetime(2026, 1, 15, 0, 0)
    assert dd_tr.locale == "tr"

    dd_fr = DateDataParser(settings={"PARSERS": ["custom-formats"]}).get_date_data(
        "15 janvier 2026",
        date_formats=["%d %B %Y"],
    )
    assert dd_fr.date_obj == datetime(2026, 1, 15, 0, 0)
    assert dd_fr.locale == "fr"

    # Strict custom-formats does not fall back when format mismatches
    assert (
        dateoxide.parse(
            "15-01-2026",
            date_formats=["%d %B %Y"],
            settings={"PARSERS": ["custom-formats"]},
        )
        is None
    )


def test_confidence_threshold_forwarded_and_validated():
    seen = []

    def detector(text, confidence_threshold):
        seen.append(confidence_threshold)
        return ["tr"]

    dt = dateoxide.parse(
        "15 Ocak 2026",
        settings={"LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD": 0.85},
        detect_languages_function=detector,
    )
    assert dt == datetime(2026, 1, 15, 0, 0)
    assert seen == [0.85]

    with pytest.raises(SettingValidationError):
        dateoxide.parse("15 Ocak 2026", settings={"LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD": 1.5})
    with pytest.raises(SettingValidationError):
        dateoxide.parse("15 Ocak 2026", settings={"LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD": -0.1})


def test_custom_language_detection_module(monkeypatch):
    from dateoxide.custom_language_detection import language_mapping

    mapped = language_mapping.map_languages(["no", "iw", "in", "ji", "zh-CN", "tr-TR", "en"])
    assert mapped == ["nb", "he", "id", "yi", "zh", "tr", "en"]

    with warnings.catch_warnings(record=True) as rec:
        warnings.simplefilter("always")
        from dateoxide.custom_language_detection import fasttext

        assert any(issubclass(w.category, DeprecationWarning) for w in rec)
        with pytest.raises(ImportError):
            fasttext.detect_languages("hello", 0.5)

    # Mock langdetect module cleanly to test adapter behavior without external wheel
    fake_mod = types.ModuleType("langdetect")

    class _FakeCandidate:
        def __init__(self, lang, prob):
            self.lang = lang
            self.prob = prob

    class _FakeDetector:
        def append(self, _text):
            pass

        def get_probabilities(self):
            return [_FakeCandidate("tr", 0.9), _FakeCandidate("de", 0.3)]

    class _FakeFactoryObj:
        seed = 0

        def load_profile(self, _dir):
            pass

        def create(self):
            return _FakeDetector()

    fake_mod.detector_factory = types.SimpleNamespace(
        DetectorFactory=lambda: _FakeFactoryObj(),
        PROFILES_DIRECTORY=".",
    )
    fake_mod.lang_detect_exception = types.SimpleNamespace(LangDetectException=RuntimeError)

    monkeypatch.setitem(sys.modules, "langdetect", fake_mod)
    from dateoxide.custom_language_detection import langdetect as ox_ld

    ox_ld._Factory.data = None
    assert ox_ld.detect_languages("15 Ocak 2026", 0.5) == ["tr"]
    ox_ld._Factory.data = None


def test_js_ctime_and_world_tz_abbreviations():
    js1 = dateoxide.parse("Thu Nov 26 2026 14:30:00 GMT+0300 (GMT+03:00)")
    assert js1 is not None and js1.tzinfo is not None
    assert js1.replace(tzinfo=None) == datetime(2026, 11, 26, 14, 30)

    js2 = dateoxide.parse("Thu Nov 26 2026 14:30:00 GMT+0300 (Turkey Time)")
    assert js2 is not None and js2.tzinfo is not None
    assert js2.replace(tzinfo=None) == datetime(2026, 11, 26, 14, 30)

    assert dateoxide.parse("Meeting (urgent)") is None

    ct1 = dateoxide.parse("Thu Nov 26 14:30:00 UTC 2026")
    assert ct1 == datetime(2026, 11, 26, 14, 30, tzinfo=timezone.utc)

    ct2 = dateoxide.parse("Fri Nov 27 02:50:00 TRT 2026")
    assert ct2 is not None and ct2.tzinfo is not None
    assert ct2.replace(tzinfo=None) == datetime(2026, 11, 27, 2, 50)

    for abbr, offset_hrs in [("PKT", 5), ("WIB", 7), ("PHT", 8), ("CLT", -4), ("NPT", 5.75)]:
        dt = dateoxide.parse(f"15 Jan 2026 14:30 {abbr}")
        assert dt is not None and dt.tzinfo is not None
        assert dt.utcoffset().total_seconds() == offset_hrs * 3600


def test_no_spaces_12_14_and_short_idioms():
    assert dateoxide.parse("202601151430") is None
    assert dateoxide.parse("202601151430", settings={"PARSERS": ["no-spaces-time"]}) == datetime(
        2026, 1, 15, 14, 30
    )
    assert dateoxide.parse(
        "150120261430",
        settings={"PARSERS": ["no-spaces-time"], "DATE_ORDER": "DMY"},
    ) == datetime(2026, 1, 15, 14, 30)
    assert dateoxide.parse("20260115143045", settings={"PARSERS": ["no-spaces-time"]}) == datetime(
        2026, 1, 15, 14, 30, 45
    )

    base = datetime(2026, 11, 26, 15, 0, 0)
    st = {"RELATIVE_BASE": base}
    assert dateoxide.parse("just now", settings=st) == base
    assert dateoxide.parse("at 5:30 PM", settings=st) == datetime(2026, 11, 26, 17, 30)
    assert dateoxide.parse("saat 14:30", settings=st) == datetime(2026, 11, 26, 14, 30)
    assert dateoxide.parse("2026-11-26 t 14:30", settings=st) == datetime(2026, 11, 26, 14, 30)
    assert dateoxide.parse("tomorrow at 5", settings=st) == datetime(2026, 11, 27, 5, 0)
    assert dateoxide.parse("dün saat 14", settings=st) == datetime(2026, 11, 25, 14, 0)


def test_cjk_korean_and_multi_unit_relatives():
    base = datetime(2026, 11, 26, 15, 0, 0)
    st = {"RELATIVE_BASE": base}

    assert dateoxide.parse("3日2時間前", settings=st) == datetime(2026, 11, 23, 13, 0)
    assert dateoxide.parse("昨日の14:30", settings=st) == datetime(2026, 11, 25, 14, 30)
    assert dateoxide.parse("昨日の午後2時30分", settings=st) == datetime(2026, 11, 25, 14, 30)
    assert dateoxide.parse("어제 오후 2시 30분", settings=st) == datetime(2026, 11, 25, 14, 30)
    assert dateoxide.parse("2일 3시간 전", settings=st) == datetime(2026, 11, 24, 12, 0)
    assert dateoxide.parse("1h30m ago", settings=st) == datetime(2026, 11, 26, 13, 30)
    assert dateoxide.parse("2小时30分钟前", settings=st) == datetime(2026, 11, 26, 12, 30)


def test_partial_date_formats_and_search_cjk_custom_formats():
    base = datetime(2026, 11, 26, 15, 0, 0)
    st_custom = {"RELATIVE_BASE": base, "PARSERS": ["custom-formats"]}

    assert dateoxide.parse("November 2026", date_formats=["%B %Y"], settings=st_custom) == datetime(
        2026, 11, 26, 0, 0
    )
    assert dateoxide.parse("2026-11", date_formats=["%Y-%m"], settings=st_custom) == datetime(
        2026, 11, 26, 0, 0
    )
    assert dateoxide.parse("15 Ocak", date_formats=["%d %B"], settings=st_custom) == datetime(
        2026, 1, 15, 0, 0
    )
    assert dateoxide.parse("14:30", date_formats=["%H:%M"], settings=st_custom) == datetime(
        2026, 11, 26, 14, 30
    )
    assert dateoxide.parse("2026-330", date_formats=["%Y-%j"], settings=st_custom) == datetime(
        2026, 11, 26, 0, 0
    )

    st = {"RELATIVE_BASE": base}
    res_fmt = dateoxide.search_dates(
        "Release 2026_11_26 is live now.",
        settings=st,
        date_formats=["%Y_%m_%d"],
        strategy="ngram",
    )
    assert res_fmt == [
        ("2026_11_26", datetime(2026, 11, 26, 0, 0)),
        ("now", datetime(2026, 11, 26, 15, 0)),
    ]

    assert dateoxide.search_dates("締め切りは2026年11月26日です。", languages=["ja"], settings=st) == [
        ("2026年11月26日", datetime(2026, 11, 26, 0, 0))
    ]
    assert dateoxide.search_dates("会议定于2026年11月26日举行。", languages=["zh"], settings=st) == [
        ("2026年11月26日", datetime(2026, 11, 26, 0, 0))
    ]
    assert dateoxide.search_dates("2026年11月20日から2026年11月26日まで", languages=["ja"], settings=st) == [
        ("2026年11月20日", datetime(2026, 11, 20, 0, 0)),
        ("2026年11月26日", datetime(2026, 11, 26, 0, 0)),
    ]


def test_remaining_idioms_and_helper_modules():
    base = datetime(2026, 11, 26, 15, 0, 0)
    st = {"RELATIVE_BASE": base}

    assert dateoxide.parse("~3 days ago", settings=st) == datetime(2026, 11, 23, 15, 0)
    assert dateoxide.parse("about 2 hours ago", settings=st) == datetime(2026, 11, 26, 13, 0)
    assert dateoxide.parse("yaklaşık 2 saat önce", settings=st) == datetime(2026, 11, 26, 13, 0)
    assert dateoxide.parse("約2時間前", settings=st) == datetime(2026, 11, 26, 13, 0)
    assert dateoxide.parse("12 noon", settings=st) == datetime(2026, 11, 26, 12, 0)
    assert dateoxide.parse("12 midnight", settings=st) == datetime(2026, 11, 26, 0, 0)
    assert dateoxide.parse("next Monday morning", settings=st) == datetime(2026, 11, 30, 9, 0)
    assert dateoxide.parse("через полчаса", settings=st) == datetime(2026, 11, 26, 15, 30)
    assert dateoxide.parse("半小时前", settings=st) == datetime(2026, 11, 26, 14, 30)
    assert dateoxide.parse("منذ يومين", settings=st) == datetime(2026, 11, 24, 15, 0)
    assert dateoxide.parse("לפני שעתיים", settings=st) == datetime(2026, 11, 26, 13, 0)

    from datetime import timedelta
    from dateoxide import timezone_parser, utils

    assert utils.strip_braces("2026-11-26 (Thursday)") == "2026-11-26"
    assert utils.normalize_unicode("éàü") == "eau"
    assert utils.find_date_separator("%Y/%m/%d") == "/"
    assert utils.get_last_day_of_month(2024, 2) == 29
    assert utils.get_previous_leap_year(2026) == 2024
    assert utils.get_next_leap_year(2024) == 2028
    assert utils.apply_timezone(datetime(2026, 11, 26, 12, 0), "+03:00").hour == 15
    assert utils.localize_timezone(datetime(2026, 11, 26, 12, 0), "UTC").tzinfo == timezone.utc

    assert timezone_parser.word_is_tz("UTC")
    assert timezone_parser.is_timezone_token("+03:00")
    assert not timezone_parser.word_is_tz("hello")
    assert isinstance(timezone_parser.local_tz_offset, timedelta)
    converted = timezone_parser.convert_to_local_tz(datetime(2026, 11, 26, 12, 0), timedelta(0))
    assert converted.tzinfo is not None
