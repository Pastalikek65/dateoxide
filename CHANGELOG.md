# Changelog

All notable changes to this project will be documented in this file.

## [0.6.1] - 2026-11-27

### Fixed
- **Microsecond `%f` in `date_formats`**: normalized Python `.%f` to `chrono`'s `%.f` inside `try_strptime_any` so `"2024-05-15 14:30:00.123456"` with `["%Y-%m-%d %H:%M:%S.%f"]` preserves full `123456` µs precision instead of truncating to nanoseconds (`123` µs).
- **`STRICT_PARSING=True` on relative expressions**: relative expressions (`"yesterday"`, `"2 days ago"`, `"in 3 hours"`) now resolve relative to `RELATIVE_BASE` under `STRICT_PARSING=True` consistently with `dateparser`.
- **Attached shorthand hours (`in 3h`, `il y a 3h`, `dans 3h`)**: prevented trailing time extraction from mistaking duration suffixes like `"3h"` for clock time `03:00` when no relative-exact base precedes it, resolving 100% of `dateparser`'s 3,620 complex `relative-type-regex` patterns.
- **European/Asian time connectors & CJK/Minguo eras**: added Romanian (`la`/`ora`), Slovak (`o`/`v`/`dňa`), Czech, Croatian/Serbian/Slovenian, Estonian, Lithuanian, Latvian, Ukrainian, Bulgarian, Greek, Indonesian/Malay, and Vietnamese time connectors; added Chinese `'点'`/`'點'` hour suffixes, direct Minguo (`民國`/`民国`) era parsing, and `"ja"` locale hint for Japanese-requested Han dates.
- **`search_dates` multi-connector bridging, Arabic punctuation & CJK multi-date extraction**: bridged 2-word connectors (`a las`, `la ora`), trimmed Arabic/CJK punctuation (`،`, `。`, `，`), and extracted CJK relative subspans (`"3日前"`, `"2天前"`) cleanly alongside absolute dates.
- **Strict language validation & `DateData.__module__`**: unknown codes in `languages`, `locales`, or `DEFAULT_LANGUAGES` now raise `ValueError: Unknown language(s): ...` consistently with `dateparser`, and `DateData` reports `module = "dateoxide.date"`.

## [0.6.0] - 2026-11-27

### Added
- **205 / 205 full-NLP language coverage**: upgraded all 205 CLDR locales so every single language now ships complete `months`, `weekdays`, `am`/`pm`, `units`, `ago`/`in_words`, `relative_exact`, `relative_affix` (both past & future), `word_numbers` (`1..10`), and `time_of_day` anchors (`205/205` across all 11 fields, including full `tl` Tagalog parity, all 84 remaining relative-affix locales, and all 132 remaining `word_numbers`/`time_of_day` locales via `scripts/locale_enrichment_205.py`).
- **28 / 28 Unicode script detection**: added Tibetan (`0x0F00..0x0FFF`, `bo`/`dz`), Oriya (`0x0B00..0x0B7F`, `or`), Cherokee (`0x13A0..0x13FF`, `chr`), Tifinagh (`0x2D30..0x2D7F`, `zgh`/`shi-tfng`), and Yi (`0xA000..0xA4CF`, `ii`), plus expanded Arabic/Cyrillic/Devanagari/Han/Hebrew script-variant hints.
- **Expanded `classify_relative_modifier`**: added `last`/`next`/`this` modifiers across 25+ additional European, Asian, and African languages.
- **Comprehensive 205-language full-NLP test suite (`tests/test_all_205_languages_nlp.py`)**: verifies absolute, exact relative, dynamic past & future relative, word-number relative, and time-of-day parsing for every single one of the 205 locales.

### Fixed
- **`RETURN_TIME_AS_PERIOD=True` on time-unit relatives**: `"2 hours ago"`, `"30 minutes ago"`, `"half an hour ago"`, `"через полчаса"`, `"半小时前"`, `"לפני שעתיים"`, and `"قبل ساعتين"` now set `has_time = true` and return `period = "time"` consistently with `dateparser`.
- **`STRICT_DATE_ORDER` invalid ISO month guard & ambiguous weekday/month disambiguation**: rejects out-of-range ISO months (`"2026-13-01"`) under `STRICT_DATE_ORDER="all"` / `"year"` while keeping default `YDM` fallback parity with `dateparser`, disambiguates overlapping weekday/month tokens (`"mar, 16 jun 2020"`, `"15 kad 2025 kas"`), picks longest matching `time_of_day` phrases safely across Unicode folding, and supports word numbers and `PREFER_DATES_FROM` directly inside `relative_affix` patterns.

## [0.5.0] - 2026-11-27

