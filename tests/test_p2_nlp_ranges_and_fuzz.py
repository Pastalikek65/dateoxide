import random
from datetime import datetime
import dateparser
import dateoxide
from dateoxide import (
    DateDataParser,
    parse_batch,
    parse_date_range,
    parse_hijri,
    parse_jalali,
    search_dates,
)

BASE = datetime(2026, 3, 15, 12, 0, 0)  # Sunday
ST = {"RELATIVE_BASE": BASE}


def test_p2_1_period_boundaries_quarters_seasons():
    cases = [
        ("end of month", None, datetime(2026, 3, 31, 0, 0)),
        ("start of month", None, datetime(2026, 3, 1, 0, 0)),
        ("end of year", None, datetime(2026, 12, 31, 0, 0)),
        ("beginning of year", None, datetime(2026, 1, 1, 0, 0)),
        ("mid-July", ["en"], datetime(2026, 7, 15, 0, 0)),
        ("ay sonu", ["tr"], datetime(2026, 3, 31, 0, 0)),
        ("ay başı", ["tr"], datetime(2026, 3, 1, 0, 0)),
        ("yıl sonu", ["tr"], datetime(2026, 12, 31, 0, 0)),
        ("Ende des Monats", ["de"], datetime(2026, 3, 31, 0, 0)),
        ("Monatsende", ["de"], datetime(2026, 3, 31, 0, 0)),
        ("fin du mois", ["fr"], datetime(2026, 3, 31, 0, 0)),
        ("début du mois", ["fr"], datetime(2026, 3, 1, 0, 0)),
        ("月末", ["ja", "zh"], datetime(2026, 3, 31, 0, 0)),
        ("Q1 2026", None, datetime(2026, 1, 1, 0, 0)),
        ("2026-Q4", None, datetime(2026, 10, 1, 0, 0)),
        ("2026 3. çeyrek", ["tr"], datetime(2026, 7, 1, 0, 0)),
        ("3. Quartal 2026", ["de"], datetime(2026, 7, 1, 0, 0)),
        ("2026年第2四半期", ["ja"], datetime(2026, 4, 1, 0, 0)),
        ("2026년 2분기", ["ko"], datetime(2026, 4, 1, 0, 0)),
        ("summer 2026", ["en"], datetime(2026, 6, 1, 0, 0)),
        ("winter 2026", ["en"], datetime(2026, 12, 1, 0, 0)),
    ]
    for expr, langs, exp in cases:
        got = dateoxide.parse(expr, languages=langs, settings=ST)
        assert got == exp, f"{expr!r} -> {got!r}, expected {exp!r}"


def test_p2_1_chained_relatives_business_days_fractionals_and_clocks():
    cases = [
        # Chained & ordinal relatives
        ("first Monday of May 2026", ["en"], datetime(2026, 5, 4, 0, 0)),
        ("last Friday of March 2026", ["en"], datetime(2026, 3, 27, 0, 0)),
        ("two weeks from Tuesday", ["en"], datetime(2026, 3, 24, 0, 0)),
        ("a week from tomorrow", ["en"], datetime(2026, 3, 23, 0, 0)),
        ("haftaya salı", ["tr"], datetime(2026, 3, 24, 0, 0)),
        ("übernächsten Freitag", ["de"], datetime(2026, 3, 27, 0, 0)),
        # Business / working days (BASE is Sunday 2026-03-15 -> -3 biz days = Wed 2026-03-11; +5 biz days = Fri 2026-03-20)
        ("3 business days ago", ["en"], datetime(2026, 3, 11, 12, 0)),
        ("in 5 working days", ["en"], datetime(2026, 3, 20, 12, 0)),
        ("3 iş günü önce", ["tr"], datetime(2026, 3, 11, 12, 0)),
        ("in 5 Werktagen", ["de"], datetime(2026, 3, 20, 12, 0)),
        ("dans 5 jours ouvrés", ["fr"], datetime(2026, 3, 20, 12, 0)),
        ("3営業日前", ["ja"], datetime(2026, 3, 11, 12, 0)),
        # Indefinite & multilingual fractionals
        ("birkaç gün önce", ["tr"], datetime(2026, 3, 12, 12, 0)),
        ("полтора часа назад", ["ru"], datetime(2026, 3, 15, 10, 30)),
        ("1時間半前", ["ja"], datetime(2026, 3, 15, 10, 30)),
        ("3点半", ["zh"], datetime(2026, 3, 15, 3, 30)),
        ("3시 반", ["ko"], datetime(2026, 3, 15, 3, 30)),
        # Conversational clocks & compound day + TOD + time
        ("10 past 8", ["en"], datetime(2026, 3, 15, 8, 10)),
        ("5 to 4", ["en"], datetime(2026, 3, 15, 3, 55)),
        ("dörde çeyrek kala", ["tr"], datetime(2026, 3, 15, 3, 45)),
        ("üçü yirmi geçe", ["tr"], datetime(2026, 3, 15, 3, 20)),
        ("halb acht", ["de"], datetime(2026, 3, 15, 7, 30)),
        ("Viertel vor drei", ["de"], datetime(2026, 3, 15, 2, 45)),
        ("Viertel nach drei", ["de"], datetime(2026, 3, 15, 3, 15)),
        ("Yesterday morning at 10am", ["en"], datetime(2026, 3, 14, 10, 0)),
    ]
    for expr, langs, exp in cases:
        got = dateoxide.parse(expr, languages=langs, settings=ST)
        assert got == exp, f"{expr!r} -> {got!r}, expected {exp!r}"


