"""Real-world example: High-throughput mixed-format log timestamp normalizer.

Normalizes heterogeneous Nginx/Apache CLF, RFC 3339, RFC 2822, and bracketed app logs
to UTC timestamps using `dateoxide.parse_batch` (Rayon multi-threaded in Rust) and
aggregates error counts per minute.

Run demo:
  python examples/log_pipeline.py
"""
from __future__ import annotations

from collections import Counter
from datetime import timezone
import re
import time

import dateoxide

CLF_TS_RE = re.compile(r"\[([^\]]+)\]")
ISO_PREFIX_RE = re.compile(r"^(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)")


def extract_timestamp_candidate(line: str) -> str:
    if line.startswith("["):
        m = CLF_TS_RE.match(line)
        if m:
            return m.group(1)
    m_iso = ISO_PREFIX_RE.match(line)
    if m_iso:
        return m_iso.group(1)
    m_clf = CLF_TS_RE.search(line)
    if m_clf:
        return m_clf.group(1)
    return line


def main() -> None:
    sample_lines = [
        '10.0.0.12 - - [26/Nov/2026:15:30:00 +0000] "GET /api/health HTTP/1.1" 200 42',
        '10.0.0.15 - - [26/Nov/2026:15:30:12 +0000] "POST /api/pay HTTP/1.1" 502 128',
        '2026-11-26T15:31:05.123456+03:00 ERROR gateway timeout',
        '[2026-11-26 15:31:45] WARN cache miss for session=abc123',
        '2026-11-26T15:32:10Z INFO worker heartbeat ok',
    ] * 2000  # 10,000 log lines

    t0 = time.perf_counter()
    candidates = [extract_timestamp_candidate(line) for line in sample_lines]
    parsed = dateoxide.parse_batch(candidates, settings={"TO_TIMEZONE": "UTC"})
    elapsed = time.perf_counter() - t0

    per_minute_errors: Counter[str] = Counter()
    ok_count = 0
    for line, dt in zip(sample_lines, parsed):
        if dt is not None:
            ok_count += 1
            if " 502 " in line or "ERROR" in line:
                utc_dt = dt.astimezone(timezone.utc) if dt.tzinfo else dt.replace(tzinfo=timezone.utc)
                per_minute_errors[utc_dt.strftime("%Y-%m-%d %H:%M UTC")] += 1

    rate = len(sample_lines) / elapsed
    print(f"Processed {ok_count}/{len(sample_lines)} log lines in {elapsed*1000:.2f} ms ({rate:,.0f} lines/sec)")
    print("Error buckets:", dict(per_minute_errors))


if __name__ == "__main__":
    main()
