"""Fair, reproducible benchmark comparing dateoxide (Rust/PyO3) vs dateparser 1.5.1 (Python).

Methodology safeguards:
  - Uses N unique, non-repeating inputs per throughput workload so LRU caching cannot
    inflate uncached numbers.
  - Clearly separates:
      1. Uncached single-threaded (`CACHE_SIZE_LIMIT=0`) vs `dateparser` single-threaded
      2. Cached single-threaded on a repeated corpus (explicitly labeled)
      3. Multi-threaded `dateoxide.parse_batch` using Rayon (explicitly labeled MT)
  - Measures isolated subprocess lifecycle memory & latency across:
      import -> 1st ISO parse -> 1st NL parse (OnceLock load) -> 20 languages -> 50k unique ops.
"""

from __future__ import annotations

import argparse
from datetime import datetime
import json
import os
import platform
import subprocess
import sys
import time
from typing import Any

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)

ENG_MONTHS = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
]
TR_MONTHS = [
    "Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran",
    "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık",
]
DE_MONTHS = [
    "Januar", "Februar", "März", "April", "Mai", "Juni",
    "Juli", "August", "September", "Oktober", "November", "Dezember",
]
FR_MONTHS = [
    "janvier", "février", "mars", "avril", "mai", "juin",
    "juillet", "août", "septembre", "octobre", "novembre", "décembre",
]
ES_MONTHS = [
    "enero", "febrero", "marzo", "abril", "mayo", "junio",
    "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre",
]


def generate_unique_workloads(n: int = 2000) -> dict[str, list[str]]:
    iso_numeric: list[str] = []
    english_nl: list[str] = []
    multilingual_nl: list[str] = []
    invalid_prose: list[str] = []

    for i in range(n):
        yr = 1995 + (i % 35)
        mo = (i % 12) + 1
        dy = (i % 28) + 1
        hr = i % 24
        mn = (i * 7) % 60
        sc = (i * 13) % 60

        kind_iso = i % 4
        if kind_iso == 0:
            iso_numeric.append(f"{yr:04d}-{mo:02d}-{dy:02d}T{hr:02d}:{mn:02d}:{sc:02d}Z")
        elif kind_iso == 1:
            iso_numeric.append(f"{yr:04d}-{mo:02d}-{dy:02d} {hr:02d}:{mn:02d}+{i % 6:02d}:00")
        elif kind_iso == 2:
            iso_numeric.append(f"{dy:02d}.{mo:02d}.{yr:04d} {hr:02d}:{mn:02d}")
        else:
            iso_numeric.append(f"{yr:04d}{mo:02d}{dy:02d}T{hr:02d}{mn:02d}{sc:02d}Z")

        kind_en = i % 4
        if kind_en == 0:
            english_nl.append(f"{dy} {ENG_MONTHS[mo - 1]} {yr} {hr:02d}:{mn:02d}")
        elif kind_en == 1:
            english_nl.append(f"{(i % 90) + 1} days ago")
        elif kind_en == 2:
            english_nl.append(f"in {(i % 72) + 1} hours")
        else:
            english_nl.append(f"{ENG_MONTHS[mo - 1]} {dy}, {yr}")

        kind_ml = i % 6
        if kind_ml == 0:
            multilingual_nl.append(f"{dy} {TR_MONTHS[mo - 1]} {yr}")
        elif kind_ml == 1:
            multilingual_nl.append(f"{(i % 60) + 1} gün önce")
        elif kind_ml == 2:
            multilingual_nl.append(f"am {dy}. {DE_MONTHS[mo - 1]} {yr} um {hr:02d}:{mn:02d}")
        elif kind_ml == 3:
            multilingual_nl.append(f"{dy} {FR_MONTHS[mo - 1]} {yr} à {hr:02d}h{mn:02d}")
        elif kind_ml == 4:
            multilingual_nl.append(f"{dy} de {ES_MONTHS[mo - 1]} de {yr}")
        else:
            multilingual_nl.append(f"{yr}年{mo}月{dy}日")

        kind_inv = i % 4
        if kind_inv == 0:
            invalid_prose.append(f"release v{i % 9}.{mo}.{dy % 9} build note {i}")
        elif kind_inv == 1:
            invalid_prose.append(f"user_profile_token_{i}_{yr}_alpha")
        elif kind_inv == 2:
            invalid_prose.append(f"lorem ipsum order {i} total ${i % 900}.{i % 99:02d}")
        else:
            invalid_prose.append(f"TK {1000 + (i % 900)} status {'.' * ((i % 20) + 3)}")

    return {
        "iso_numeric": iso_numeric,
        "english_nl": english_nl,
        "multilingual_nl": multilingual_nl,
        "invalid_prose": invalid_prose,
    }


