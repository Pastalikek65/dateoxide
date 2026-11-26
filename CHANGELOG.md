# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2026-11-27

### Added
- **40 Tier-1 full-featured languages** (up from 9): added full word-numbers (`1..10`) and time-of-day anchors (`morning`/`noon`/`afternoon`/`evening`/`night`/`midnight`) across 31 additional languages (`pl`, `uk`, `cs`, `sk`, `sv`, `da`, `nb`, `fi`, `ro`, `hu`, `el`, `he`, `ar`, `fa`, `ur`, `hi`, `bn`, `id`, `ms`, `vi`, `th`, `zh`, `ja`, `ko`, `az`, `uz`, `kk`, `bg`, `hr`, `sr`, `ca`).
- **ISO 639-1 legacy language aliases** in `resolve_lang_id`: `"no" -> "nb"`, `"iw" -> "he"`, `"in" -> "id"`, `"ji" -> "yi"`, `"fil" -> "tl"`.
- **Natural & numeric date-range parser** (`dateoxide.parse_date_range` / `DateDataParser.parse_date_range` / `dateoxide_core::parse_date_range`): parses `"12-15 Ocak 2026"`, `"March 10-14, 2026"`, `"12 Ocak - 15 Şubat 2026"`, `"Dec 28 - Jan 3, 2026"`, `"from 2026-11-01 to 2026-11-15"`, and `"10:00 - 12:30"`.
- **13-language `time_spans`** (`crates/dateoxide-core/src/time_spans.rs`): added Italian, Portuguese, Dutch, Polish, Russian, Japanese, Chinese, and Korean alongside English, Turkish, German, French, and Spanish.
- **Nullable `parse_batch`** (`Sequence[str | None]`): `dateoxide.parse_batch` and `DateDataParser.parse_batch` now pass `None` elements straight through without failing.

## [0.2.0] - 2026-11-26

### Added
- **Conversational & fractional natural-language parsing**:
  - `"the day after tomorrow"`, `"the day before yesterday"` (leading `"the"` stripping + exact idioms)
  - Fractional multi-unit relative phrases: `"two and a half hours ago"`, `"in 2 and a half days"`, `"bir buçuk saat önce"`, `"vor eineinhalb Stunden"`, `"in anderthalb Monaten"`
  - Conversational clocks: `"quarter past 3"`, `"quarter to 4"`, `"half past 5"`, `"saat 3 buçuk"`, `"3'ü çeyrek geçe"`, `"saat 4'e çeyrek kala"` (with `PREFER_DATES_FROM` support)
- **`SettingValidationError` parity**: `parse(..., settings={...})` now validates unknown setting keys and invalid values with the same `SettingValidationError` exception exported by `dateoxide.conf`.
- **Modular Cargo features**: `dateoxide-core` now exposes `default = ["rayon", "calendars"]` (`cargo check --no-default-features` compiles cleanly with sequential batch fallback).
- **Criterion Rust micro-benchmarks**: added `crates/dateoxide-core/benches/parse_bench.rs`.
- **Linux `musllinux_1_2` (`x86_64`) wheels** added to `.github/workflows/release.yml` for Alpine Docker images.
- **PEP 561 `py.typed` marker** shipped alongside `dateoxide/__init__.pyi` for `mypy`/`pyright` type checking.
- **`USE_GIVEN_LANGUAGE_ORDER` enforcement** across `detect_candidates` and relative phrase resolution.
- **Timezone validation & DST gap handling**: invalid `TIMEZONE`/`TO_TIMEZONE` now raise `SettingValidationError`, and non-existent local times during DST spring-forward shift forward inside the IANA zone instead of falling back to UTC.
- **Bounded `DateDataParser` cache** honoring `CACHE_SIZE_LIMIT`.
- **Multilingual `time_spans`** extended to Turkish, German, French, and Spanish alongside English.

### Fixed
- **`release.yml` artifact upload path**: restored `path: dist` under `actions/upload-artifact@v4` in `build-wheels`.
- **`search_dates` 6/6 on torture suite**: fixed English conjunction splits (`"or on 12"` in `"Let us meet tomorrow at 5pm or on 12 December 2025."`) and kept trailing German `"Uhr"` inside matched spans (`"09:00 Uhr"`).
- **Weekday-only time consistency**: `"last Friday"` / `"geçen pazartesi"` now resolve to `00:00:00` consistently with `dateparser`.

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
