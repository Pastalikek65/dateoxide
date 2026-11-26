# dateoxide

**Blazing-fast, low-memory, multilingual natural-language date parser written in Rust — a full-featured drop-in replacement for Python's [`dateparser`](https://github.com/scrapinghub/dateparser).**

`dateoxide` replaces runtime Python regex compilation and 205 dynamically imported locale modules with a compiled zero-backtracking Rust engine (`dateoxide-core`) exposed to Python via **PyO3 + maturin**.

- **128x – 8,587x faster** single-threaded parsing (`1,172x – 51,298x` with GIL-free parallel `parse_batch`)
- **7.5x lower memory footprint** (`+6.84 MB` vs `+51.39 MB` RSS delta across 205 languages)
- **16.8x faster cold import** (`5.59 ms` vs `94.17 ms`) via PEP 562 lazy submodules
- **Zero ReDoS** — no backtracking regular expressions on hot paths
- **205/205 languages verified** out of the box (Turkish, English, German, French, Spanish, Italian, Portuguese, Russian, Ukrainian, Polish, Czech, Dutch, Swedish, Greek, Hebrew, Arabic, Persian, Hindi, Korean, Japanese, Chinese, Indonesian, Vietnamese, Thai, and 181 more)
- **Complete `dateparser` feature parity**:
  - `parse()`, `DateDataParser` (`get_date_data`, `get_date_tuple`, `parts`, `part_of_day`), `search_dates(..., add_period=..., add_detected_language=...)`
  - `Settings`, `SettingValidationError`, `PartOfDay`, `PartsOfDay`, `apply_settings`, `conf`
  - `FreshnessDateDataParser` (`freshness_date_parser`) & `timezone_parser.pop_tz_offset_from_string`
  - `utils.time_spans` (`detect_time_span`, `generate_time_span`, `RETURN_TIME_SPAN`, `DEFAULT_START_OF_WEEK`, `DEFAULT_DAYS_IN_MONTH`)
  - Native pure-Rust non-Gregorian calendars: `JalaliCalendar`, `HijriCalendar`, `MinguoCalendar`, `FrenchRepublicanCalendar`
  - `IGNORE_SURROUNDING_TEXT`, `STRICT_DATE_ORDER`, `NORMALIZE`, `PARSERS`, `SKIP_TOKENS`, `RETURN_TIME_AS_PERIOD`, `TIMEZONE_ABBREVIATIONS`, `region`, and `detect_languages_function`

---

## Benchmarks

Measured with [`benchmarks/bench_vs_dateparser.py`](benchmarks/bench_vs_dateparser.py) on Python 3.12.10, `dateparser 1.5.1`, Rust 1.99.1 (`--release` with thin LTO).

### Cold Import & Memory Footprint (Isolated Subprocesses)

| Metric | `dateparser` (Python) | `dateoxide` (Rust) | Improvement |
| :--- | ---: | ---: | ---: |
| **Cold import time** | 94.17 ms | **5.59 ms** | **16.8x faster** |
| **RSS after import** | +8.13 MB | **+0.73 MB** | **11.1x smaller** |
| **RSS delta after parsing** | +51.39 MB | **+6.84 MB** | **7.5x less RAM** |

### Parsing Throughput (Operations / Second)

| Workload | `dateparser.parse` | `dateoxide.parse` | `dateoxide.DateDataParser` | `dateoxide.parse_batch` *(Rayon)* |
| :--- | ---: | ---: | ---: | ---: |
| **ISO / RFC / Numeric** | 430 /s | **679,436 /s** *(1,578x)* | **3,696,175 /s** *(8,587x)* | **5,260,113 /s** *(12,220x)* |
| **Multilingual Natural Language (Auto)** | 1,190 /s | **151,958 /s** *(128x)* | **187,011 /s** *(157x)* | **1,394,831 /s** *(1,172x)* |
| **Invalid / Non-Date Text (Worst Case)** | 59 /s | **261,941 /s** *(4,445x)* | **396,308 /s** *(6,726x)* | **3,022,828 /s** *(51,298x)* |

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
dateparser.parse("2 days")               # bare relative durations
dateparser.parse("3 gün önce")
dateparser.parse("3 gun once")           # diacritic-folded normalization
dateparser.parse("dün gece")             # time-of-day phrases
dateparser.parse("yesterday at noon")
dateparser.parse("vor 2 Wochen")
dateparser.parse("hier à 14h30")
dateparser.parse("12 de diciembre de 2025")
dateparser.parse("2026년 11월 26일")      # Korean
dateparser.parse("2026年11月26日")        # Japanese / Chinese
dateparser.parse("١٢ ديسمبر ٢٠٢٥")        # Arabic-Indic numerals
dateparser.parse("20261126T153000Z")     # Compact ISO-8601
dateparser.parse("Published: 12 December 2025", settings={"IGNORE_SURROUNDING_TEXT": True})
```

### `Settings`, `PartsOfDay`, `DateData.parts`, and `DateData.part_of_day`

```python
from datetime import time
from dateoxide import DateDataParser, PartsOfDay, Settings

