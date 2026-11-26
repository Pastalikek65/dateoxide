from __future__ import annotations

from datetime import datetime
import dateoxide
from dateoxide.date import DateDataParser
from dateoxide.search import search_dates

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


def test_parse_batch_parallel_consistency():
    samples = [
        "2026-11-26T15:30:00Z",
        "12 December 2025",
        "3 gün önce",
        "vor 2 Wochen",
        "hier à 14h30",
        "not a date",
        "2026年11月26日",
        "12 de diciembre de 2025",
    ] * 200

    settings = {"RELATIVE_BASE": FIXED_BASE}
    batch_res = dateoxide.parse_batch(samples, settings=settings)
    assert len(batch_res) == len(samples)

    for i in range(8):
        single = dateoxide.parse(samples[i], settings=settings)
        assert batch_res[i] == single
    assert batch_res[5] is None


def test_date_data_parser_batch():
    ddp = DateDataParser(settings={"RELATIVE_BASE": FIXED_BASE})
    res = ddp.parse_batch(["2026-11-26", "3 days ago", "invalid"])
    assert res[0] == datetime(2026, 11, 26)
    assert res[1] == datetime(2026, 11, 23, 15, 0, 0)
    assert res[2] is None


def test_search_dates_english_turkish_and_detected_language():
    text_en = "The contract starts on 12 December 2025 and the draft was sent 3 days ago."
    found_en = search_dates(text_en, settings={"RELATIVE_BASE": FIXED_BASE})
    assert found_en is not None
    assert len(found_en) == 2
    assert found_en[0][1].date() == datetime(2025, 12, 12).date()
    assert found_en[1][1].date() == datetime(2026, 11, 23).date()

    found_with_lang = search_dates(
        text_en,
        settings={"RELATIVE_BASE": FIXED_BASE},
        add_detected_language=True,
    )
    assert found_with_lang is not None
    assert len(found_with_lang) == 2
    assert found_with_lang[0][2] == "en"
    assert found_with_lang[1][2] == "en"

    text_tr = "Toplantı 15 Ocak 2026 tarihinde yapılacak, önceki görüşme 3 gün önce bitti."
    found_tr = search_dates(
        text_tr,
        settings={"RELATIVE_BASE": FIXED_BASE},
        add_detected_language=True,
    )
    assert found_tr is not None
    assert len(found_tr) == 2
    assert found_tr[0][1].date() == datetime(2026, 1, 15).date()
    assert found_tr[0][2] == "tr"
    assert found_tr[1][1].date() == datetime(2026, 11, 23).date()
    assert found_tr[1][2] == "tr"

    assert search_dates("Nothing remotely resembling a calendar here.") is None
