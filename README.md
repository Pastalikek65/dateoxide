# dateoxide

**Blazing-fast, low-memory, multilingual natural-language date parser written in Rust — a drop-in accelerator for Python's [`dateparser`](https://github.com/scrapinghub/dateparser).**

`dateoxide` replaces runtime Python regex compilation and 205 dynamically imported locale modules with a compiled zero-backtracking Rust engine (`dateoxide-core`) exposed to Python via **PyO3 + maturin**.

- **290x – 10,680x faster** single-threaded parsing (`2,479x – 82,228x` with GIL-free parallel `parse_batch`)
- **9.7x lower memory footprint** (`+5.3 MB` vs `+51.4 MB` RSS delta across 205 languages)
- **29.8x faster cold import** (`3.2 ms` vs `95.5 ms`)
- **Zero ReDoS** — no backtracking regular expressions on hot paths
- **205 languages supported** out of the box (Turkish, English, German, French, Spanish, Russian, Japanese, Chinese, Arabic, and 196 more)
- **Drop-in compatible API**: `parse()`, `DateDataParser`, `search_dates()`, and `settings`

---

## Benchmarks

Measured with [`benchmarks/bench_vs_dateparser.py`](benchmarks/bench_vs_dateparser.py) on Python 3.12.10, `dateparser 1.5.1`, Rust 1.99.1 (`--release` with thin LTO).

### Cold Import & Memory Footprint (Isolated Subprocesses)

| Metric | `dateparser` (Python) | `dateoxide` (Rust) | Improvement |
| :--- | ---: | ---: | ---: |
| **Cold import time** | 95.53 ms | **3.21 ms** | **29.8x faster** |
| **RSS after import** | +7.98 MB | **+0.20 MB** | **39.9x smaller** |
| **RSS delta after parsing** | +51.39 MB | **+5.32 MB** | **9.7x less RAM** |

### Parsing Throughput (Operations / Second)

| Workload | `dateparser.parse` | `dateoxide.parse` | `dateoxide.DateDataParser` | `dateoxide.parse_batch` *(Rayon)* |
| :--- | ---: | ---: | ---: | ---: |
| **ISO / RFC / Numeric** | 444 /s | **1,517,316 /s** *(3,419x)* | **4,739,898 /s** *(10,680x)* | **5,491,488 /s** *(12,373x)* |
| **Multilingual Natural Language (Auto)** | 1,207 /s | **351,043 /s** *(291x)* | **462,029 /s** *(383x)* | **2,992,471 /s** *(2,479x)* |
| **Invalid / Non-Date Text (Worst Case)** | 59 /s | **584,484 /s** *(9,858x)* | **830,420 /s** *(14,006x)* | **4,875,480 /s** *(82,228x)* |

Reproduce locally:
```bash
python benchmarks/bench_vs_dateparser.py
```

---

## Quick Start (Python)

### Drop-in replacement for `dateparser`

```python
import dateoxide as dateparser

dateparser.parse("3 days ago")
dateparser.parse("3 gün önce")
dateparser.parse("vor 2 Wochen")
dateparser.parse("hier à 14h30")
dateparser.parse("12 de diciembre de 2025")
dateparser.parse("2026年11月26日")
dateparser.parse("2026-11-26T12:00:00Z", settings={"TO_TIMEZONE": "Europe/Istanbul"})
```

### Reusable `DateDataParser` (Zero per-call settings overhead)

```python
from dateoxide import DateDataParser

parser = DateDataParser(
    languages=["tr", "en"],
    settings={"DATE_ORDER": "DMY", "PREFER_DATES_FROM": "past"},
)
data = parser.get_date_data("12 Aralık 2025")
print(data.date_obj, data.period, data.locale)
```

### Parallel Batch Parsing (GIL-free, Multi-Core)

```python
import dateoxide

dates = ["2026-11-26", "3 gün önce", "vor 2 Wochen", "not a date"] * 25_000
results = dateoxide.parse_batch(dates)  # Runs across all CPU cores via Rayon
```

### Extract Dates from Free-Form Text (`search_dates`)

```python
from dateoxide.search import search_dates

text = "Toplantı 15 Ocak 2026 tarihinde yapılacak, önceki görüşme 3 gün önce bitti."
matches = search_dates(text)
# [('15 Ocak 2026', datetime(2026, 1, 15, 0, 0)), ('3 gün önce', datetime(...))]
```

---

## Standalone Rust Usage (`dateoxide-core`)

`dateoxide-core` has zero Python dependencies and can be used directly in Rust applications:

```rust
use dateoxide_core::{parse, parse_batch, search_dates, settings::Settings};

let dt = parse("3 gün önce").unwrap();
let found = search_dates("Meeting on 12 December 2025", &Settings::default());
let batch = parse_batch(&["2026-11-26", "vor 2 Wochen", "3日前"], &Settings::default());
```

---

## Architecture: Why Is It So Fast & Lean?

1. **Zero-Regex Byte Scanner Fast Path (`fast_path.rs`):** ISO-8601, RFC-2822/3339, delimited numeric dates, compact `YYYYMMDD`, and Unix timestamps are parsed directly from bytes without touching regexes or locale tables.
2. **Compile-Time Binary Locale Database (`build.rs` + `postcard`):** All 205 languages are compiled at build time into a compact embedded binary (`include_bytes!`) lazily loaded via `OnceLock` on first natural-language parse — eliminating Python's 205-module import tax.
3. **Zero-Regex Structured Relative Patterns (`relative.rs`):** 99.97% of `dateparser`'s 3,621 `relative-type-regex` entries are pure `<prefix><NUMBER><suffix>` shapes. `dateoxide` pre-compiles them into flat prefix/suffix matchers with guaranteed linear time and zero ReDoS risk.
4. **O(tokens) Inverted Language Index (`locale/mod.rs`):** Instead of iterating through 205 languages on unknown input, `dateoxide` votes across tokens in a single `FxHashMap` lookup (plus Unicode script detection for CJK, Cyrillic, Arabic, Devanagari, etc.), rejecting non-date strings in nanoseconds.
5. **GIL-Free Rayon Parallelism (`parse_batch`):** Releases Python's GIL via `py.allow_threads` and fans out across all CPU cores with Rayon.

---

## Development

```bash
# Build & install release Python extension locally
maturin develop --release

# Run Rust core tests
cargo test -p dateoxide-core --release

# Run Python compatibility & multilingual test suite
pytest -v

# Run head-to-head benchmark vs dateparser
python benchmarks/bench_vs_dateparser.py
```

---

## License & Attribution

Dual-licensed under **MIT** or **Apache-2.0**.

Multilingual calendar and relative-date lexical data is derived from Unicode CLDR and [`scrapinghub/dateparser`](https://github.com/scrapinghub/dateparser) (BSD 3-Clause License). See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE) for full details.