cfg = Settings(
    DATE_ORDER="MDY",
    STRICT_DATE_ORDER="all",
    PARTS_OF_DAY=PartsOfDay(morning=time(8, 30)),
    TIMEZONE_ABBREVIATIONS={"FOO": "+04:30"},
)
parser = DateDataParser(settings=cfg)
dd = parser.get_date_data("yesterday morning")
print(dd.date_obj, dd.parts, dd.part_of_day)
# 2026-11-25 08:30:00 ('year', 'month', 'day', 'time') PartOfDay.MORNING
```

### Non-Gregorian Calendars (Zero-Dependency Native Rust)

```python
from dateoxide.calendars import (
    JalaliCalendar,
    HijriCalendar,
    MinguoCalendar,
    FrenchRepublicanCalendar,
)

JalaliCalendar("۵ آذر ۱۴۰۴").get_date()              # 2025-11-26
HijriCalendar("1 Ramadan 1446").get_date()          # 2025-03-01
MinguoCalendar("115年11月26日").get_date()          # 2026-11-26
FrenchRepublicanCalendar("14 Germinal an III").get_date()  # 1795-04-03
```

### Time Spans, Freshness Parser & `search_dates`

```python
from dateoxide.freshness_date_parser import freshness_date_parser
from dateoxide.search import search_dates
from dateoxide.utils import time_spans

freshness_date_parser.parse("3 gün önce")
span = time_spans.detect_time_span("Show errors from past 3 days")
start, end = time_spans.generate_time_span(span)

matches = search_dates(
    "Event in December 2025; follow-up 12 Jan 2026",
    add_period=True,
    add_detected_language=True,
)
# [('December 2025', datetime(2025, 12, ...), 'month', 'en'), ('12 Jan 2026', datetime(2026, 1, 12, 0, 0), 'day', 'en')]
```

### Parallel Batch Parsing (GIL-free, Multi-Core)

```python
import dateoxide

dates = ["2026-11-26", "3 gün önce", "vor 2 Wochen", "not a date"] * 25_000
results = dateoxide.parse_batch(dates)  # Runs across all CPU cores via Rayon
```

---

## Standalone Rust Usage (`dateoxide-core`)

`dateoxide-core` has zero Python dependencies and can be used directly in Rust applications:

```rust
use dateoxide_core::{calendars, parse, parse_batch, search_dates, settings::Settings};

let dt = parse("3 gün önce").unwrap();
let found = search_dates("Meeting on 12 December 2025", &Settings::default());
let batch = parse_batch(&["2026-11-26", "vor 2 Wochen", "3日前"], &Settings::default());
let jalali = calendars::parse_jalali("۵ آذر ۱۴۰۴", &Settings::default());
```

---

## Architecture: Why Is It So Fast & Lean?

1. **Zero-Regex Byte Scanner Fast Path (`fast_path.rs`):** Extended & compact ISO-8601, RFC-2822/3339, delimited numeric dates, standalone compact times (`5:30p.m.`, `09h15`), and Unix timestamps are parsed directly from bytes without touching regexes or locale tables.
2. **Compile-Time Binary Locale Database (`build.rs` + `postcard`):** All 205 languages (including script-tagged locales like `az-Cyrl`, `sr-Latn`, `uz-Cyrl`) are compiled at build time into a compact embedded binary (`include_bytes!`) lazily loaded via `OnceLock` on first natural-language parse — eliminating Python's 205-module import tax.
3. **Zero-Regex Structured Relative Patterns (`relative.rs`):** 99.97% of `dateparser`'s 3,621 `relative-type-regex` entries are pure `<prefix><NUMBER><suffix>` shapes. `dateoxide` pre-compiles them into flat prefix/suffix matchers with guaranteed linear time and zero ReDoS risk, plus multi-sense relative disambiguation (`PREFER_DATES_FROM`), bare durations, and configurable `PartsOfDay`.
4. **O(tokens) Inverted Language Index + Diacritic Folding (`locale/mod.rs`):** Instead of iterating through 205 languages on unknown input, `dateoxide` scores exact and diacritic-folded tokens in `FxHashMap` lookups (plus Unicode script detection for Korean, Japanese, Chinese, Cyrillic, Arabic, Greek, Hebrew, Thai, Devanagari), rejecting non-date strings in microseconds.
5. **Native Rust Calendars & Time Spans (`calendars.rs`, `time_spans.rs`):** Zero-dependency Solar Hijri (Jalali), Islamic Hijri, Minguo, and French Republican calendar arithmetic plus compiled linear-time span detection.
6. **GIL-Free Rayon Parallelism (`parse_batch`):** Releases Python's GIL via `py.allow_threads` and fans out across all CPU cores with Rayon.

---

## Development

```bash
# Build & install release Python extension locally
maturin develop --release

# Run Rust core tests
cargo test -p dateoxide-core --release

# Run full Python parity & 205-language test suite
pytest -v

# Run head-to-head benchmark vs dateparser
python benchmarks/bench_vs_dateparser.py
```

---

## License & Attribution

Dual-licensed under **MIT** or **Apache-2.0**.

Multilingual calendar and relative-date lexical data is derived from Unicode CLDR and [`scrapinghub/dateparser`](https://github.com/scrapinghub/dateparser) (BSD 3-Clause License). See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE) for full details.
