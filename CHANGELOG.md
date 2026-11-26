# Changelog

All notable changes to this project will be documented in this file.

## [0.1.1] - 2026-11-26

### Added
- **ISO 8601 week dates** (`YYYY-Www-D`, `YYYY-Www`, `YYYYWwwD`) in zero-regex `fast_path.rs` with `STRICT_PARSING` and `REQUIRE_PARTS` support.
- **`PARSERS=["no-spaces-time"]`** support for compact times (`1430`, `143025`) and compact 8-digit dates (`26112026` DMY, `11262026` MDY, `20261126` YMD) respecting `DATE_ORDER` and `STRICT_DATE_ORDER`.
- **Time-only `PREFER_DATES_FROM`** (`past` / `future` / `current_period`) for standalone time strings (`"09:00"`, `"18:00"`).
- **Brutal independent benchmark** (`benchmarks/torture_benchmark.py` with `--json`) evaluating accuracy, latency, ReDoS stress bombs, `search_dates`, and isolated RSS vs `dateparser`.
- **Comprehensive compatibility & fuzzing suites**: `tests/test_dateparser_parity_matrix.py`, `tests/test_differential_and_fuzz.py`, `tests/test_comprehensive_edge_cases.py`, and exact `datetime` assertions for all 205 languages in `tests/test_corpus_2000.py` (3,102+ total tests).
- **3 real-world runnable examples**: `examples/telegram_reminder_bot.py`, `examples/log_pipeline.py`, and `examples/big_data_parser.py`.
- **`crates.io` & `docs.rs` polish**: crate-level rustdoc examples, keywords, categories, README link, and `CHANGELOG.md`.

### Fixed
- **Bare 4-digit year parsing** (e.g. `"2025"` with `PREFER_MONTH_OF_YEAR="first"/"last"`) when locale detection returns no keyword candidates.
- **Year-0 / out-of-range year guard** preventing Python `ValueError: year 0 is out of range` on inputs like `"0000-01-01"`.
- **`search_dates` false-positive guards** rejecting minor-locale English word collisions (`"Let"`, `"her man"`) and dotted semantic versions (`"3.12.1"`).

## [0.1.0] - 2026-11-26

### Added
- Initial public release of `dateoxide` on PyPI (`cp39-abi3` wheels for Linux x86_64/aarch64, macOS arm64/x86_64, Windows x64 + sdist) and `dateoxide-core` on crates.io.
- 205-language compiled `postcard` locale engine, zero-regex fast path, linear-time `search_dates`, official Saudi Umm al-Qura Hijri (1343–1500 AH), Jalali, Minguo, French Republican calendars, time spans, and GIL-free Rayon `parse_batch`.
