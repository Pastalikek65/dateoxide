# dateoxide

[![CI](https://github.com/Pastalikek65/dateoxide/actions/workflows/ci.yml/badge.svg)](https://github.com/Pastalikek65/dateoxide/actions/workflows/ci.yml)
[![PyPI version](https://img.shields.io/pypi/v/dateoxide.svg)](https://pypi.org/project/dateoxide/)
[![crates.io](https://img.shields.io/crates/v/dateoxide-core.svg)](https://crates.io/crates/dateoxide-core)
[![docs.rs](https://docs.rs/dateoxide-core/badge.svg)](https://docs.rs/dateoxide-core)
[![Python versions](https://img.shields.io/pypi/pyversions/dateoxide.svg)](https://pypi.org/project/dateoxide/)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

**Blazing-fast, low-memory, multilingual natural-language date parser written in Rust — a full-featured drop-in replacement for Python's [`dateparser`](https://github.com/scrapinghub/dateparser).**

```bash
pip install dateoxide        # Python (pre-built cp39-abi3 wheels: Linux, macOS, Windows)
cargo add dateoxide-core     # Pure Rust crate
```

`dateoxide` replaces runtime Python regex compilation and 205 dynamically imported locale modules with a compiled zero-backtracking Rust engine (`dateoxide-core`) exposed to Python via **PyO3 + maturin**.

- **108x – 7,210x faster** single-threaded parsing (`977x – 36,034x` with GIL-free parallel `parse_batch`)
- **7.4x lower memory footprint** (`+6.93 MB` vs `+51.56 MB` RSS delta across 205 languages)
- **17.3x faster cold import** (`5.21 ms` vs `90.22 ms`) via PEP 562 lazy submodules
- **Zero ReDoS** — linear-time parsing with zero catastrophic backtracking
- **3,102+ automated tests passing in <4s** — including 1,010 upstream `scrapinghub/dateparser` test vectors, exact `datetime` assertions across 205 languages, `dateparser` compatibility & fuzzing matrices, known `dateparser` bug regressions, and 3,792 official Saudi Umm al-Qura Hijri dates
- **Official Saudi Umm al-Qura Hijri Calendar (1343–1500 AH)** embedded directly in pure Rust (3,792/3,792 month-start & month-end dates verified against `hijridate`, with zero runtime dependencies), plus Jalali, Minguo, and French Republican calendars
- **Complete `dateparser` feature parity**:
  - `parse()`, `DateDataParser` (`get_date_data`, `get_date_tuple`, `parts`, `part_of_day`), `search_dates(..., add_period=..., add_detected_language=...)`
  - `Settings`, `SettingValidationError`, `PartOfDay`, `PartsOfDay`, `apply_settings`, `conf`
  - `FreshnessDateDataParser` (`freshness_date_parser`) & `timezone_parser.pop_tz_offset_from_string`
  - `utils.time_spans` (`detect_time_span`, `generate_time_span`, `RETURN_TIME_SPAN`, `DEFAULT_START_OF_WEEK`, `DEFAULT_DAYS_IN_MONTH`)
  - `IGNORE_SURROUNDING_TEXT`, `STRICT_DATE_ORDER`, `NORMALIZE`, `PARSERS`, `SKIP_TOKENS`, `RETURN_TIME_AS_PERIOD`, `TIMEZONE_ABBREVIATIONS`, `region`, and `detect_languages_function`

---

## Benchmarks

Measured with [`benchmarks/bench_vs_dateparser.py`](benchmarks/bench_vs_dateparser.py) on Python 3.12.10, `dateparser 1.5.1`, Rust 1.99.1 (`--release` with thin LTO).

### Cold Import & Memory Footprint (Isolated Subprocesses)

| Metric | `dateparser` (Python) | `dateoxide` (Rust) | Improvement |
| :--- | ---: | ---: | ---: |
| **Cold import time** | 90.22 ms | **5.21 ms** | **17.3x faster** |
| **RSS after import** | +8.06 MB | **+0.76 MB** | **10.6x smaller** |
| **RSS delta after parsing** | +51.56 MB | **+6.93 MB** | **7.4x less RAM** |

### Parsing Throughput (Operations / Second)

| Workload | `dateparser.parse` | `dateoxide.parse` | `dateoxide.DateDataParser` | `dateoxide.parse_batch` *(Rayon)* |
| :--- | ---: | ---: | ---: | ---: |
| **ISO / RFC / Numeric** | 440 /s | **672,789 /s** *(1,531x)* | **3,168,819 /s** *(7,210x)* | **5,364,615 /s** *(12,206x)* |
| **Multilingual Natural Language (Auto)** | 1,239 /s | **133,916 /s** *(108x)* | **160,849 /s** *(130x)* | **1,211,446 /s** *(977x)* |
| **Invalid / Non-Date Text (Worst Case)** | 60 /s | **196,333 /s** *(3,279x)* | **262,130 /s** *(4,378x)* | **2,157,386 /s** *(36,034x)* |

Reproduce locally (or run the accuracy-checked torture suite):
```bash
python benchmarks/bench_vs_dateparser.py
python benchmarks/torture_benchmark.py --json
```

### Independent Torture Benchmark (Accuracy, ReDoS & Known Differences)

Measured with [`benchmarks/torture_benchmark.py`](benchmarks/torture_benchmark.py) across 79 adversarial cases (multilingual casing/scripts, fractional/conversational phrases, month-end clamping, leap/ISO-week traps, RFC/CLF/DST, false-positive traps), dirty `search_dates`, and 6 ReDoS stress bombs (`dateoxide 0.2.0` vs `dateparser 1.5.1` on Python 3.12.10):

| Metric | `dateparser 1.5.1` | `dateoxide 0.2.0` |
| :--- | ---: | ---: |
| **Torture accuracy (79 hard cases)** | 66 / 79 (`4,243.30 ms`) | **78 / 79 (`1.60 ms`)** |
| **Dirty `search_dates` (6 docs)** | 4 / 6 (`1,885.91 ms`) | **6 / 6 (`0.48 ms`)** |
| **6 ReDoS stress bombs (3k–5k chars)** | `20,111.62 ms` | **`5.46 ms`** |
| **Full 79-case isolated throughput & RSS** | `57.4 /s` (`+64.52 MB`) | **`20,629.9 /s` (`+10.24 MB`)** *(359.4x)* |

**Transparently noted difference (`1 / 79`):**
- `"2026-13-01"` swaps to `2026-01-13` under default `STRICT_DATE_ORDER="none"` in **both** `dateoxide` and `dateparser` (set `STRICT_DATE_ORDER="all"` to reject invalid month indices strictly).

---

## Upstream `dateparser` Bugs Fixed / Avoided

Tested directly in [`tests/test_dateparser_bugs.py`](tests/test_dateparser_bugs.py):

- **Compact ISO-8601 basic (`20261126T153000Z`):** `dateparser 1.5.1` misparses this as year `1608-01-02`; `dateoxide` returns `2026-11-26 15:30:00+00:00`.
- **Quadratic ReDoS & punctuation runs (`GHSA-rx9v-94ff-9cc2`, `#426`):** `5,000` repeated dots/dashes/Arabic commas return `None` in microseconds.
- **Long-running memory & latency stability (`#457`, `#1140`):** Stateless Rust core with zero unbounded `previous_locales` growth.
- **Timestamp fragmentation in `search_dates` (`#968`):** `"Server rebooted at 2026-11-26 15:30:00 UTC successfully."` stays grouped as a single combined match.
- **Extreme overflow protection:** Inputs like `"+999999999999999 days ago"` or `"5000 years ago"` cleanly return `None` instead of overflowing or raising out-of-range errors.
- **Turkish `İ`/`I`/`ı` casing & leap/month-end clamping:** `"15 OCAK 2026"`, `"3 GÜN ÖNCE"`, `"DÜN SAAT 14:30"` work out of the box without false-positive city names like `"İSTANBUL"`.

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

JalaliCalendar("۵ آذر ۱۴۰۴").get_date()                 # 2025-11-26
HijriCalendar("1 Ramadan 1446").get_date()             # 2025-03-01 (Official Umm al-Qura)
MinguoCalendar("115年11月26日").get_date()             # 2026-11-26
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

### Parallel Batch Parsing (Nullable) & Date Ranges

```python
import dateoxide

dates = ["2026-11-26", None, "3 gün önce", "vor 2 Wochen", "not a date"]
results = dateoxide.parse_batch(dates)  # GIL-free Rayon across all CPU cores

start, end = dateoxide.parse_date_range("12 Ocak - 15 Şubat 2026")
start, end = dateoxide.parse_date_range("March 10-14, 2026")
start, end = dateoxide.parse_date_range("from 2026-11-01 to 2026-11-15")
```

---

## Language Support Tiers (205 Languages)

| Tier | Count | Included Capabilities | Languages |
| :--- | :---: | :--- | :--- |
| **Tier 1 (Full NLP)** | **40** | 12 months, 7 weekdays, `relative_exact`, `relative_affix`, `word_numbers`, `time_of_day` | `en`, `tr`, `de`, `fr`, `es`, `pt`, `it`, `nl`, `ru`, `pl`, `uk`, `cs`, `sk`, `sv`, `da`, `nb`/`no`, `fi`, `ro`, `hu`, `el`, `he`, `ar`, `fa`, `ur`, `hi`, `bn`, `id`, `ms`, `vi`, `th`, `zh`, `ja`, `ko`, `az`, `uz`, `kk`, `bg`, `hr`, `sr`, `ca` |
| **Tier 2 (Full Calendar + Relative)** | **121** | 12 months, 7 weekdays, `relative_exact`, dynamic `relative_affix` (`N units ago/in`) | Tier 1 + 81 additional world languages (African, South/SE Asian, Celtic, Baltic, Caucasian, etc.) |
| **Tier 3 (CLDR Calendar + Exact Relatives)** | **205** | 12 months, 7 weekdays, `units`, 204 `relative_exact` phrases, Unicode numerals | All 205 Unicode CLDR / `dateparser` locales |

*(Multilingual `time_spans` covers 13 languages: EN, TR, DE, FR, ES, IT, PT, NL, PL, RU, JA, ZH, KO.)*

### Real-World Examples (`examples/`)

- [`examples/telegram_reminder_bot.py`](examples/telegram_reminder_bot.py) — Multilingual Telegram reminder parser & UTC scheduler
- [`examples/log_pipeline.py`](examples/log_pipeline.py) — Mixed Nginx CLF / RFC 3339 / bracketed log normalizer with `parse_batch`
- [`examples/big_data_parser.py`](examples/big_data_parser.py) — 50,000-record multilingual ETL throughput & RSS demo

```bash
python examples/telegram_reminder_bot.py
python examples/log_pipeline.py
python examples/big_data_parser.py
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
let hijri = calendars::parse_hijri("1446/09/01", &Settings::default());
```

---

## Architecture: Why Is It So Fast & Lean?

1. **Zero-Regex Byte Scanner Fast Path (`fast_path.rs`):** Extended & compact ISO-8601, RFC-2822/3339, delimited numeric dates, standalone compact times (`5:30p.m.`, `09h15`), and Unix timestamps are parsed directly from bytes without touching regexes or locale tables.
2. **Compile-Time Binary Locale Database (`build.rs` + `postcard`):** All 205 languages (including script-tagged locales like `az-Cyrl`, `sr-Latn`, `uz-Cyrl`) are compiled at build time into a compact embedded binary (`include_bytes!`) lazily loaded via `OnceLock` on first natural-language parse — eliminating Python's 205-module import tax.
3. **Zero-Regex Structured Relative Patterns (`relative.rs`):** 99.97% of `dateparser`'s 3,621 `relative-type-regex` entries are pure `<prefix><NUMBER><suffix>` shapes. `dateoxide` pre-compiles them into flat prefix/suffix matchers with guaranteed linear time and zero ReDoS risk, plus multi-sense relative disambiguation (`PREFER_DATES_FROM`), bare durations, and configurable `PartsOfDay`.
4. **O(tokens) Inverted Language Index + Diacritic Folding (`locale/mod.rs`):** Instead of iterating through 205 languages on unknown input, `dateoxide` scores exact and diacritic-folded tokens in `FxHashMap` lookups (plus Unicode script detection for Korean, Japanese, Chinese, Cyrillic, Arabic, Greek, Hebrew, Thai, Devanagari), rejecting non-date strings in microseconds.
5. **Native Rust Calendars & Time Spans (`calendars.rs`, `umm_al_qura.rs`, `time_spans.rs`):** Official Saudi Umm al-Qura Hijri lookup table (1343–1500 AH), Solar Hijri (Jalali), Minguo, and French Republican calendar arithmetic plus compiled linear-time span detection.
6. **GIL-Free Rayon Parallelism (`parse_batch`):** Releases Python's GIL via `py.allow_threads` and fans out across all CPU cores with Rayon.

---

## Development & Publishing

```bash
# Build & install release Python extension locally
maturin develop --release

# Run Rust core tests
cargo test -p dateoxide-core --release

# Run full 3,000+ test suite (including parity matrix, fuzzing & Umm al-Qura Hijri)
pytest -v

# Run standard & torture head-to-head benchmarks vs dateparser
python benchmarks/bench_vs_dateparser.py
python benchmarks/torture_benchmark.py
```

Automated PyPI (OIDC Trusted Publishing) and crates.io multi-platform wheel builds trigger on release tags:
```bash
git tag v0.1.0 && git push --tags
```

---

## License & Attribution

Dual-licensed under **MIT** or **Apache-2.0**.

Multilingual calendar and relative-date lexical data is derived from Unicode CLDR and [`scrapinghub/dateparser`](https://github.com/scrapinghub/dateparser) (BSD 3-Clause License). Official Saudi Umm al-Qura month-start data (1343–1500 AH) is derived from [`dralshehri/hijridate`](https://github.com/dralshehri/hijridate) (MIT License). See [`LICENSE`](LICENSE) and [`NOTICE`](NOTICE) for full details.
