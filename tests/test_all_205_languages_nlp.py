import json
from datetime import datetime
from pathlib import Path

import pytest

import dateoxide
from dateoxide import DateDataParser

LOCALES_PATH = Path(__file__).resolve().parent.parent / "crates" / "dateoxide-core" / "data" / "locales.json"
with LOCALES_PATH.open(encoding="utf-8") as _f:
    ALL_LOCALES = json.load(_f)

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)
ST = {"RELATIVE_BASE": FIXED_BASE}


def _pick_affix(loc, sign: int):
    for pfx, sfx, sg, uid in loc["relative_affix"]:
        if sg == sign and (pfx or sfx):
            return pfx, sfx, uid
    raise AssertionError(f"{loc['code']} missing sign={sign} affix")


def test_all_205_locales_have_complete_nlp_metadata():
    assert len(ALL_LOCALES) == 205
    assert len(dateoxide.supported_languages()) == 205
    required = (
        "months",
        "weekdays",
        "am",
        "pm",
        "units",
        "ago",
        "in_words",
        "relative_exact",
        "relative_affix",
        "word_numbers",
        "time_of_day",
    )
    for loc in ALL_LOCALES:
        for fld in required:
            assert loc.get(fld), f"Locale {loc['code']} missing field {fld!r}"
        assert any(a[2] == -1 for a in loc["relative_affix"]), f"{loc['code']} missing past affix"
        assert any(a[2] == 1 for a in loc["relative_affix"]), f"{loc['code']} missing future affix"


@pytest.mark.parametrize("loc", ALL_LOCALES, ids=[x["code"] for x in ALL_LOCALES])
def test_every_language_full_nlp_end_to_end(loc):
    code = loc["code"]

    # 1. Absolute date with native month name
    jan_names = loc["months"][0]
    abs_text = f"15 {jan_names[0]} 2026"
    dt_abs = dateoxide.parse(abs_text, languages=[code], settings=ST)
    assert dt_abs is not None and (dt_abs.year, dt_abs.month, dt_abs.day) == (2026, 1, 15), (
        f"[{code}] abs failed: {abs_text!r}"
    )

    # 2. Exact relative phrase
    rel_phrase = loc["relative_exact"][0][0]
    dt_rel_exact = dateoxide.parse(rel_phrase, languages=[code], settings=ST)
    assert dt_rel_exact is not None, f"[{code}] relative_exact failed: {rel_phrase!r}"

    # 3. Dynamic past & future relative expressions
    p_pfx, p_sfx, _ = _pick_affix(loc, -1)
    past_text = f"{p_pfx}3{p_sfx}".strip()
    dt_past = dateoxide.parse(past_text, languages=[code], settings=ST)
    assert dt_past is not None and dt_past < FIXED_BASE, f"[{code}] past affix failed: {past_text!r}"

    f_pfx, f_sfx, _ = _pick_affix(loc, 1)
    fut_text = f"{f_pfx}3{f_sfx}".strip()
    dt_fut = dateoxide.parse(fut_text, languages=[code], settings=ST)
    assert dt_fut is not None and dt_fut > FIXED_BASE, f"[{code}] future affix failed: {fut_text!r}"

    # 4. Word-number relative expression
    wn_word, _wn_val = next((w, v) for w, v in loc["word_numbers"] if v in (2, 3, 1))
    wn_text = f"{p_pfx}{wn_word}{p_sfx}".strip()
    dt_wn = dateoxide.parse(wn_text, languages=[code], settings=ST)
    assert dt_wn is not None and dt_wn < FIXED_BASE, f"[{code}] word-number rel failed: {wn_text!r}"

    # 5. Time-of-day anchor expression combined with an exact relative day
    tod_phrase, tod_hour = loc["time_of_day"][0]
    expected_hour = 3 if tod_hour == 6 else tod_hour
    tod_expr = f"{rel_phrase} {tod_phrase}"
    dt_tod = dateoxide.parse(tod_expr, languages=[code], settings=ST)
    assert dt_tod is not None and dt_tod.hour == expected_hour, (
        f"[{code}] time_of_day failed: {tod_expr!r} -> {dt_tod!r} (expected hour {expected_hour})"
    )


def test_v060_time_period_and_iso_invalid_month_and_no_english_false_positives():
    ddp_tp = DateDataParser(settings={**ST, "RETURN_TIME_AS_PERIOD": True})
    for expr in ("2 hours ago", "30 minutes ago", "half an hour ago", "через полчаса"):
        dd = ddp_tp.get_date_data(expr)
        assert dd.date_obj is not None
        assert dd.period == "time", f"expected period='time' for {expr!r}, got {dd.period!r}"

    assert dateoxide.parse("2026-13-01", settings={**ST, "STRICT_DATE_ORDER": "all"}) is None
    assert dateoxide.parse("2026-13-01", settings=ST) == datetime(2026, 1, 13, 0, 0)
    assert dateoxide.parse("2026-13-01", settings={**ST, "DATE_ORDER": "YDM"}) == datetime(
        2026, 1, 13, 0, 0
    )

    # English prose false-positive guards remain clean despite 205-language enrichment
    assert dateoxide.search_dates("She can see her man, did you say?", settings=ST) is None
    assert dateoxide.search_dates("Let us go in or on to the end of it all.", settings=ST) is None