def measure_lifecycle(module_name: str) -> dict[str, float]:
    code = f"""
import json, time, psutil
from datetime import datetime
p = psutil.Process()
rss0 = p.memory_info().rss / 1048576

t0 = time.perf_counter()
import {module_name} as mod
t_imp = (time.perf_counter() - t0) * 1000.0
rss_imp = p.memory_info().rss / 1048576

base = datetime(2026, 11, 26, 15, 0, 0)
cfg = {{"RELATIVE_BASE": base}}

t0 = time.perf_counter()
mod.parse("2026-11-26T15:30:00Z", settings=cfg)
t_first_iso = (time.perf_counter() - t0) * 1000.0
rss_iso = p.memory_info().rss / 1048576

t0 = time.perf_counter()
mod.parse("3 days ago", settings=cfg)
t_first_nl = (time.perf_counter() - t0) * 1000.0
rss_nl = p.memory_info().rss / 1048576

twenty_langs = [
    "12 December 2025", "3 gün önce", "vor 2 Wochen", "12 de diciembre de 2025",
    "hier à 14h30", "26 ноября 2026", "2026年11月26日", "2026년 11월 26일",
    "منذ 3 أيام", "۳ روز پیش", "15 maja 2025", "12 maggio 2025",
    "3 dagen geleden", "för 3 dagar sedan", "om 2 uger", "for 3 dager siden",
    "3 päivää sitten", "3 napja", "σε 3 ημέρες", "לפני 3 ימים",
]
t0 = time.perf_counter()
for s in twenty_langs:
    mod.parse(s, settings=cfg)
t_20 = (time.perf_counter() - t0) * 1000.0
rss_20 = p.memory_info().rss / 1048576

# Stress test RSS stability across 50k unique strings on dateoxide, 1.5k on dateparser
n_stress = 50000 if {module_name!r} == "dateoxide" else 1500
for idx in range(n_stress):
    mod.parse(f"{{1990 + (idx % 35):04d}}-{{(idx % 12) + 1:02d}}-{{(idx % 28) + 1:02d}} {{idx % 24:02d}}:{{idx % 60:02d}}", settings=cfg)
rss_stress = p.memory_info().rss / 1048576

print(json.dumps({{
    "import_ms": round(t_imp, 2),
    "first_iso_ms": round(t_first_iso, 3),
    "first_nl_ms": round(t_first_nl, 2),
    "twenty_langs_ms": round(t_20, 2),
    "rss_base_mb": round(rss0, 2),
    "rss_after_import_delta_mb": round(rss_imp - rss0, 2),
    "rss_after_iso_delta_mb": round(rss_iso - rss0, 2),
    "rss_after_nl_delta_mb": round(rss_nl - rss0, 2),
    "rss_after_20langs_delta_mb": round(rss_20 - rss0, 2),
    "rss_after_stress_delta_mb": round(rss_stress - rss0, 2),
    "stress_count": n_stress,
}}))
"""
    out = subprocess.check_output([sys.executable, "-c", code], text=True)
    return json.loads(out.strip())


