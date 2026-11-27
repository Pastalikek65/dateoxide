import pickle
from datetime import datetime, timezone
from zoneinfo import ZoneInfo
import pytest
import dateoxide
from dateoxide import DateData, DateDataParser, Settings, SettingValidationError, search_dates

BASE = datetime(2026, 3, 15, 12, 0, 0)  # Sunday
ST = {"RELATIVE_BASE": BASE}


def test_p1_aware_relative_base():
    aware_utc = datetime(2026, 3, 15, 12, 0, tzinfo=timezone.utc)
    aware_ny = datetime(2026, 3, 15, 12, 0, tzinfo=ZoneInfo("America/New_York"))
    assert dateoxide.parse("2 days ago", settings={"RELATIVE_BASE": aware_utc}) == datetime(2026, 3, 13, 12, 0)
    assert dateoxide.parse("2 days ago", settings={"RELATIVE_BASE": aware_ny}) == datetime(2026, 3, 13, 12, 0)


def test_p1_explicit_languages_filter_never_bypassed():
    # Conversational clock/fraction
    assert dateoxide.parse("quarter past 3", languages=["tr"], settings=ST) is None
    assert dateoxide.parse("saat 3 buçuk", languages=["en"], settings=ST) is None
    # CJK absolute
    assert dateoxide.parse("2026年11月26日", languages=["en"], settings=ST) is None
    # CJK relative weekday
    assert dateoxide.parse("先週の月曜日", languages=["en"], settings=ST) is None
    # Prefixed time-only
    assert dateoxide.parse("at 5:30 PM", languages=["tr"], settings=ST) is None
    assert dateoxide.parse("saat 14:30", languages=["en"], settings=ST) is None
    # Custom date_formats %b in non-matching language
    assert dateoxide.parse("26 Nov 2026", date_formats=["%d %b %Y"], languages=["tr"], settings=ST) is None


def test_p1_false_positives_and_currency_hijacks():
    for bad in (
        "25 EUR",
        "100 EUR",
        "19.99 EUR",
        "25 SEK",
        "100 SEK",
        "50 NGN",
        "5 men",
        "12 men",
        "5 mi",
        "10 kat",
        "10 Mal",
        "5 mer",
        "10 lab",
        "5 bio",
        "10 inn",
        "20 mug",
        "5 hat",
        "10 pt",
    ):
        assert dateoxide.parse(bad, settings=ST) is None, f"False positive on {bad!r}"

    assert search_dates("Ürün fiyatı 10.05 EUR civarında.", settings=ST) is None
    # <2-digit> <weekday> must NOT become a 2-digit year 2025
    dt_sun = dateoxide.parse("25 Sun", settings=ST)
    assert dt_sun is None or dt_sun.year == 2026
    # Underflow must return None cleanly, never hijack Quechua August
    assert dateoxide.parse("1 day ago", settings={"RELATIVE_BASE": datetime(1, 1, 1)}) is None


def test_p1_arabic_folded_am_pm_and_feb29_leap_resolution():
    assert dateoxide.parse("15 مارس 2026 10:00 مساء", languages=["ar"]) == datetime(2026, 3, 15, 22, 0)
    assert dateoxide.parse("15 مارس 2026 10:00 صباحا", languages=["ar"]) == datetime(2026, 3, 15, 10, 0)
    res_ar = search_dates("الاجتماع يوم 15 مارس 2026 الساعة 10:00 مساء", languages=["ar"])
    assert res_ar is not None and res_ar[0][1] == datetime(2026, 3, 15, 22, 0)

    # Yearless February 29 resolves to nearest leap year
    assert dateoxide.parse("February 29", settings={"RELATIVE_BASE": BASE, "PREFER_DATES_FROM": "past"}) == datetime(2024, 2, 29)
    assert dateoxide.parse("February 29", settings={"RELATIVE_BASE": BASE, "PREFER_DATES_FROM": "current_period"}) == datetime(2024, 2, 29)
    assert dateoxide.parse("February 29", settings={"RELATIVE_BASE": BASE, "PREFER_DATES_FROM": "future"}) == datetime(2028, 2, 29)


def test_p1_strict_date_order_all_enforces_year_position():
    for order in ("YMD", "YDM", "MYD", "DYM"):
        st = {**ST, "DATE_ORDER": order, "STRICT_DATE_ORDER": "all"}
        assert dateoxide.parse("01/02/2026", settings=st) is None, f"Leaked 01/02/2026 under {order}"
        assert dateoxide.parse("15 March 2026", settings=st) is None, f"Leaked text under {order}"


def test_p1_unicode_controls_fullwidth_and_emoji():
    for s in (
        "\uFEFF2024-03-15",
        "\uFEFF15 March 2024",
        "\u200E2024-03-15\u200E",
        "2024\u200F-\u200F03\u200F-\u200F15",
        "\u202A15 March 2024\u202C",
        "2024-03-15\u200B",
        "2024\u00AD-03-\u00AD15",
        "2024／03／15",
        "2024－03－15",
        "2024-03-15 14：30：00",
        "March 15， 2024",
        "📅2024-03-15",
        "📅 15 March 2024 🎉",
    ):
        dt = dateoxide.parse(s, settings=ST)
        assert dt is not None and dt.date().isoformat() == "2024-03-15", f"Failed Unicode input {s!r}: {dt!r}"


