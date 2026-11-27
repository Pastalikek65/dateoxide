"""Independent, accuracy-verified torture benchmark: dateoxide (Rust/PyO3) vs dateparser (Python).

Tests 7 brutal categories where both libraries face realistic production pain:
  1. multilingual_hard        - Turkish I/ı casing, CJK, Korean, Arabic-Indic, Persian, Russian/Polish genitives, Cyrillic, Thai, Hebrew
  2. relative_and_clamping_hard - Compound phrases, word numbers, month-end/leap clamping, parts of day
  3. calendar_leap_iso_traps  - Gregorian leap exceptions (1900/2000/2100/2024), ISO week years, compact ISO, year-0 trap
  4. logs_rfc_tz_dst          - RFC 2822/3339, Nginx CLF, Syslog, summer/winter DST transitions, time-only PREFER_DATES_FROM
  5. false_positives_must_be_none - Versions, IPs, IPv6, UUIDs, prices, percentages, city names, English prose, overflow
  6. search_dates_dirty_texts - Extracting exact date subsets from noisy multi-language prose/logs
  7. redos_stress_bombs       - Pathological 3k-5k char inputs measuring worst-case algorithmic complexity

Usage:
  python benchmarks/torture_benchmark.py
  python benchmarks/torture_benchmark.py --json
"""
from __future__ import annotations

import argparse
from dataclasses import dataclass
from datetime import datetime, timezone
import json
import os
import platform
import statistics
import subprocess
import sys
import time
from typing import Any

FIXED_BASE = datetime(2026, 11, 26, 15, 0, 0)


@dataclass(frozen=True)
class TortureCase:
    category: str
    text: str
    settings: dict[str, Any]
    expected: datetime | None
    compare_utc: bool = False
    date_formats: list[str] | None = None