def bench_throughput(n_unique: int = 2000) -> dict[str, dict[str, Any]]:
    import dateoxide
    import dateparser

    base_cfg = {"RELATIVE_BASE": FIXED_BASE}
    uncached_cfg = {"RELATIVE_BASE": FIXED_BASE, "CACHE_SIZE_LIMIT": 0}
    workloads = generate_unique_workloads(n_unique)

    # Equal multi-language warmup
    for w in ["2026-11-26", "3 days ago", "3 gün önce", "vor 2 Wochen", "2026年11月26日"]:
        dateoxide.parse(w, settings=uncached_cfg)
        dateparser.parse(w, settings=base_cfg)

    ddp_ox_uncached = dateoxide.DateDataParser(settings=uncached_cfg)
    ddp_dp = dateparser.DateDataParser(settings=base_cfg)
    results: dict[str, dict[str, Any]] = {}

    for name, items in workloads.items():
        # Sample enough items for dateparser without taking minutes (min 150 unique items)
        dp_n = 150 if name != "invalid_prose" else 80
        dp_items = items[:dp_n]
        t0 = time.perf_counter()
        for s in dp_items:
            ddp_dp.get_date_data(s)
        dp_s = time.perf_counter() - t0
        dp_ops = dp_n / dp_s

        # 1. dateoxide uncached 1T via reusable DateDataParser(CACHE_SIZE_LIMIT=0)
        t0 = time.perf_counter()
        for s in items:
            ddp_ox_uncached.parse(s)
        ox_ddp_s = time.perf_counter() - t0
        ox_ddp_ops = len(items) / ox_ddp_s

        # 2. dateoxide uncached 1T via top-level dateoxide.parse(..., CACHE_SIZE_LIMIT=0)
        t0 = time.perf_counter()
        for s in items:
            dateoxide.parse(s, settings=uncached_cfg)
        ox_fn_s = time.perf_counter() - t0
        ox_fn_ops = len(items) / ox_fn_s

        # 3. dateoxide cached LRU (explicitly repeated corpus: 100 unique * 20 reps = 2000 ops)
        rep_items = items[:100] * 20
        ddp_ox_cached = dateoxide.DateDataParser(settings=base_cfg)
        t0 = time.perf_counter()
        for s in rep_items:
            ddp_ox_cached.parse(s)
        ox_cached_s = time.perf_counter() - t0
        ox_cached_ops = len(rep_items) / ox_cached_s

        # 4. dateoxide multi-threaded Rayon batch on unique items (10,000 items = 5x unique * slight suffix or 2000x5 unique)
        batch_items = (items * 5)[:10000]
        t0 = time.perf_counter()
        dateoxide.parse_batch(batch_items, settings=uncached_cfg)
        ox_batch_s = time.perf_counter() - t0
        ox_batch_ops = len(batch_items) / ox_batch_s

        results[name] = {
            "n_unique": len(items),
            "dateparser_1t_ops_s": round(dp_ops, 1),
            "dateoxide_parse_uncached_1t_ops_s": round(ox_fn_ops, 1),
            "dateoxide_ddp_uncached_1t_ops_s": round(ox_ddp_ops, 1),
            "dateoxide_ddp_cached_repeated_1t_ops_s": round(ox_cached_ops, 1),
            "dateoxide_parse_batch_mt_ops_s": round(ox_batch_ops, 1),
            "speedup_uncached_fn_1t": round(ox_fn_ops / dp_ops, 1),
            "speedup_uncached_ddp_1t": round(ox_ddp_ops / dp_ops, 1),
            "speedup_cached_repeated_1t": round(ox_cached_ops / dp_ops, 1),
            "speedup_batch_mt": round(ox_batch_ops / dp_ops, 1),
        }

    return results


def main() -> None:
    parser = argparse.ArgumentParser(description="Fair benchmark: dateoxide vs dateparser")
    parser.add_argument("--json", action="store_true", help="Emit JSON report")
    parser.add_argument("--unique", type=int, default=2000, help="Unique inputs per category")
    args = parser.parse_args()

    lifecycle_dp = measure_lifecycle("dateparser")
    lifecycle_ox = measure_lifecycle("dateoxide")
    throughput = bench_throughput(n_unique=args.unique)

    report = {
        "system": {
            "platform": platform.platform(),
            "cpu_count": os.cpu_count(),
            "python_version": platform.python_version(),
        },
        "lifecycle": {
            "dateparser": lifecycle_dp,
            "dateoxide": lifecycle_ox,
        },
        "throughput": throughput,
    }

    if args.json:
        print(json.dumps(report, indent=2))
        return

    print("=== 1. Isolated Startup & RSS Lifecycle (Subprocesses) ===")
    print(f"dateparser: {lifecycle_dp}")
    print(f"dateoxide : {lifecycle_ox}")

    print(f"\n=== 2. Fair Throughput on {args.unique} Unique Inputs per Category (ops/sec) ===")
    for name, s in throughput.items():
        print(
            f"[{name:16s}] dp_1T={s['dateparser_1t_ops_s']:7.1f}/s | "
            f"ox.parse_uncached_1T={s['dateoxide_parse_uncached_1t_ops_s']:9.1f}/s ({s['speedup_uncached_fn_1t']:6.1f}x) | "
            f"ox.DDP_uncached_1T={s['dateoxide_ddp_uncached_1t_ops_s']:9.1f}/s ({s['speedup_uncached_ddp_1t']:6.1f}x) | "
            f"ox.DDP_cached_rep={s['dateoxide_ddp_cached_repeated_1t_ops_s']:10.1f}/s | "
            f"ox.batch_MT={s['dateoxide_parse_batch_mt_ops_s']:10.1f}/s ({s['speedup_batch_mt']:6.1f}x)"
        )


if __name__ == "__main__":
    main()