def test_p1_misc_accuracy_and_unicode_digits_in_search():
    # Fractional month ~45 days consistent with "one and a half months ago"
    dt_f = dateoxide.parse("1.5 months ago", settings={"RELATIVE_BASE": datetime(2026, 5, 13, 14, 0)})
    assert dt_f == datetime(2026, 3, 29, 14, 0)

    # German decimal comma
    assert dateoxide.parse("vor 1,5 Stunden", languages=["de"], settings=ST) == datetime(2026, 3, 15, 10, 30)

    # CJK quarters must not misparse "2분기" as 2 minutes or "第2四半期" as day 2
    dt_ko_q = dateoxide.parse("2026년 2분기", settings=ST)
    assert dt_ko_q is None or dt_ko_q.minute == 0
    dt_ja_q = dateoxide.parse("2026年第2四半期", settings=ST)
    assert dt_ja_q is None or dt_ja_q.day != 2

    # 4-digit years outside 1000..=2999 in text dates
    assert dateoxide.parse("15 March 0099") == datetime(99, 3, 15)
    assert dateoxide.parse("15 March 9999") == datetime(9999, 3, 15)

    # Invalid 24h+pm rejected in absolute.rs
    assert dateoxide.parse("12 December 2025 13:00 pm") is None

    # Unicode uppercase prepositions À / В
    assert dateoxide.parse("À 14:30", settings=ST) == datetime(2026, 3, 15, 14, 30)
    assert dateoxide.parse("В 14:30", settings=ST) == datetime(2026, 3, 15, 14, 30)

    # Boundary PREFER_DATES_FROM on today's weekday/date/month
    assert dateoxide.parse("Sunday", settings={"RELATIVE_BASE": BASE, "PREFER_DATES_FROM": "past"}) == datetime(2026, 3, 8)
    assert dateoxide.parse("Sunday", settings={"RELATIVE_BASE": BASE, "PREFER_DATES_FROM": "future"}) == datetime(2026, 3, 22)
    assert dateoxide.parse("March 15", settings={"RELATIVE_BASE": BASE, "PREFER_DATES_FROM": "future"}) == datetime(2027, 3, 15)

    # Thai เวลา in search_dates & German dative plural Tagen in RETURN_TIME_SPAN
    res_th = search_dates("นัดประชุมวันที่ 15 พฤษภาคม 2024 เวลา 14:30 น.", languages=["th"], settings=ST)
    assert res_th is not None and any(dt == datetime(2024, 5, 15, 14, 30) for _, dt in res_th)
    res_de_span = search_dates("In den letzten 3 Tagen", settings={**ST, "RETURN_TIME_SPAN": True})
    assert res_de_span is not None and len(res_de_span) == 2 and "(start)" in res_de_span[0][0]

    # Non-ASCII Unicode decimal scripts in search_dates
    for txt, lang in [
        ("Updated ٢٠٢٤-٠٥-١٥ today", "ar"),
        ("جلسه در تاریخ ۱۵ می ۲۰۲۴ برگزار می‌شود", "fa"),
        ("बैठक १५ मई २०२४ को है", "hi"),
        ("সভা ১৫ মে ২০২৪ তারিখে", "bn"),
        ("ประชุมวันที่ ๑๕ พฤษภาคม ๒๐๒๔", "th"),
        ("កិច្ចប្រជុំ ១៥ ឧសភា ២០២៤", "km"),
        ("အစည်းအဝေး ၁၅ မေ ၂၀၂၄", "my"),
        ("会議は２０２４－０５－１５です", "ja"),
    ]:
        found = search_dates(txt, languages=[lang], settings=ST)
        assert found is not None and any(d.date().isoformat() == "2024-05-15" for _, d in found), (
            f"search_dates failed on Unicode digits for {lang}: {found!r}"
        )


def test_p1_python_api_datedata_equality_pickle_errors_and_stubs():
    d1 = DateData(date_obj=datetime(2026, 3, 15), period="day", locale="en", parts=("year", "month", "day"))
    d2 = DateData(date_obj=datetime(2026, 3, 15), period="day", locale="en", parts=("year", "month", "day"))
    assert d1 == d2
    assert pickle.loads(pickle.dumps(d1)) == d1

    # Do not mask non-TypeError exceptions in detect_languages_function
    def broken_detector(text, conf=0.5):
        return 1 / 0  # ZeroDivisionError

    with pytest.raises(ZeroDivisionError):
        dateoxide.parse("2026-03-15", detect_languages_function=broken_detector)

    # DateDataParser.parse_batch and parse_date_range honor detect_languages_function
    p = DateDataParser(detect_languages_function=lambda t, c=0.5: ["tr"])
    assert p.parse_batch(["05/06/2026"]) == [datetime(2026, 6, 5)]

    # Invalid setting types/bounds raise SettingValidationError
    with pytest.raises(SettingValidationError):
        dateoxide.parse("2026-03-15", settings={"STRICT_DATE_ORDER": True})
    with pytest.raises(SettingValidationError):
        dateoxide.parse("2026-03-15", settings={"CACHE_SIZE_LIMIT": -1})
    with pytest.raises(SettingValidationError):
        dateoxide.parse("2026-03-15", settings={"DEFAULT_DAYS_IN_MONTH": 0})
    with pytest.raises(ValueError):
        search_dates("2026-03-15", strategy="invalid_strategy_xyz")
    with pytest.raises(SettingValidationError):
        dateoxide.parse("2026-03-15", settings={"FUZZY": True})