### Added
- **Regional locale date order (`en-GB`, `en-CA`, `en-AU`, etc.)**: extracts 2-letter region subtags from BCP-47 `locales` while ignoring 4-letter script subtags (`uz-Cyrl`, `zh-Hans`), honoring `PREFER_LOCALE_DATE_ORDER`.
- **Stateful `DateDataParser(try_previous_locales=True)` & `LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD`**: caches previously matched locales per parser instance to disambiguate subsequent numeric dates and validates/forwards confidence threshold to `detect_languages_function`.
- **Full & partial `date_formats` with 205-language localization**: `chrono::format::Parsed` fallback handles partial formats (`%B %Y`, `%Y-%m`, `%d %B`, `%H:%M`, `%Y-%j`) and translates localized month/weekday/AM-PM names (`%B`, `%b`, `%h`, `%A`, `%a`, `%p`) across all compiled locales.
- **Timestamps & `no-spaces-time` expansion**: added `PARSERS=["negative-timestamp"]`, 16-digit positive/negative microsecond timestamps, and 12/14-digit `no-spaces-time` (`YYYYMMDDHHMM[SS]`, `DDMMYYYYHHMM[SS]`).
- **JS `Date.toString()`, Unix `ctime` mid-string TZ & full world TZ dictionary**: strips parenthesized comments (`GMT+0300 (Turkey Time)`, `-0500 (EST)`), extracts pre-year `ctime` timezones (`Thu Nov 26 14:30:00 UTC 2026`), and expands abbreviations (`PKT`, `WIB`, `WITA`, `WIT`, `PHT`, `NPT`, `ACST`, `ACDT`, `CLT`, `COT`, `IRST`, `AFT`, `UZT`, `ALMT`, etc.).
- **Japanese Imperial Eras, apostrophe years, CJK/Korean clocks & idioms**:
  - Correct Gregorian year conversion for `令和`/`平成`/`昭和`/`大正`/`明治` and `元年` (`令和8年` → `2026`).
  - Apostrophe 2-digit years (`Nov 26 '26`, `15 Jan ’99`), European connectors (`il 15 gennaio 2026 alle 14:30`), `"just now"`, `"at 5:30 PM"`, `"tomorrow at 5"`, `"12 noon"`, `"12 midnight"`, `"next Monday morning"`.
  - Approximate prefixes (`~`, `≈`, `約`, `약 `, `about `, `yaklaşık `, `vor etwa `, `il y a environ `), spaceless multi-unit CJK/Korean relatives & clocks (`3日2時間前`, `昨日の14:30`, `어제 오후 2시 30분`, `2일 3시간 전`), Russian/CJK half-units (`через полчаса`, `半小时前`), and Arabic/Hebrew duals (`منذ يومين`, `לפני שעתיים`).
- **`search_dates` custom `date_formats` & precise CJK span trimming**: supports custom format matching (including `%Y_%m_%d` under `strategy="ngram"`) and cleanly extracts date subspans from spaceless CJK sentences (`"締め切りは2026年11月26日です。"` → `'2026年11月26日'`).
- **Drop-in helper modules**: added `dateoxide.custom_language_detection` (`langdetect`, `fasttext`, `language_mapping`), `dateoxide.timezone_parser` (`convert_to_local_tz`, `word_is_tz`, `is_timezone_token`, `local_tz_offset`), and `dateoxide.utils` (`strip_braces`, `normalize_unicode`, `find_date_separator`, `get_last_day_of_month`, leap year & timezone helpers).

### Fixed
- **Japanese Reiwa year parsing bug**: fixed `令和8年11月26日` misparsing as `2008` instead of `2026`.
- **Friulian `"ca di ..."` prefix safety**: kept approximation stripping (`ca.`) from colliding with Friulian future prefix `"ca di "`.

## [0.4.0] - 2026-11-27

### Added
- **73 Tier-1 full-featured languages** (up from 40): added word numbers and time-of-day anchors for 33 more languages (`sl`, `lt`, `lv`, `et`, `ka`, `hy`, `sq`, `mk`, `bs`, `be`, `is`, `ga`, `cy`, `eu`, `gl`, `af`, `sw`, `ta`, `te`, `ml`, `kn`, `mr`, `gu`, `pa`, `ne`, `fil`, `nn`, `my`, `km`, `si`, `mn`, `ky`, `am`).
- **All 18 Unicode decimal numeral systems** normalized to ASCII `0-9` (`Arabic-Indic`, `Persian`, `Devanagari`, `Bengali`, `Gurmukhi`, `Gujarati`, `Oriya`, `Tamil`, `Telugu`, `Kannada`, `Malayalam`, `Sinhala`, `Thai`, `Lao`, `Tibetan`, `Myanmar`, `Khmer`, `Fullwidth`) plus script hints across 23 Unicode scripts and dual-indexing for Unicode-digit month/weekday names (fixing Tibetan/Dzongkha).
- **`±2 days` relative idioms across 26 languages** (`evvelsi gün`, `öbür gün`, `anteayer`, `pasado mañana`, `dopodomani`, `przedwczoraj`, `pojutrze`, `그제`, `모레`, `أول أمس`, `بعد غد`, `پریروز`, `پس‌فردا`, `שלשום`, `מחרתיים`, `προχθές`, `μεθαύριο`, `alaltăieri`, `poimâine`, `tegnapelőtt`, `holnapután`, `i förrgår`, `toissapäivänä`, `ylihuomenna`, etc.).
- **Post-positional & CJK/Korean relative weekdays**: `"vendredi dernier"`, `"lundi prochain"`, `"viernes pasado"`, `"venerdì scorso"`, `"sexta-feira passada"`, `"vorige vrijdag"`, `"지난 금요일"`, `"다음 월요일"`, `"上周五"`, `"下周一"`, `"先週の金曜日"`.
- **Expanded multi-word `in_words` / `ago` prepositions** (`daqui a`, `dentro de`, `peste`, `دیگر`, `بعد`, `evvel`, `əvvəl`) with longest-marker-first stripping.
- **Extreme multilingual benchmark & tests**: `benchmarks/multilingual_torture.py` (`87/87` in `0.41–0.63 ms` vs `dateparser`'s `48/87` in `400.83 ms`) and `tests/test_multilingual_extreme.py` (`3,218` total tests passing).

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
