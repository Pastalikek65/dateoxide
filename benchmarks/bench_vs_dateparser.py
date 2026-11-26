"""Reproducible benchmark comparing dateoxide (Rust/PyO3) vs scrapinghub/dateparser (Python).

Measures:
  1. Cold import latency (ms) in isolated subprocesses
  2. Peak / delta RSS memory (MB) in isolated subprocesses
  3. Throughput (ops/sec) across:
       - ISO / numeric / RFC dates
       - Multilingual natural-language dates (auto-detect)
       - Multilingual natural-language dates (explicit language hint)
       - Mixed workload including invalid strings (worst case for backtracking)
       - Parallel batch throughput (`dateoxide.parse_batch` vs loop)
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from datetime import datetime

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)

WORKLOADS: dict[str, list[str]] = {
    "iso_numeric": [
        "2026-11-26T15:30:00Z",
        "2026-11-26 14:45:12+03:00",
        "26.11.2026",
        "11/26/2026 09:15",
        "Thu, 26 Nov 2026 15:30:00 +0300",
        "20261126",
    ],
    "multilingual_auto": [
        "12 December 2025",
        "3 days ago",
        "3 gün önce",
        "12 Aralık 2025",
        "vor 2 Wochen",
        "12 de diciembre de 2025",
        "hier à 14h30",
        "26 ноября 2026",
        "2026年11月26日",
        "3日前",
    ],
    "invalid_strings": [
        "not a date at all xyz",
        "lorem ipsum dolor sit amet",
        "hello world foo bar",
        "user_id_98765_profile",
        "....................................",
    ],
}


def measure_import_and_memory(module_name: str) -> dict[str, float]:
    code = f"""
import json, time, psutil
from datetime import datetime
p = psutil.Process()
rss0 = p.memory_info().rss / 1048576
t0 = time.perf_counter()
import {module_name} as mod
t_imp = (time.perf_counter() - t0) * 1000
rss_imp = p.memory_info().rss / 1048576
samples = [
    "2026-11-26T15:30:00Z", "12 December 2025", "3 days ago", "3 gün önce",
    "vor 2 Wochen", "12 de diciembre de 2025", "hier à 14h30", "26 ноября 2026",
    "2026年11月26日", "not a date at all xyz",
]
base = datetime(2026, 11, 26, 15, 0, 0)
for s in samples * 20:
    mod.parse(s, settings={{"RELATIVE_BASE": base}})
rss_after = p.memory_info().rss / 1048576
print(json.dumps({{
    "import_ms": round(t_imp, 2),
    "rss_before_mb": round(rss0, 2),
    "rss_after_import_mb": round(rss_imp, 2),
    "rss_after_parse_mb": round(rss_after, 2),
    "rss_delta_mb": round(rss_after - rss0, 2),
}}))
"""
    out = subprocess.check_output([sys.executable, "-c", code], text=True)
    return json.loads(out.strip())


def bench_throughput() -> dict[str, dict[str, float]]:
    import dateoxide
    import dateparser

    settings = {"RELATIVE_BASE": FIXED_BASE}
    results: dict[str, dict[str, float]] = {}

    # Warm up both libraries
    dateoxide.parse("3 gün önce", settings=settings)
    dateparser.parse("3 gün önce", settings=settings)

    for name, items in WORKLOADS.items():
        # Reference dateparser
        dp_reps = 30 if name != "invalid_strings" else 15
        dp_inputs = items * dp_reps
        t0 = time.perf_counter()
        for s in dp_inputs:
            dateparser.parse(s, settings=settings)
        dp_s = time.perf_counter() - t0
        dp_ops = len(dp_inputs) / dp_s

        # dateoxide single-threaded parse()
        ox_reps = 2000
        ox_inputs = items * ox_reps
        t0 = time.perf_counter()
        for s in ox_inputs:
            dateoxide.parse(s, settings=settings)
        ox_s = time.perf_counter() - t0
        ox_ops = len(ox_inputs) / ox_s

        # dateoxide reusable DateDataParser.parse() (no per-call dict parsing)
        ddp = dateoxide.DateDataParser(settings=settings)
        t0 = time.perf_counter()
        for s in ox_inputs:
            ddp.parse(s)
        ddp_s = time.perf_counter() - t0
        ddp_ops = len(ox_inputs) / ddp_s

        # dateoxide parallel parse_batch() (Rayon, GIL-free)
        batch_inputs = items * 10000
        t0 = time.perf_counter()
        dateoxide.parse_batch(batch_inputs, settings=settings)
        batch_s = time.perf_counter() - t0
        batch_ops = len(batch_inputs) / batch_s

        results[name] = {
            "dateparser_ops_s": round(dp_ops, 1),
            "dateoxide_parse_ops_s": round(ox_ops, 1),
            "dateoxide_parser_ops_s": round(ddp_ops, 1),
            "dateoxide_batch_ops_s": round(batch_ops, 1),
            "speedup_parse": round(ox_ops / dp_ops, 1),
            "speedup_parser": round(ddp_ops / dp_ops, 1),
            "speedup_batch": round(batch_ops / dp_ops, 1),
        }

    return results


def main() -> None:
    print("=== 1. Import Latency & Memory Footprint (Isolated Subprocesses) ===")
    mem_dp = measure_import_and_memory("dateparser")
    mem_ox = measure_import_and_memory("dateoxide")
    print(f"dateparser: {mem_dp}")
    print(f"dateoxide:  {mem_ox}")
    print(
        f"-> Import speedup: {mem_dp['import_ms'] / max(0.01, mem_ox['import_ms']):.1f}x | "
        f"RSS delta reduction: {mem_dp['rss_delta_mb'] / max(0.1, mem_ox['rss_delta_mb']):.1f}x"
    )

    print("\n=== 2. Throughput Benchmarks (ops/sec) ===")
    tp = bench_throughput()
    for name, stats in tp.items():
        print(
            f"[{name:17s}] dateparser={stats['dateparser_ops_s']:8.1f}/s | "
            f"dateoxide.parse={stats['dateoxide_parse_ops_s']:10.1f}/s ({stats['speedup_parse']:6.1f}x) | "
            f"DateDataParser={stats['dateoxide_parser_ops_s']:10.1f}/s ({stats['speedup_parser']:6.1f}x) | "
            f"parse_batch={stats['dateoxide_batch_ops_s']:11.1f}/s ({stats['speedup_batch']:7.1f}x)"
        )


if __name__ == "__main__":
    main()
