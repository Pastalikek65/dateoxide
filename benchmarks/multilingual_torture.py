"""Independent extreme multilingual comparison: dateoxide vs dateparser across 87 hard cases."""
from __future__ import annotations

from datetime import datetime
import time

import dateoxide
import dateparser
from tests.test_multilingual_extreme import EXTREME_CASES, FIXED_BASE


def main() -> None:
    cfg = {"RELATIVE_BASE": FIXED_BASE}
    # Warmup
    dateoxide.parse("3 gün önce", settings=cfg)
    dateparser.parse("3 gün önce", settings=cfg)

    t0 = time.perf_counter()
    ox_auto = sum(1 for _, txt, exp in EXTREME_CASES if dateoxide.parse(txt, settings=cfg) == exp)
    t_ox_auto = (time.perf_counter() - t0) * 1000.0

    t0 = time.perf_counter()
    ox_exp = sum(1 for lang, txt, exp in EXTREME_CASES if dateoxide.parse(txt, languages=[lang], settings=cfg) == exp)
    t_ox_exp = (time.perf_counter() - t0) * 1000.0

    t0 = time.perf_counter()
    dp_exp = sum(1 for lang, txt, exp in EXTREME_CASES if dateparser.parse(txt, languages=[lang], settings=cfg) == exp)
    t_dp_exp = (time.perf_counter() - t0) * 1000.0

    n = len(EXTREME_CASES)
    print("=" * 72)
    print(f"EXTREME MULTILINGUAL BENCHMARK ({n} cases across 30+ languages & 10+ scripts)")
    print("=" * 72)
    print(f"dateoxide (auto-detect)    : {ox_auto}/{n} ({t_ox_auto:.2f} ms)")
    print(f"dateoxide (explicit lang)  : {ox_exp}/{n} ({t_ox_exp:.2f} ms)")
    print(f"dateparser (explicit lang) : {dp_exp}/{n} ({t_dp_exp:.2f} ms)")


if __name__ == "__main__":
    main()
