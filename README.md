# dateoxide

**Blazing-fast, low-memory, multilingual natural-language date parser written in Rust — a drop-in accelerator for Python's [`dateparser`](https://github.com/scrapinghub/dateparser).**

`dateoxide` replaces runtime Python regex compilation and 205 dynamically imported locale modules with a compiled zero-backtracking Rust engine (`dateoxide-core`) exposed to Python via **PyO3 + maturin**.

- **158x – 10,527x faster** single-threaded parsing (`1,300x – 54,133x` with GIL-free parallel `parse_batch`)
- **8.1x lower memory footprint** (`+6.36 MB` vs `+51.47 MB` RSS delta across 205 languages)
- **33.2x faster cold import** (`2.55 ms` vs `84.72 ms`)
- **Zero ReDoS** — no backtracking regular expressions on hot paths
- **205/205 languages verified** out of the box (Turkish, English, German, French, Spanish, Italian, Portuguese, Russian, Ukrainian, Polish, Czech, Dutch, Swedish, Greek, Hebrew, Arabic, Persian, Hindi, Korean, Japanese, Chinese, Indonesian, Vietnamese, Thai, and 181 more)
- **Accent-tolerant normalization (`NORMALIZE=True`)**, time-of-day phrases (`"yesterday at noon"`, `"tonight"`, `"dün gece"`), compact ISO (`"20261126T153000Z"`), Apache log timestamps, and full `dateparser` settings compatibility (`DATE_ORDER`, `region`, `PARSERS`, `SKIP_TOKENS`, `RETURN_TIME_AS_PERIOD`, `PREFER_DATES_FROM`, `search_dates(..., add_detected_language=True)`)

---

## Benchmarks

Measured with [`benchmarks/bench_vs_dateparser.py`](benchmarks/bench_vs_dateparser.py) on Python 3.12.10, `dateparser 1.5.1`, Rust 1.99.1 (`--release` with thin LTO).

### Cold Import & Memory Footprint (Isolated Subprocesses)

| Metric | `dateparser` (Python) | `dateoxide` (Rust) | Improvement |
| :--- | ---: | ---: | ---: |
| **Cold import time** | 84.72 ms | **2.55 ms** | **33.2x faster** |
| **RSS after import** | +8.27 MB | **+0.19 MB** | **43.5x smaller** |
| **RSS delta after parsing** | +51.47 MB | **+6.36 MB** | **8.1x less RAM** |

### Parsing Throughput (Operations / Second)

| Workload | `dateparser.parse` | `dateoxide.parse` | `dateoxide.DateDataParser` | `dateoxide.parse_batch` *(Rayon)* |
| :--- | ---: | ---: | ---: | ---: |
| **ISO / RFC / Numeric** | 448 /s | **1,201,923 /s** *(2,684x)* | **4,713,646 /s** *(10,527x)* | **5,408,914 /s** *(12,080x)* |
| **Multilingual Natural Language (Auto)** | 1,251 /s | **197,634 /s** *(158x)* | **229,369 /s** *(183x)* | **1,626,104 /s** *(1,300x)* |
| **Invalid / Non-Date Text (Worst Case)** | 59 /s | **350,424 /s** *(5,981x)* | **457,746 /s** *(7,813x)* | **3,171,462 /s** *(54,133x)* |

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
dateparser.parse("3 gun once")          # diacritic-folded normalization
dateparser.parse("dün gece")            # time-of-day phrases
dateparser.parse("yesterday at noon")
dateparser.parse("vor 2 Wochen")
dateparser.parse("hier à 14h30")
dateparser.parse("12 de diciembre de 2025")
dateparser.parse("2026년 11월 26일")     # Korean
dateparser.parse("2026年11月26日")       # Japanese / Chinese
dateparser.parse("١٢ ديسمبر ٢٠٢٥")       # Arabic-Indic numerals
dateparser.parse("20261126T153000Z")    # Compact ISO-8601
dateparser.parse("2026-11-26T12:00:00Z", settings={"TO_TIMEZONE": "Europe/Istanbul"})
```

### Settings & Regional Compatibility

```python
import dateoxide

# Region-aware numeric ordering
dateoxide.parse("05-11-2026", region="US")  # 2026-05-11
dateoxide.parse("05-11-2026", region="GB")  # 2026-11-05

# Skip noisy prefixes, filter enabled parsers, toggle diacritic normalization
dateoxide.parse("Published: 12 December 2025", settings={"SKIP_TOKENS": ["Published:"]})
dateoxide.parse("15 Subat 2026", settings={"NORMALIZE": False})  # None (strict accents)
dateoxide.parse("3 days ago", settings={"PARSERS": ["absolute-time"]})  # None
```

### Reusable `DateDataParser` (Zero per-call settings overhead)

```python
from dateoxide import DateDataParser

parser = DateDataParser(
    languages=["tr", "en"],
    settings={"DATE_ORDER": "DMY", "PREFER_DATES_FROM": "past", "RETURN_TIME_AS_PERIOD": True},
)
data = parser.get_date_data("12 Aralık 2025 14:30")
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
matches = search_dates(text, add_detected_language=True)
# [('15 Ocak 2026', datetime(2026, 1, 15, 0, 0), 'tr'), ('3 gün önce', datetime(...), 'tr')]
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

1. **Zero-Regex Byte Scanner Fast Path (`fast_path.rs`):** Extended & compact ISO-8601, RFC-2822/3339, delimited numeric dates, standalone compact times (`5:30p.m.`, `09h15`), and Unix timestamps are parsed directly from bytes without touching regexes or locale tables.
2. **Compile-Time Binary Locale Database (`build.rs` + `postcard`):** All 205 languages (including script-tagged locales like `az-Cyrl`, `sr-Latn`, `uz-Cyrl`) are compiled at build time into a compact embedded binary (`include_bytes!`) lazily loaded via `OnceLock` on first natural-language parse — eliminating Python's 205-module import tax.
3. **Zero-Regex Structured Relative Patterns (`relative.rs`):** 99.97% of `dateparser`'s 3,621 `relative-type-regex` entries are pure `<prefix><NUMBER><suffix>` shapes. `dateoxide` pre-compiles them into flat prefix/suffix matchers with guaranteed linear time and zero ReDoS risk, plus multi-sense relative disambiguation (`PREFER_DATES_FROM`) and time-of-day anchoring.
4. **O(tokens) Inverted Language Index + Diacritic Folding (`locale/mod.rs`):** Instead of iterating through 205 languages on unknown input, `dateoxide` scores exact and diacritic-folded tokens in `FxHashMap` lookups (plus Unicode script detection for Korean, Japanese, Chinese, Cyrillic, Arabic, Greek, Hebrew, Thai, Devanagari), rejecting non-date strings in microseconds.
5. **GIL-Free Rayon Parallelism (`parse_batch`):** Releases Python's GIL via `py.allow_threads` and fans out across all CPU cores with Rayon.

---

## Development

```bash
# Build & install release Python extension locally
maturin develop --release

# Run Rust core tests
cargo test -p dateoxide-core --release

# Run Python compatibility & 205-language test suite
pytest -v

# Run head-to-head benchmark vs dateparser
python benchmarks/bench_vs_dateparser.py
```

---

## License & Attribution

Dual-licensed under **MIT** or **Apache-2.0**.

Multilingual calendar and relative-date lexical data is derived from Unicode CLDR and [`scrapinghub/dateparser`](https://github.com/scrapinghub/dateparser) (BSD 3-Clause License). See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE) for full details.
