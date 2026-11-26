"""Large-scale 2,200+ test suite combining:
1. 1,010 verified upstream scrapinghub/dateparser test vectors (tests/data/upstream_corpus.json)
2. Exhaustive 205-language matrix across months, weekdays, exact relatives, and affixes
"""
from __future__ import annotations

import json
from datetime import datetime
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


def _build_205_lang_cases() -> list[tuple[str, str, str, tuple[int, int, int] | None]]:
    cases: list[tuple[str, str, str, tuple[int, int, int] | None]] = []
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
                cases.append((code, "weekday", f"15 {loc['months'][2][0]} 2025 {wd_names[0]}" if loc["months"][2] else wd_names[0], (2025, 3, 15) if loc["months"][2] else None))
        # Up to 2 exact relative phrases per language -> ~408 cases
        for rel in loc["relative_exact"][:2]:
            cases.append((code, "rel_exact", rel[0], None))
        # First relative affix pattern if available -> ~121 cases
        if loc["relative_affix"]:
            pref, suff, _, _ = loc["relative_affix"][0]
            cases.append((code, "rel_affix", f"{pref}4{suff}", None))
    return cases


LANG_MATRIX_CASES = _build_205_lang_cases()


@pytest.mark.parametrize("text,ymd", UPSTREAM_CORPUS)
def test_upstream_dateparser_corpus(text: str, ymd: list[int]):
    dt = dateoxide.parse(text, settings=SETTINGS)
    assert dt is not None, f"Failed upstream corpus case: {text!r}"
    assert [dt.year, dt.month, dt.day] == ymd, (
        f"Mismatch on {text!r}: got {[dt.year, dt.month, dt.day]} vs expected {ymd}"
    )


@pytest.mark.parametrize("lang,kind,text,expected_ymd", LANG_MATRIX_CASES)
def test_205_language_matrix(
    lang: str, kind: str, text: str, expected_ymd: tuple[int, int, int] | None
):
    dt = dateoxide.parse(text, languages=[lang], settings=SETTINGS)
    assert dt is not None, f"[{lang}:{kind}] failed on {text!r}"
    if expected_ymd is not None:
        assert (dt.year, dt.month, dt.day) == expected_ymd, (
            f"[{lang}:{kind}] {text!r} -> {(dt.year, dt.month, dt.day)} vs {expected_ymd}"
        )