def build_cases() -> list[TortureCase]:
    base_cfg = {"RELATIVE_BASE": FIXED_BASE}
    return [
        # 1. multilingual_hard (20 cases)
        TortureCase("multilingual_hard", "15 EKİM 2025 SAAT 14:30", base_cfg, datetime(2025, 10, 15, 14, 30)),
        TortureCase("multilingual_hard", "3 GÜN ÖNCE", base_cfg, datetime(2026, 11, 23, 15, 0)),
        TortureCase("multilingual_hard", "1 yıl 2 ay 3 gün önce", base_cfg, datetime(2025, 9, 23, 15, 0)),
        TortureCase("multilingual_hard", "geçen pazartesi", base_cfg, datetime(2026, 11, 23, 0, 0)),
        TortureCase("multilingual_hard", "yarın sabah", base_cfg, datetime(2026, 11, 27, 9, 0)),
        TortureCase("multilingual_hard", "29 февраля 2024 года", base_cfg, datetime(2024, 2, 29)),
        TortureCase("multilingual_hard", "15 maja 2025", base_cfg, datetime(2025, 5, 15)),
        TortureCase("multilingual_hard", "2 dni temu", base_cfg, datetime(2026, 11, 24, 15, 0)),
        TortureCase("multilingual_hard", "před 3 dny", base_cfg, datetime(2026, 11, 23, 15, 0)),
        TortureCase("multilingual_hard", "vor 1 Jahr und 2 Monaten", base_cfg, datetime(2025, 9, 26, 15, 0)),
        TortureCase("multilingual_hard", "am 31. Dezember 2025 um 23:59 Uhr", base_cfg, datetime(2025, 12, 31, 23, 59)),
        TortureCase("multilingual_hard", "1er décembre 2025 à 14h30", base_cfg, datetime(2025, 12, 1, 14, 30)),
        TortureCase("multilingual_hard", "il y a 1 an et 2 mois", base_cfg, datetime(2025, 9, 26, 15, 0)),
        TortureCase("multilingual_hard", "12 de diciembre de 2025", base_cfg, datetime(2025, 12, 12)),
        TortureCase("multilingual_hard", "hace 2 semanas", base_cfg, datetime(2026, 11, 12, 15, 0)),
        TortureCase("multilingual_hard", "١٢ ديسمبر ٢٠٢٥", base_cfg, datetime(2025, 12, 12)),
        TortureCase("multilingual_hard", "منذ ٣ أيام", base_cfg, datetime(2026, 11, 23, 15, 0)),
        TortureCase("multilingual_hard", "۳ روز پیش", base_cfg, datetime(2026, 11, 23, 15, 0)),
        TortureCase("multilingual_hard", "2026年11月26日 15時30分", base_cfg, datetime(2026, 11, 26, 15, 30)),
        TortureCase("multilingual_hard", "2026년 11월 26일 오후 3시 30분", base_cfg, datetime(2026, 11, 26, 15, 30)),

        # 2. relative_and_clamping_hard (16 cases)
        TortureCase("relative_and_clamping_hard", "half an hour ago", base_cfg, datetime(2026, 11, 26, 14, 30)),
        TortureCase("relative_and_clamping_hard", "a fortnight ago", base_cfg, datetime(2026, 11, 12, 15, 0)),
        TortureCase("relative_and_clamping_hard", "two days ago", base_cfg, datetime(2026, 11, 24, 15, 0)),
        TortureCase("relative_and_clamping_hard", "one year, 2 months and 3 days ago", base_cfg, datetime(2025, 9, 23, 15, 0)),
        TortureCase("relative_and_clamping_hard", "1.5 hours ago", base_cfg, datetime(2026, 11, 26, 13, 30)),
        TortureCase("relative_and_clamping_hard", "in 2 weeks and 3 days", base_cfg, datetime(2026, 12, 13, 15, 0)),
        TortureCase("relative_and_clamping_hard", "1 month ago", {"RELATIVE_BASE": datetime(2026, 3, 31, 10, 0)}, datetime(2026, 2, 28, 10, 0)),
        TortureCase("relative_and_clamping_hard", "1 year ago", {"RELATIVE_BASE": datetime(2024, 2, 29, 10, 0)}, datetime(2023, 2, 28, 10, 0)),
        TortureCase("relative_and_clamping_hard", "tomorrow at 5pm", base_cfg, datetime(2026, 11, 27, 17, 0)),
        TortureCase("relative_and_clamping_hard", "yesterday morning", base_cfg, datetime(2026, 11, 25, 9, 0)),
        TortureCase("relative_and_clamping_hard", "2 days ago at 14:00", base_cfg, datetime(2026, 11, 24, 14, 0)),
        TortureCase("relative_and_clamping_hard", "day before yesterday", base_cfg, datetime(2026, 11, 24, 15, 0)),
        TortureCase("relative_and_clamping_hard", "the day after tomorrow", base_cfg, datetime(2026, 11, 28, 15, 0)),
        TortureCase("relative_and_clamping_hard", "quarter past 3", base_cfg, datetime(2026, 11, 26, 3, 15)),
        TortureCase("relative_and_clamping_hard", "two and a half hours ago", base_cfg, datetime(2026, 11, 26, 12, 30)),
        TortureCase("relative_and_clamping_hard", "last Friday", base_cfg, datetime(2026, 11, 20, 0, 0)),

        # 3. calendar_leap_iso_traps (15 cases)
        TortureCase("calendar_leap_iso_traps", "2024-02-29", base_cfg, datetime(2024, 2, 29)),
        TortureCase("calendar_leap_iso_traps", "2025-02-29", base_cfg, None),
        TortureCase("calendar_leap_iso_traps", "2000-02-29", base_cfg, datetime(2000, 2, 29)),
        TortureCase("calendar_leap_iso_traps", "1900-02-29", base_cfg, None),
        TortureCase("calendar_leap_iso_traps", "2100-02-29", base_cfg, None),
        TortureCase("calendar_leap_iso_traps", "2020-W53-4", base_cfg, datetime(2020, 12, 31)),
        TortureCase("calendar_leap_iso_traps", "2026-W48-4", base_cfg, datetime(2026, 11, 26)),
        TortureCase("calendar_leap_iso_traps", "2021-W53-1", base_cfg, None),
        TortureCase("calendar_leap_iso_traps", "20261126T153000Z", base_cfg, datetime(2026, 11, 26, 15, 30, tzinfo=timezone.utc), compare_utc=True),
        TortureCase("calendar_leap_iso_traps", "0000-01-01", base_cfg, None),
        TortureCase("calendar_leap_iso_traps", "9999-12-31", base_cfg, datetime(9999, 12, 31)),
        TortureCase("calendar_leap_iso_traps", "2026-13-01", base_cfg, None),
        TortureCase("calendar_leap_iso_traps", "2026-04-31", base_cfg, None),
        TortureCase("calendar_leap_iso_traps", "2026-11-26 25:00:00", base_cfg, None),
        TortureCase("calendar_leap_iso_traps", "2024-366", {}, datetime(2024, 12, 31), date_formats=["%Y-%j"]),

        # 4. logs_rfc_tz_dst (13 cases)
        TortureCase("logs_rfc_tz_dst", "Thu, 26 Nov 2026 15:30:00 +0300", base_cfg, datetime(2026, 11, 26, 12, 30, tzinfo=timezone.utc), compare_utc=True),
        TortureCase("logs_rfc_tz_dst", "2026-11-26T15:30:00.123456+03:00", base_cfg, datetime(2026, 11, 26, 12, 30, 0, 123456, tzinfo=timezone.utc), compare_utc=True),
        TortureCase("logs_rfc_tz_dst", "26/Nov/2026:15:30:00 +0000", base_cfg, datetime(2026, 11, 26, 15, 30, tzinfo=timezone.utc), compare_utc=True),
        TortureCase("logs_rfc_tz_dst", "Nov 26 15:30:00", base_cfg, datetime(2026, 11, 26, 15, 30)),
        TortureCase("logs_rfc_tz_dst", "2026-07-15 12:00:00", {"TIMEZONE": "America/New_York", "TO_TIMEZONE": "Europe/Istanbul"}, datetime(2026, 7, 15, 19, 0)),
        TortureCase("logs_rfc_tz_dst", "2026-01-15 12:00:00", {"TIMEZONE": "America/New_York", "TO_TIMEZONE": "Europe/Istanbul"}, datetime(2026, 1, 15, 20, 0)),
        TortureCase("logs_rfc_tz_dst", "2026-11-26 12:00:00 EST", {"TO_TIMEZONE": "UTC"}, datetime(2026, 11, 26, 17, 0, tzinfo=timezone.utc), compare_utc=True),
        TortureCase("logs_rfc_tz_dst", "2026-11-26 12:00:00 +05:30", {"TO_TIMEZONE": "UTC"}, datetime(2026, 11, 26, 6, 30, tzinfo=timezone.utc), compare_utc=True),
        TortureCase("logs_rfc_tz_dst", "12:00 AM", base_cfg, datetime(2026, 11, 26, 0, 0)),
        TortureCase("logs_rfc_tz_dst", "12:00 PM", base_cfg, datetime(2026, 11, 26, 12, 0)),
        TortureCase("logs_rfc_tz_dst", "5:30 p.m.", base_cfg, datetime(2026, 11, 26, 17, 30)),
        TortureCase("logs_rfc_tz_dst", "09:00", {"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "future"}, datetime(2026, 11, 27, 9, 0)),
        TortureCase("logs_rfc_tz_dst", "18:00", {"RELATIVE_BASE": FIXED_BASE, "PREFER_DATES_FROM": "past"}, datetime(2026, 11, 25, 18, 0)),

        # 5. false_positives_must_be_none (15 cases)
        TortureCase("false_positives_must_be_none", "Python 3.12.1", base_cfg, None),
        TortureCase("false_positives_must_be_none", "v0.1.0", base_cfg, None),
        TortureCase("false_positives_must_be_none", "192.168.1.1", base_cfg, None),
        TortureCase("false_positives_must_be_none", "2001:0db8:85a3::8a2e", base_cfg, None),
        TortureCase("false_positives_must_be_none", "$12.99", base_cfg, None),
        TortureCase("false_positives_must_be_none", "€1.250,50", base_cfg, None),
        TortureCase("false_positives_must_be_none", "100%", base_cfg, None),
        TortureCase("false_positives_must_be_none", "İSTANBUL", base_cfg, None),
        TortureCase("false_positives_must_be_none", "550e8400-e29b-41d4-a716-446655440000", base_cfg, None),
        TortureCase("false_positives_must_be_none", "+90 532 123 45 67", base_cfg, None),
        TortureCase("false_positives_must_be_none", "Let us meet soon", base_cfg, None),
        TortureCase("false_positives_must_be_none", "She can see her man, did you say?", base_cfg, None),
        TortureCase("false_positives_must_be_none", "+999999999999999 days ago", base_cfg, None),
        TortureCase("false_positives_must_be_none", "5000 years ago", base_cfg, None),
        TortureCase("false_positives_must_be_none", "12\x00December 2025", base_cfg, None),
    ]


SEARCH_TORTURE_CASES: list[tuple[str, list[tuple[int, int, int, int, int]]]] = [
    (
        "[2026-11-26 15:30:00] ERROR db timeout, retry at 2026-11-26 15:35:00",
        [(2026, 11, 26, 15, 30), (2026, 11, 26, 15, 35)],
    ),
    (
        "Toplantı 15 Ocak 2026 tarihinde, önceki görüşme 3 gün önce bitti.",
        [(2026, 1, 15, 0, 0), (2026, 11, 23, 15, 0)],
    ),
    (
        "Let us meet tomorrow at 5pm or on 12 December 2025.",
        [(2026, 11, 27, 17, 0), (2025, 12, 12, 0, 0)],
    ),
    (
        "Python 3.12.1 on 192.168.1.1 costs $12.99 (100% ok)",
        [],
    ),
    (
        "Berlin: 12. Dezember 2025, Paris: hier à 14h30",
        [(2025, 12, 12, 0, 0), (2026, 11, 25, 14, 30)],
    ),
    (
        "Header Thu, 26 Nov 2026 15:30:00 +0300 received",
        [(2026, 11, 26, 15, 30)],
    ),
]


REDOS_BOMBS: list[tuple[str, str]] = [
    ("dots_5000", "." * 5000),
    ("dashes_5000", "-" * 5000),
    ("arabic_commas_5000", "،" * 5000),
    ("nested_parens_4000", "(" * 2000 + "hello" + ")" * 2000),
    ("non_date_words_500x", "not a date " * 500),
    ("digits_dash_1500x", "99-" * 1500),
]


def _match_expected(got: datetime | None, expected: datetime | None, compare_utc: bool) -> bool:
    if expected is None:
        return got is None
    if got is None:
        return False
    if compare_utc:
        if got.tzinfo is None or expected.tzinfo is None:
            return False
        return got.astimezone(timezone.utc) == expected.astimezone(timezone.utc)
    if (got.tzinfo is None) != (expected.tzinfo is None):
        return False
    return got == expected


def _eval_parser_cases(lib_name: str, mod: Any, cases: list[TortureCase]) -> dict[str, Any]:
    # Multi-language warmup across ISO, English, Turkish, German, French, Japanese, and Arabic
    for _ in range(3):
        for w in [
            "2026-11-26T15:30:00Z",
            "12 December 2025",
            "3 gün önce",
            "vor 2 Wochen",
            "hier à 14h30",
            "2026年11月26日",
            "١٢ ديسمبر ٢٠٢٥",
        ]:
            mod.parse(w, settings={"RELATIVE_BASE": FIXED_BASE})

    by_cat: dict[str, dict[str, Any]] = {}
    latencies_us: list[float] = []
    misses: list[dict[str, str]] = []
    exceptions: list[dict[str, str]] = []

    for c in cases:
        cat = by_cat.setdefault(c.category, {"passed": 0, "total": 0, "exceptions": 0, "total_ms": 0.0})
        cat["total"] += 1
        t0 = time.perf_counter()
        try:
            got = (
                mod.parse(c.text, date_formats=c.date_formats, settings=c.settings)
                if c.date_formats is not None
                else mod.parse(c.text, settings=c.settings)
            )
            dt_ms = (time.perf_counter() - t0) * 1000.0
            latencies_us.append(dt_ms * 1000.0)
            cat["total_ms"] += dt_ms
            if _match_expected(got, c.expected, c.compare_utc):
                cat["passed"] += 1
            else:
                misses.append({
                    "category": c.category,
                    "text": repr(c.text),
                    "expected": repr(c.expected),
                    "got": repr(got),
                })
        except Exception as exc:
            dt_ms = (time.perf_counter() - t0) * 1000.0
            latencies_us.append(dt_ms * 1000.0)
            cat["total_ms"] += dt_ms
            cat["exceptions"] += 1
            exceptions.append({
                "category": c.category,
                "text": repr(c.text),
                "error": f"{type(exc).__name__}: {exc}",
            })

    lat_sorted = sorted(latencies_us)
    p95_idx = min(len(lat_sorted) - 1, int(len(lat_sorted) * 0.95))
    return {
        "library": lib_name,
        "total_passed": sum(v["passed"] for v in by_cat.values()),
        "total_cases": sum(v["total"] for v in by_cat.values()),
        "total_exceptions": len(exceptions),
        "total_ms": round(sum(v["total_ms"] for v in by_cat.values()), 3),
        "median_us": round(statistics.median(latencies_us), 2),
        "p95_us": round(lat_sorted[p95_idx], 2),
        "by_category": {
            k: {**v, "total_ms": round(v["total_ms"], 3)} for k, v in by_cat.items()
        },
        "misses": misses,
        "exceptions": exceptions,
    }


def _eval_search_cases(lib_name: str, search_fn: Any) -> dict[str, Any]:
    passed = 0
    details: list[dict[str, Any]] = []
    t0 = time.perf_counter()
    for text, exp_tuples in SEARCH_TORTURE_CASES:
        try:
            res = search_fn(text, settings={"RELATIVE_BASE": FIXED_BASE}) or []
            got_tuples = [(dt.year, dt.month, dt.day, dt.hour, dt.minute) for _, dt in res]
            ok = got_tuples == exp_tuples
            if ok:
                passed += 1
            else:
                details.append({"text": text, "expected": exp_tuples, "got": got_tuples, "raw": [(t, str(d)) for t, d in res]})
        except Exception as exc:
            details.append({"text": text, "error": f"{type(exc).__name__}: {exc}"})
    elapsed_ms = (time.perf_counter() - t0) * 1000.0
    return {
        "library": lib_name,
        "passed": passed,
        "total": len(SEARCH_TORTURE_CASES),
        "elapsed_ms": round(elapsed_ms, 3),
        "misses": details,
    }


def _eval_redos(lib_name: str, mod: Any) -> dict[str, Any]:
    results: dict[str, Any] = {}
    total_ms = 0.0
    for name, payload in REDOS_BOMBS:
        t0 = time.perf_counter()
        try:
            got = mod.parse(payload, settings={"RELATIVE_BASE": FIXED_BASE})
            ms = (time.perf_counter() - t0) * 1000.0
            results[name] = {"ms": round(ms, 3), "ok": got is None}
        except Exception as exc:
            ms = (time.perf_counter() - t0) * 1000.0
            results[name] = {"ms": round(ms, 3), "ok": False, "error": f"{type(exc).__name__}: {exc}"}
        total_ms += ms
    return {"library": lib_name, "total_ms": round(total_ms, 3), "bombs": results}


def _isolated_throughput_and_rss(module_name: str, texts: list[str], reps: int) -> dict[str, Any]:
    payload_json = json.dumps(texts)
    code = f"""
import json, time
from datetime import datetime
try:
    import psutil
    proc = psutil.Process()
    rss0 = proc.memory_info().rss / 1048576
except Exception:
    proc = None
    rss0 = 0.0

mod = __import__({module_name!r})
cfg = {{"RELATIVE_BASE": datetime(2026, 11, 26, 15, 0, 0)}}
texts = {payload_json}
for s in ["2026-11-26T15:30:00Z", "12 December 2025", "3 gün önce", "vor 2 Wochen"]:
    mod.parse(s, settings=cfg)
t0 = time.perf_counter()
n_err = 0
for _ in range({reps}):
    for s in texts:
        try:
            mod.parse(s, settings=cfg)
        except Exception:
            n_err += 1
elapsed = time.perf_counter() - t0
rss1 = proc.memory_info().rss / 1048576 if proc else 0.0
n_ops = len(texts) * {reps}
print(json.dumps({{
    "ops": n_ops,
    "exceptions": n_err,
    "elapsed_s": round(elapsed, 4),
    "ops_per_sec": round(n_ops / elapsed, 1),
    "rss_delta_mb": round(rss1 - rss0, 2),
}}))
"""
    out = subprocess.check_output([sys.executable, "-c", code], text=True)
    return json.loads(out.strip())


def run_benchmark() -> dict[str, Any]:
    import dateoxide
    import dateparser
    from dateoxide.search import search_dates as ox_search
    from dateparser.search import search_dates as dp_search

    cases = build_cases()
    ox_eval = _eval_parser_cases("dateoxide", dateoxide, cases)
    dp_eval = _eval_parser_cases("dateparser", dateparser, cases)

    ox_search_eval = _eval_search_cases("dateoxide", ox_search)
    dp_search_eval = _eval_search_cases("dateparser", dp_search)

    ox_redos = _eval_redos("dateoxide", dateoxide)
    dp_redos = _eval_redos("dateparser", dateparser)

    valid_and_false_pos_texts = [c.text for c in cases]
    ox_tp = _isolated_throughput_and_rss("dateoxide", valid_and_false_pos_texts, reps=40)
    dp_tp = _isolated_throughput_and_rss("dateparser", valid_and_false_pos_texts, reps=2)

    return {
        "system": {
            "platform": platform.platform(),
            "machine": platform.machine(),
            "processor": platform.processor(),
            "cpu_count": os.cpu_count(),
            "python_version": platform.python_version(),
            "dateoxide_version": getattr(dateoxide, "__version__", "0.1.0"),
            "dateparser_version": getattr(dateparser, "__version__", "unknown"),
        },
        "parse_accuracy": {"dateoxide": ox_eval, "dateparser": dp_eval},
        "search_accuracy": {"dateoxide": ox_search_eval, "dateparser": dp_search_eval},
        "redos_stress": {"dateoxide": ox_redos, "dateparser": dp_redos},
        "isolated_throughput": {"dateoxide": ox_tp, "dateparser": dp_tp},
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Torture benchmark: dateoxide vs dateparser")
    parser.add_argument("--json", action="store_true", help="Emit JSON output")
    args = parser.parse_args()

    report = run_benchmark()
    if args.json:
        print(json.dumps(report, indent=2, ensure_ascii=False))
        return

    sys_info = report["system"]
    print("=" * 78)
    print(
        f"TORTURE BENCHMARK | {sys_info['platform']} | CPU cores={sys_info['cpu_count']} "
        f"| Python {sys_info['python_version']}"
    )
    print(f"dateoxide={sys_info['dateoxide_version']} vs dateparser={sys_info['dateparser_version']}")
    print("=" * 78)

    ox = report["parse_accuracy"]["dateoxide"]
    dp = report["parse_accuracy"]["dateparser"]
    print(f"\n1) TORTURE PARSE ACCURACY ({ox['total_cases']} hard cases across 5 categories)")
    print(f"{'Category':30} | {'dateoxide':16} | {'dateparser':16}")
    print("-" * 70)
    for cat in ox["by_category"]:
        ov = ox["by_category"][cat]
        dv = dp["by_category"][cat]
        print(
            f"{cat:30} | {ov['passed']:2d}/{ov['total']:2d} ({ov['total_ms']:6.2f}ms) | "
            f"{dv['passed']:2d}/{dv['total']:2d} ({dv['total_ms']:6.2f}ms)"
        )
    print("-" * 70)
    print(
        f"{'TOTAL':30} | {ox['total_passed']:2d}/{ox['total_cases']:2d} ({ox['total_ms']:6.2f}ms) | "
        f"{dp['total_passed']:2d}/{dp['total_cases']:2d} ({dp['total_ms']:6.2f}ms)"
    )
    print(f"Latency median/p95: dateoxide={ox['median_us']}us/{ox['p95_us']}us | dateparser={dp['median_us']}us/{dp['p95_us']}us")
    print(f"Exceptions/Crashes: dateoxide={ox['total_exceptions']} | dateparser={dp['total_exceptions']}")

    print(f"\n   [dateoxide misses ({len(ox['misses'])}) & exceptions ({len(ox['exceptions'])})]")
    for m in ox["misses"]:
        print(f"     MISS [{m['category']}] {m['text']} -> got {m['got']} (exp {m['expected']})")
    for e in ox["exceptions"]:
        print(f"     ERR  [{e['category']}] {e['text']} -> {e['error']}")

    print(f"\n   [dateparser misses ({len(dp['misses'])}) & exceptions ({len(dp['exceptions'])})]")
    for m in dp["misses"]:
        print(f"     MISS [{m['category']}] {m['text']} -> got {m['got']} (exp {m['expected']})")
    for e in dp["exceptions"]:
        print(f"     ERR  [{e['category']}] {e['text']} -> {e['error']}")

    os_res = report["search_accuracy"]["dateoxide"]
    ds_res = report["search_accuracy"]["dateparser"]
    print(
        f"\n2) SEARCH_DATES DIRTY TEXTS ({os_res['total']} docs): "
        f"dateoxide={os_res['passed']}/{os_res['total']} ({os_res['elapsed_ms']}ms) | "
        f"dateparser={ds_res['passed']}/{ds_res['total']} ({ds_res['elapsed_ms']}ms)"
    )

    ox_r = report["redos_stress"]["dateoxide"]
    dp_r = report["redos_stress"]["dateparser"]
    print(f"\n3) ReDoS STRESS BOMBS: dateoxide total={ox_r['total_ms']}ms | dateparser total={dp_r['total_ms']}ms")
    for bname in ox_r["bombs"]:
        print(
            f"   {bname:22}: dateoxide={ox_r['bombs'][bname]['ms']:7.3f}ms | "
            f"dateparser={dp_r['bombs'][bname]['ms']:7.3f}ms"
        )

    ox_tp = report["isolated_throughput"]["dateoxide"]
    dp_tp = report["isolated_throughput"]["dateparser"]
    speedup = ox_tp["ops_per_sec"] / max(dp_tp["ops_per_sec"], 0.1)
    print(
        f"\n4) ISOLATED THROUGHPUT + RSS on full 79-case torture corpus:\n"
        f"   dateoxide : {ox_tp['ops_per_sec']:10.1f} ops/sec (RSS delta +{ox_tp['rss_delta_mb']} MB)\n"
        f"   dateparser: {dp_tp['ops_per_sec']:10.1f} ops/sec (RSS delta +{dp_tp['rss_delta_mb']} MB)\n"
        f"   Speedup   : {speedup:.1f}x"
    )


if __name__ == "__main__":
    main()
