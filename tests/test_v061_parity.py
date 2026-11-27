from datetime import datetime
import pytest
import dateoxide
from dateoxide import DateDataParser, search_dates

BASE = datetime(2026, 11, 26, 15, 30, 0)
ST = {"RELATIVE_BASE": BASE}


def test_v061_date_formats_microseconds():
    dt = dateoxide.parse(
        "2024-05-15 14:30:00.123456",
        date_formats=["%Y-%m-%d %H:%M:%S.%f"],
    )
    assert dt == datetime(2024, 5, 15, 14, 30, 0, 123456)

    dt_half = dateoxide.parse(
        "2024-05-15 14:30:00.5",
        date_formats=["%Y-%m-%d %H:%M:%S.%f"],
    )
    assert dt_half == datetime(2024, 5, 15, 14, 30, 0, 500000)

    # Without fractional part under custom-formats only, format requiring .%f must not falsely match
    assert dateoxide.parse(
        "2024-05-15 14:30:00",
        date_formats=["%Y-%m-%d %H:%M:%S.%f"],
        settings={"PARSERS": ["custom-formats"]},
    ) is None


def test_v061_strict_parsing_allows_relative_dates():
    st = {**ST, "STRICT_PARSING": True}
    assert dateoxide.parse("yesterday", settings=st) == datetime(2026, 11, 25, 15, 30)
    assert dateoxide.parse("2 days ago", settings=st) == datetime(2026, 11, 24, 15, 30)
    assert dateoxide.parse("May 2024", settings=st) is None


def test_v061_attached_short_hour_relatives():
    assert dateoxide.parse("in 3h", settings=ST) == datetime(2026, 11, 26, 18, 30)
    assert dateoxide.parse("il y a 3h", settings=ST) == datetime(2026, 11, 26, 12, 30)
    assert dateoxide.parse("dans 3h", settings=ST) == datetime(2026, 11, 26, 18, 30)
    # Trailing time on relative-exact expressions still works
    assert dateoxide.parse("hier à 14h", languages=["fr"], settings=ST) == datetime(2026, 11, 25, 14, 0)


def test_v061_ro_sk_connectors_minguo_and_ja_hint():
    assert dateoxide.parse("15 mai 2024 la ora 15:30", languages=["ro"]) == datetime(2024, 5, 15, 15, 30)
    assert dateoxide.parse("15. mája 2024 o 15:30", languages=["sk"]) == datetime(2024, 5, 15, 15, 30)
    assert dateoxide.parse("民國113年5月15日", languages=["zh"]) == datetime(2024, 5, 15, 0, 0)
    dd_ja = DateDataParser(languages=["ja"], settings=ST).get_date_data("2024年5月15日14:30")
    assert dd_ja.date_obj == datetime(2024, 5, 15, 14, 30)
    assert dd_ja.locale == "ja"


def test_v061_search_dates_multilingual_connectors_and_cjk_relatives():
    res_es = search_dates("La reunión es el 15 de mayo de 2024 a las 15:30 o hace 3 días.", languages=["es"], settings=ST)
    assert res_es == [
        ("15 de mayo de 2024 a las 15:30", datetime(2024, 5, 15, 15, 30)),
        ("hace 3 días", datetime(2026, 11, 23, 15, 30)),
    ]

    res_ar = search_dates("الاجتماع في 15 مايو 2024 الساعة 14:30، تم التحديث قبل يومين.", languages=["ar"], settings=ST)
    assert res_ar == [
        ("15 مايو 2024 الساعة 14:30", datetime(2024, 5, 15, 14, 30)),
        ("قبل يومين", datetime(2026, 11, 24, 15, 30)),
    ]

    res_ja = search_dates("会議は2024年5月15日14:30からです。3日前に通知済み。", languages=["ja"], settings=ST)
    assert res_ja == [
        ("2024年5月15日14:30", datetime(2024, 5, 15, 14, 30)),
        ("3日前", datetime(2026, 11, 23, 15, 30)),
    ]

    res_zh = search_dates("会议定于2024年5月15日下午3点，2天前已发送邀请。", languages=["zh"], settings=ST)
    assert res_zh == [
        ("2024年5月15日下午3点", datetime(2024, 5, 15, 15, 0)),
        ("2天前", datetime(2026, 11, 24, 15, 30)),
    ]


def test_v061_unknown_language_raises_value_error_and_datedata_module():
    with pytest.raises(ValueError, match="Unknown language"):
        dateoxide.parse("2024-05-15", languages=["xx_nonexistent"])

    with pytest.raises(ValueError, match="Unknown language"):
        DateDataParser(settings={"DEFAULT_LANGUAGES": ["xx_bad"]})

    # Valid regional & alias codes still work
    assert dateoxide.parse("01/02/2024", locales=["en-GB"]) == datetime(2024, 2, 1, 0, 0)
    assert dateoxide.parse("01/02/2024", locales=["pt_BR"]) == datetime(2024, 2, 1, 0, 0)

    dd = DateDataParser().get_date_data("2024-05-15")
    assert type(dd).__module__ == "dateoxide.date"