def test_p2_2_ranges_v2():
    # ISO-8601 intervals
    assert parse_date_range("2026-05-01/2026-05-15", settings=ST) == (
        datetime(2026, 5, 1, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )
    assert parse_date_range("2026-05-01/P14D", settings=ST) == (
        datetime(2026, 5, 1, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )
    # Shared-month word ranges across languages
    assert parse_date_range("10 to 15 May 2026", settings=ST) == (
        datetime(2026, 5, 10, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )
    assert parse_date_range("du 10 au 15 mai 2026", languages=["fr"], settings=ST) == (
        datetime(2026, 5, 10, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )
    assert parse_date_range("del 10 al 15 de mayo de 2026", languages=["es"], settings=ST) == (
        datetime(2026, 5, 10, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )
    assert parse_date_range("10 bis 15 Mai 2026", languages=["de"], settings=ST) == (
        datetime(2026, 5, 10, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )
    assert parse_date_range("с 10 по 15 мая 2026", languages=["ru"], settings=ST) == (
        datetime(2026, 5, 10, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )
    assert parse_date_range("5月10日到5月15日", languages=["zh"], settings=ST) == (
        datetime(2026, 5, 10, 0, 0),
        datetime(2026, 5, 15, 0, 0),
    )
    # Tight time ranges & proper end-of-month endpoint
    assert parse_date_range("14:00-16:30", settings=ST) == (
        datetime(2026, 3, 15, 14, 0),
        datetime(2026, 3, 15, 16, 30),
    )
    assert parse_date_range("May 2026 - July 2026", settings=ST) == (
        datetime(2026, 5, 1, 0, 0),
        datetime(2026, 7, 31, 0, 0),
    )


def test_p2_3_buddhist_era_partial_calendars_spans_and_batch_generators():
    # Thai Buddhist Era (2567 BE = 2024 CE)
    assert dateoxide.parse("พ.ศ. 2567", languages=["th"], settings=ST).year == 2024
    assert dateoxide.parse("15 พฤษภาคม 2567", languages=["th"], settings=ST) == datetime(2024, 5, 15)

    # Partial Jalali & Hijri
    dj = parse_jalali("آذر ۱۴۰۴", settings=ST)
    assert dj.date_obj is not None and dj.period == "month"
    dh = parse_hijri("رمضان 1446", settings=ST)
    assert dh.date_obj is not None and dh.period == "month" and dh.date_obj.date().isoformat() == "2025-03-01"

    # Spaceless SE Asian scripts in search_dates
    res_th = search_dates("พบกัน15พฤษภาคม2024ที่ทำงาน", languages=["th"], settings=ST)
    assert res_th is not None and any(d == datetime(2024, 5, 15) for _, d in res_th)

    # search_dates(..., return_spans=True) exact Python char offsets on multi-byte text
    sample = "Öğle 2026-05-15 toplantısı 🎉"
    sp = search_dates(sample, return_spans=True, settings=ST)
    assert sp is not None and len(sp) == 1
    matched_text, dt, span = sp[0]
    assert dt == datetime(2026, 5, 15)
    assert sample[span[0]:span[1]] == matched_text

    # Generator support in parse_batch & parse_batch_timestamps
    gen = (x for x in ("2026-05-01", None, "2026-05-02"))
    assert parse_batch(gen, settings=ST) == [datetime(2026, 5, 1), None, datetime(2026, 5, 2)]
    ts = dateoxide.parse_batch_timestamps(["2026-05-01", None], settings=ST)
    assert len(ts) == 2 and isinstance(ts[0], float) and ts[1] is None


def test_p2_5_seeded_unicode_stress_and_differential_fuzz_vs_dateparser():
    rng = random.Random(20260315)
    weird_chunks = [
        "\uFEFF", "\u200B", "\u200E", "\u200F", "\u00A0", "\u202F", "\u3000", "İ", "ğ", "ü",
        "ش", "١", "๕", "年", "月", "日", "년", "🎉", "\n", "\t", "-", "/", ":", ".", ",",
        "(", ")", "令和6年", "Q2", "2026", "May", "15", "ago", "vor", "dans", "p.m.", "UTC",
    ]
    # 550 randomized Unicode stress inputs: must never panic or raise unhandled exceptions
    for _ in range(550):
        k = rng.randint(1, 14)
        s = "".join(rng.choice(weird_chunks) for _ in range(k))
        res = dateoxide.parse(s, settings=ST)
        assert res is None or isinstance(res, datetime)
        sd = search_dates(s, settings=ST)
        assert sd is None or isinstance(sd, list)

    # Differential checks on canonical & tricky real dates vs dateparser 1.5.1
    diff_cases = [
        "2024-03-15",
        "15 March 2024",
        "March 15, 2024 14:30",
        "2 days ago",
        "in 3 weeks",
        "15 май 2024",
        "2024年5月15日",
        "15 mai 2024",
        "15 de mayo de 2024",
        "not a date at all 999999999",
    ]
    for text in diff_cases:
        do_res = dateoxide.parse(text, settings=ST)
        dp_res = dateparser.parse(text, settings=ST)
        if dp_res is None:
            assert do_res is None, f"False positive vs dateparser on {text!r}: {do_res!r}"
        else:
            assert do_res is not None and do_res.date() == dp_res.date(), (
                f"Mismatch on {text!r}: dateoxide={do_res!r} dateparser={dp_res!r}"
            )
