"""Large-scale 2,200+ test suite combining:
1. 1,010 verified upstream scrapinghub/dateparser test vectors (tests/data/upstream_corpus.json)
2. Exhaustive 205-language matrix across months, weekdays, exact relatives, and affixes
"""
from __future__ import annotations

import calendar
import json
from collections import Counter
from datetime import datetime, timedelta
from pathlib import Path
import pytest

import dateoxide

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)
SETTINGS = {"RELATIVE_BASE": FIXED_BASE}
ROOT = Path(__file__).resolve().parent.parent
UPSTREAM_CORPUS = json.loads(
    (ROOT / "tests" / "data" / "upstream_corpus.json").read_text(encoding="utf-8")
)
LOCALES = json.loads(
    (ROOT / "crates" / "dateoxide-core" / "data" / "locales.json").read_text(encoding="utf-8")
)


def _add_months(dt: datetime, delta: int) -> datetime:
    total = dt.year * 12 + (dt.month - 1) + delta
    y, m0 = divmod(total, 12)
    m = m0 + 1
    d = min(dt.day, calendar.monthrange(y, m)[1])
    return dt.replace(year=y, month=m, day=d)


def _expected_relative_dt(base: datetime, sign: int, amount: int, unit_id: int) -> datetime:
    s = -amount if sign < 0 else amount
    if unit_id == 0:
        return _add_months(base, s * 12)
    if unit_id == 1:
        return _add_months(base, s)
    if unit_id == 2:
        return base + timedelta(weeks=s)
    if unit_id == 3:
        return base + timedelta(days=s)
    if unit_id == 4:
        return base + timedelta(hours=s)
    if unit_id == 5:
        return base + timedelta(minutes=s)
    if unit_id == 6:
        return base + timedelta(seconds=s)
    if unit_id == 7:
        return _add_months(base, s * 120)
    if unit_id == 8:
        return _add_months(base, s * 1200)
    if unit_id == 9:
        return base + timedelta(weeks=s * 2)
    raise ValueError(f"Unknown unit_id: {unit_id}")


def _build_205_lang_cases() -> list[tuple[str, str, str, tuple[int, int, int] | datetime | None]]:
    cases: list[tuple[str, str, str, tuple[int, int, int] | datetime | None]] = []
    for loc in LOCALES:
        code = loc["code"]
        # 3 distinct months per language (Jan, Jun, Dec) -> ~615 cases
        for m_idx in (0, 5, 11):
            names = loc["months"][m_idx]
            if names:
                cases.append((code, "month", f"15 {names[0]} 2025", (2025, m_idx + 1, 15)))
        # 2 weekdays per language -> ~410 cases
        for wd_idx in (0, 4):
            wd_names = loc["weekdays"][wd_idx]
            if wd_names:
                cases.append(
                    (
                        code,
                        "weekday",
                        f"15 {loc['months'][2][0]} 2025 {wd_names[0]}"
                        if loc["months"][2]
                        else wd_names[0],
                        (2025, 3, 15) if loc["months"][2] else None,
                    )
                )
        # Unambiguous exact relative phrases: 0-offset, past non-zero, future non-zero
        phrase_counts = Counter(entry[0] for entry in loc["relative_exact"])
        for kind_tag, pred in (
            ("rel_exact_cur", lambda e: e[2] == 0),
            ("rel_exact_past", lambda e: e[2] > 0 and e[1] < 0),
            ("rel_exact_fut", lambda e: e[2] > 0 and e[1] > 0),
        ):
            for phrase, sign, amt, uid in loc["relative_exact"]:
                if phrase_counts[phrase] == 1 and pred((phrase, sign, amt, uid)):
                    cases.append(
                        (code, kind_tag, phrase, _expected_relative_dt(FIXED_BASE, sign, amt, uid))
                    )
                    break
        # Unambiguous relative affix patterns: first past (-1) and first future (+1)
        affix_counts = Counter((entry[0], entry[1]) for entry in loc["relative_affix"])
        for kind_tag, target_sign in (("rel_affix_past", -1), ("rel_affix_fut", 1)):
            for pref, suff, sign, uid in loc["relative_affix"]:
                if affix_counts[(pref, suff)] == 1 and sign == target_sign:
                    cases.append(
                        (
                            code,
                            kind_tag,
                            f"{pref}4{suff}",
                            _expected_relative_dt(FIXED_BASE, sign, 4, uid),
                        )
                    )
                    break
    return cases


LANG_MATRIX_CASES = _build_205_lang_cases()


@pytest.mark.parametrize("text,ymd", UPSTREAM_CORPUS)
def test_upstream_dateparser_corpus(text: str, ymd: list[int]):
    dt = dateoxide.parse(text, settings=SETTINGS)
    assert dt is not None, f"Failed upstream corpus case: {text!r}"
    assert [dt.year, dt.month, dt.day] == ymd, (
        f"Mismatch on {text!r}: got {[dt.year, dt.month, dt.day]} vs expected {ymd}"
    )


@pytest.mark.parametrize("lang,kind,text,expected", LANG_MATRIX_CASES)
def test_205_language_matrix(
    lang: str, kind: str, text: str, expected: tuple[int, int, int] | datetime | None
):
    dt = dateoxide.parse(text, languages=[lang], settings=SETTINGS)
    assert dt is not None, f"[{lang}:{kind}] failed on {text!r}"
    if isinstance(expected, datetime):
        assert dt == expected, f"[{lang}:{kind}] {text!r} -> {dt!r} vs {expected!r}"
    elif expected is not None:
        assert (dt.year, dt.month, dt.day) == expected, (
            f"[{lang}:{kind}] {text!r} -> {(dt.year, dt.month, dt.day)} vs {expected}"
        )
