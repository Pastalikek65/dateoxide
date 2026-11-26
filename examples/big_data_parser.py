"""Real-world example: Large-scale multilingual ETL date parser (50,000 records).

Demonstrates processing a realistic 50,000-row mixed dataset (multi-language timestamps,
ISO/RFC strings, natural relative phrases, and 15% dirty/invalid fields) using
`dateoxide.parse_batch` without GIL contention, reporting throughput and memory delta.

Run demo:
  python examples/big_data_parser.py
"""
from __future__ import annotations

from datetime import datetime
import time

import dateoxide

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


def build_dataset(n_rows: int = 50_000) -> list[str]:
    templates = [
        "2026-11-26T15:30:00Z",
        "12 December 2025",
        "15 EKİM 2025",
        "3 gün önce",
        "vor 2 Wochen",
        "hier à 14h30",
        "12 de diciembre de 2025",
        "2026年11月26日",
        "2026-W48-4",
        "١٢ ديسمبر ٢٠٢٥",
        "Thu, 26 Nov 2026 15:30:00 +0300",
        "not_a_date_placeholder",
        "N/A",
        "Python 3.12.1",
    ]
    return [templates[i % len(templates)] for i in range(n_rows)]


def main() -> None:
    try:
        import psutil
        proc = psutil.Process()
        rss_before = proc.memory_info().rss / 1048576
    except Exception:
        proc = None
        rss_before = 0.0

    rows = build_dataset(50_000)
    settings = {"RELATIVE_BASE": FIXED_BASE, "TO_TIMEZONE": "UTC"}

    # Warmup
    dateoxide.parse_batch(rows[:100], settings=settings)

    t0 = time.perf_counter()
    parsed = dateoxide.parse_batch(rows, settings=settings)
    elapsed = time.perf_counter() - t0

    valid_count = sum(1 for dt in parsed if dt is not None)
    invalid_count = len(parsed) - valid_count
    rss_after = proc.memory_info().rss / 1048576 if proc else 0.0

    print(f"Total records : {len(rows):,}")
    print(f"Valid parsed  : {valid_count:,}")
    print(f"Clean None    : {invalid_count:,}")
    print(f"Elapsed       : {elapsed*1000:.2f} ms ({len(rows)/elapsed:,.0f} records/sec)")
    if proc:
        print(f"RSS delta     : {rss_after - rss_before:+.2f} MB")


if __name__ == "__main__":
    main()
