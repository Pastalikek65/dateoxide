//! High-performance, low-memory, zero-ReDoS multilingual natural-language date parser in pure Rust.
//!
//! `dateoxide-core` powers the Python package [`dateoxide`](https://pypi.org/project/dateoxide/)
//! across 205 languages, non-Gregorian calendars (official Saudi Umm al-Qura Hijri,
//! Jalali, Minguo, French Republican), time spans, and parallel batch parsing.
//!
//! # Quick Start
//!
//! ```
//! use dateoxide_core::{calendars, parse, parse_batch, parse_with_settings, search::search_dates, settings::Settings};
//! use chrono::{Datelike, NaiveDate, NaiveDateTime};
//!
//! // Fast ISO-8601, RFC-2822/3339, and ISO week date parsing
//! let dt = parse("2026-11-26").unwrap();
//! assert_eq!(dt.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 26).unwrap());
//!
//! let iso_week = parse("2026-W48-4").unwrap();
//! assert_eq!(iso_week.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 26).unwrap());
//!
//! // Multilingual natural-language & relative parsing with fixed RELATIVE_BASE
//! let mut settings = Settings::default();
//! settings.relative_base = Some(
//!     NaiveDateTime::parse_from_str("2026-11-26 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
//! );
//! let tr = parse_with_settings("3 gün önce", &settings).unwrap();
//! assert_eq!(tr.naive_local().day(), 23);
//!
//! // Free-form text extraction and parallel batch parsing
//! let found = search_dates("Meeting on 12 December 2025", &settings);
//! assert_eq!(found.len(), 1);
//!
//! let batch = parse_batch(&["2026-11-26", "vor 2 Wochen", "not a date"], &settings);
//! assert!(batch[0].is_some() && batch[1].is_some() && batch[2].is_none());
//!
//! // Non-Gregorian calendar conversion (official Umm al-Qura Hijri)
//! let hijri = calendars::parse_hijri("1446/09/01", &settings);
//! assert_eq!(hijri.date_obj.unwrap().naive_local().date(), NaiveDate::from_ymd_opt(2025, 3, 1).unwrap());
//! ```

pub mod absolute;
#[cfg(feature = "calendars")]
pub mod calendars;
pub mod fast_path;
pub mod locale;
pub mod relative;
pub mod search;
pub mod settings;
pub mod time_spans;
pub mod timezone;
#[cfg(feature = "calendars")]
pub mod umm_al_qura;

use locale::{normalize_digits_and_lower, LocaleRegistry};
#[cfg(feature = "rayon")]
use rayon::prelude::*;
use search::SearchMatch;
use settings::{DateData, ParsedDateTime, ParserKind, Period, Settings};
use timezone::{extract_trailing_timezone_with_abbrevs, finalize_datetime};

/// Parse a date string using default settings.
#[inline]
pub fn parse(input: &str) -> Option<ParsedDateTime> {
    parse_with_settings(input, &Settings::default())
}

/// Parse a date string using explicit `Settings`.
#[inline]
pub fn parse_with_settings(input: &str, settings: &Settings) -> Option<ParsedDateTime> {
    get_date_data(input, settings).date_obj
}

fn strip_user_skip_tokens(input: &str, skip_tokens: &[String]) -> String {
    let mut out = Vec::new();
    for tok in input.split_whitespace() {
        let clean = tok.trim_matches(|c: char| c == ',' || c == '.');
        if skip_tokens
            .iter()
            .any(|sk| sk.eq_ignore_ascii_case(tok) || sk.eq_ignore_ascii_case(clean))
        {
            continue;
        }
        out.push(tok);
    }
    out.join(" ")
}

#[inline]
fn apply_time_period(period: Period, has_time: bool, settings: &Settings) -> Period {
    if settings.return_time_as_period && has_time {
        Period::Time
    } else {
        period
    }
}

/// Relative-only parser backing `dateparser.freshness_date_parser.FreshnessDateDataParser`.
pub fn parse_freshness(input: &str, settings: &Settings) -> DateData {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return DateData::empty();
    }
    let (without_tz, tz) =
        extract_trailing_timezone_with_abbrevs(trimmed, &settings.timezone_abbreviations);
    let normalized = normalize_digits_and_lower(without_tz);
    if normalized.is_empty() {
        return DateData::empty();
    }
    let registry = LocaleRegistry::global();
    let tokens: Vec<&str> = normalized
        .split(|c: char| c.is_whitespace() || c == ',' || c == '/')
        .map(|t| t.trim_matches(|c: char| c == '.' || c == '(' || c == ')'))
        .filter(|t| !t.is_empty())
        .collect();
    let candidates = registry.detect_candidates(
        &normalized,
        &tokens,
        settings.languages.as_deref(),
        &settings.default_languages,
        settings.normalize,
    );
    if let Some(m) = relative::try_parse_relative(&normalized, &candidates, registry, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, tz, settings)),
            period: apply_time_period(m.period, m.has_time, settings),
            locale: Some(m.locale),
            parts: m.parts,
            part_of_day: m.part_of_day,
        };
    }
    DateData::empty()
}

/// Full `dateparser.date.DateDataParser.get_date_data` equivalent returning
/// `DateData { date_obj, period, locale, parts, part_of_day }`.
pub fn get_date_data(input: &str, settings: &Settings) -> DateData {
    let trimmed_raw = input.trim();
    if trimmed_raw.is_empty() {
        return DateData::empty();
    }

    let owned_cleaned;
    let trimmed: &str = if settings.skip_tokens.is_empty() {
        trimmed_raw
    } else {
        owned_cleaned = strip_user_skip_tokens(trimmed_raw, &settings.skip_tokens);
        if owned_cleaned.trim().is_empty() {
            return DateData::empty();
        }
        owned_cleaned.trim()
    };

    // 0. Custom user strptime formats (`date_formats`)
    if let Some(m) = fast_path::try_custom_formats(trimmed, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, m.tz, settings)),
            period: apply_time_period(m.period, m.has_time, settings),
            locale: Some("en".to_string()),
            parts: m.parts,
            part_of_day: None,
        };
    }

    // 1. Zero-regex, zero-allocation fast path
    if let Some(m) = fast_path::try_fast_path(trimmed, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, m.tz, settings)),
            period: apply_time_period(m.period, m.has_time, settings),
            locale: Some("en".to_string()),
            parts: m.parts,
            part_of_day: None,
        };
    }

    // 2. Peel off optional trailing timezone before natural-language normalization
    let (without_tz, tz) =
        extract_trailing_timezone_with_abbrevs(trimmed, &settings.timezone_abbreviations);
    let normalized = normalize_digits_and_lower(without_tz);
    if normalized.is_empty() {
        return DateData::empty();
    }

    // Re-check fast path if Unicode digits (e.g. Arabic-Indic / Persian) were normalized to ASCII
    if normalized.as_bytes()[0].is_ascii_digit() && normalized != without_tz {
        if let Some(m) = fast_path::try_fast_path(&normalized, settings) {
            return DateData {
                date_obj: Some(finalize_datetime(m.naive, m.tz.or(tz), settings)),
                period: apply_time_period(m.period, m.has_time, settings),
                locale: Some("en".to_string()),
                parts: m.parts,
                part_of_day: None,
            };
        }
    }

    // 3. Lazy-load the compact 205-language registry and detect top candidates in O(tokens)
    let registry = LocaleRegistry::global();
    let mut tokens: Vec<&str> = Vec::with_capacity(8);
    for chunk in normalized.split(|c: char| {
        c.is_whitespace() || c == ',' || c == '/' || c == '[' || c == ']' || c == '–' || c == '—'
    }) {
        let clean = chunk.trim_matches(|c: char| matches!(c, '.' | '(' | ')' | '[' | ']'));
        if clean.is_empty() {
            continue;
        }
        if registry.token_to_langs.contains_key(clean) {
            tokens.push(clean);
            continue;
        }
        for sub in clean.split(['-', '.']) {
            if sub.is_empty() {
                continue;
            }
            if sub.chars().any(|c| c.is_ascii_digit()) && sub.chars().any(|c| c.is_alphabetic()) {
                let mut start = 0usize;
                let mut prev_d = sub.chars().next().unwrap().is_ascii_digit();
                for (idx, ch) in sub.char_indices().skip(1) {
                    let cur_d = ch.is_ascii_digit();
                    if cur_d != prev_d {
                        tokens.push(&sub[start..idx]);
                        start = idx;
                        prev_d = cur_d;
                    }
                }
                if start < sub.len() {
                    tokens.push(&sub[start..]);
                }
            } else {
                tokens.push(sub);
            }
        }
    }

    let candidates = registry.detect_candidates(
        &normalized,
        &tokens,
        settings.languages.as_deref(),
        &settings.default_languages,
        settings.normalize,
    );

    if candidates.is_empty() {
        if settings.allows_parser(ParserKind::RelativeTime) {
            if let Some(m) = relative::try_conversational_clock_or_fraction(&normalized, settings) {
                return DateData {
                    date_obj: Some(finalize_datetime(m.naive, tz, settings)),
                    period: apply_time_period(m.period, m.has_time, settings),
                    locale: Some(m.locale),
                    parts: m.parts,
                    part_of_day: m.part_of_day,
                };
            }
        }
        let is_cjk_or_bare_year = (normalized.len() == 4
            && normalized.bytes().all(|b| b.is_ascii_digit()))
            || normalized.contains('年')
            || normalized.contains('月')
            || normalized.contains('日')
            || normalized.contains('년')
            || normalized.contains('월')
            || normalized.contains('일');
        if settings.allows_parser(ParserKind::AbsoluteTime) && is_cjk_or_bare_year {
            if let Some(m) = absolute::try_parse_absolute(&normalized, &[], registry, settings) {
                return DateData {
                    date_obj: Some(finalize_datetime(m.naive, tz, settings)),
                    period: apply_time_period(m.period, m.has_time, settings),
                    locale: Some(m.locale),
                    parts: m.parts,
                    part_of_day: m.part_of_day,
                };
            }
        }
        return ignore_surrounding_fallback(trimmed, settings);
    }

    // 4. Multilingual Relative Date Engine
    if settings.allows_parser(ParserKind::RelativeTime) {
        if let Some(m) = relative::try_parse_relative(&normalized, &candidates, registry, settings) {
            return DateData {
                date_obj: Some(finalize_datetime(m.naive, tz, settings)),
                period: apply_time_period(m.period, m.has_time, settings),
                locale: Some(m.locale),
                parts: m.parts,
                part_of_day: m.part_of_day,
            };
        }
    }

    // 5. Multilingual Absolute & Partial Date Engine
    if settings.allows_parser(ParserKind::AbsoluteTime) {
        if let Some(m) = absolute::try_parse_absolute(&normalized, &candidates, registry, settings) {
            return DateData {
                date_obj: Some(finalize_datetime(m.naive, tz, settings)),
                period: apply_time_period(m.period, m.has_time, settings),
                locale: Some(m.locale),
                parts: m.parts,
                part_of_day: m.part_of_day,
            };
        }
    }

    ignore_surrounding_fallback(trimmed, settings)
}

fn ignore_surrounding_fallback(trimmed: &str, settings: &Settings) -> DateData {
    if !settings.ignore_surrounding_text {
        return DateData::empty();
    }
    let mut inner = settings.clone();
    inner.ignore_surrounding_text = false;
    let found = search_dates_full(trimmed, &inner);
    if let Some(first) = found.into_iter().next() {
        return DateData {
            date_obj: Some(first.date_obj),
            period: first.period,
            locale: first.locale,
            parts: vec!["year", "month", "day"],
            part_of_day: None,
        };
    }
    DateData::empty()
}

/// Parallel batch parsing powered by Rayon across all available CPU cores
/// (falls back to sequential iteration when the `rayon` feature is disabled).
pub fn parse_batch<S: AsRef<str> + Sync>(
    inputs: &[S],
    settings: &Settings,
) -> Vec<Option<ParsedDateTime>> {
    #[cfg(feature = "rayon")]
    {
        if inputs.len() < 256 {
            return inputs
                .iter()
                .map(|s| parse_with_settings(s.as_ref(), settings))
                .collect();
        }
        return inputs
            .par_iter()
            .map(|s| parse_with_settings(s.as_ref(), settings))
            .collect();
    }
    #[cfg(not(feature = "rayon"))]
    {
        inputs
            .iter()
            .map(|s| parse_with_settings(s.as_ref(), settings))
            .collect()
    }
}

/// Find and parse all date expressions embedded in free-form text.
pub fn search_dates(text: &str, settings: &Settings) -> Vec<(String, ParsedDateTime)> {
    search::search_dates(text, settings)
}

/// Find and parse all date expressions embedded in free-form text, including detected locale code.
pub fn search_dates_with_locale(
    text: &str,
    settings: &Settings,
) -> Vec<(String, ParsedDateTime, Option<String>)> {
    search::search_dates_with_locale(text, settings)
}

/// Find and parse all date expressions returning full match metadata.
pub fn search_dates_full(text: &str, settings: &Settings) -> Vec<SearchMatch> {
    search::search_dates_full(text, settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime, Timelike};
    use settings::{DateOrder, PartOfDay, PreferDatesFrom, PreferDayOfMonth, StrictDateOrder};

    fn fixed_settings() -> Settings {
        Settings {
            relative_base: Some(
                NaiveDate::from_ymd_opt(2026, 11, 26)
                    .unwrap()
                    .and_hms_opt(15, 0, 0)
                    .unwrap(),
            ),
            ..Default::default()
        }
    }

    #[test]
    fn test_fast_path_and_strict_date_order() {
        let mut s = fixed_settings();
        s.date_order = Some(DateOrder::MDY);
        s.strict_date_order = StrictDateOrder::None;
        assert_eq!(
            parse_with_settings("26/11/2026", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );

        s.strict_date_order = StrictDateOrder::All;
        assert!(parse_with_settings("26/11/2026", &s).is_none());
        assert_eq!(
            parse_with_settings("2026/11/26", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );
    }

    #[cfg(feature = "calendars")]
    #[test]
    fn test_calendars_conversions() {
        let s = fixed_settings();
        assert_eq!(
            calendars::jalali_to_gregorian(1395, 1, 1),
            Some(NaiveDate::from_ymd_opt(2016, 3, 20).unwrap())
        );
        assert_eq!(
            calendars::jalali_to_gregorian(1400, 1, 1),
            Some(NaiveDate::from_ymd_opt(2021, 3, 21).unwrap())
        );
        assert_eq!(
            calendars::jalali_to_gregorian(1404, 9, 5),
            Some(NaiveDate::from_ymd_opt(2025, 11, 26).unwrap())
        );
        assert_eq!(
            calendars::parse_jalali("۵ آذر ۱۴۰۴", &s).date_obj.unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2025, 11, 26).unwrap()
        );

        assert_eq!(
            calendars::hijri_to_gregorian(1445, 1, 1),
            Some(NaiveDate::from_ymd_opt(2023, 7, 19).unwrap())
        );
        assert_eq!(
            calendars::hijri_to_gregorian(1446, 9, 1),
            Some(NaiveDate::from_ymd_opt(2025, 3, 1).unwrap())
        );

        assert_eq!(
            calendars::parse_minguo("115年11月26日", &s).date_obj.unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );

        assert_eq!(
            calendars::french_republican_to_gregorian(1, 1, 1),
            Some(NaiveDate::from_ymd_opt(1792, 9, 22).unwrap())
        );
        assert_eq!(
            calendars::parse_french_republican("14 Germinal an III", &s)
                .date_obj
                .unwrap()
                .naive_local()
                .date(),
            NaiveDate::from_ymd_opt(1795, 4, 3).unwrap()
        );
    }

    #[test]
    fn test_parts_of_day_bare_durations_and_surrounding_text() {
        let mut s = fixed_settings();
        s.parts_of_day.morning = NaiveTime::from_hms_opt(8, 30, 0).unwrap();
        let dd = get_date_data("yesterday morning", &s);
        assert_eq!(
            dd.date_obj.unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 25)
                .unwrap()
                .and_hms_opt(8, 30, 0)
                .unwrap()
        );
        assert_eq!(dd.part_of_day, Some(PartOfDay::Morning));
        assert_eq!(dd.parts, vec!["year", "month", "day", "time"]);

        // Bare duration
        let d_bare = parse_with_settings("2 days", &fixed_settings()).unwrap();
        assert_eq!(d_bare.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 24).unwrap());

        // Week period
        let dd_week = get_date_data("last week", &fixed_settings());
        assert_eq!(dd_week.period, Period::Week);

        // Ignore surrounding text
        let mut s_ign = fixed_settings();
        s_ign.ignore_surrounding_text = true;
        assert_eq!(
            parse_with_settings("Published: 12 December 2025", &s_ign)
                .unwrap()
                .naive_local()
                .date(),
            NaiveDate::from_ymd_opt(2025, 12, 12).unwrap()
        );
    }

    #[test]
    fn test_time_spans_and_custom_tz_abbrevs() {
        let mut s = fixed_settings();
        s.return_time_span = true;
        let span_matches = search_dates_full("Show logs from last week please", &s);
        assert_eq!(span_matches.len(), 2);

        let mut s_tz = fixed_settings();
        s_tz.timezone_abbreviations
            .insert("FOO".to_string(), "+04:30".to_string());
        s_tz.return_as_timezone_aware = Some(true);
        let dt = parse_with_settings("2026-11-26 10:00 FOO", &s_tz).unwrap();
        assert!(dt.is_aware());
        assert_eq!(dt.naive_local().hour(), 10);
    }

    #[test]
    fn test_timezone_conversion_and_preferences() {
        let mut s = fixed_settings();
        s.to_timezone = Some("Europe/Istanbul".to_string());
        s.return_as_timezone_aware = Some(true);
        let dt = parse_with_settings("2026-11-26T12:00:00Z", &s).unwrap();
        assert_eq!(
            dt.naive_local(),
            NaiveDateTime::parse_from_str("2026-11-26 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );

        s.prefer_day_of_month = PreferDayOfMonth::First;
        assert_eq!(
            parse_with_settings("December 2025", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2025, 12, 1).unwrap()
        );
        s.prefer_dates_from = PreferDatesFrom::Past;
        assert_eq!(
            parse_with_settings("December 15", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2025, 12, 15).unwrap()
        );
    }

    #[test]
    fn test_iso_week_dates_and_year_zero_guard() {
        let s = fixed_settings();
        assert_eq!(
            parse_with_settings("2026-W48-4", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );
        assert_eq!(
            parse_with_settings("2026-W48", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()
        );
        assert_eq!(
            parse_with_settings("2026W484", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );
        assert!(parse_with_settings("2026-W54-1", &s).is_none());
        assert!(parse_with_settings("0000-01-01", &s).is_none());

        let mut s_strict = fixed_settings();
        s_strict.strict_parsing = true;
        assert!(parse_with_settings("2026-W48", &s_strict).is_none());
    }

    #[test]
    fn test_no_spaces_time_and_time_only_preferences() {
        let mut s_ns = fixed_settings();
        s_ns.parsers = Some(vec![ParserKind::NoSpacesTime]);
        assert_eq!(
            parse_with_settings("1430", &s_ns).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 26)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap()
        );
        assert_eq!(
            parse_with_settings("143025", &s_ns).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 26)
                .unwrap()
                .and_hms_opt(14, 30, 25)
                .unwrap()
        );
        s_ns.date_order = Some(DateOrder::DMY);
        assert_eq!(
            parse_with_settings("26112026", &s_ns).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );
        s_ns.date_order = Some(DateOrder::MDY);
        assert_eq!(
            parse_with_settings("11262026", &s_ns).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );

        let mut s_fut = fixed_settings();
        s_fut.prefer_dates_from = PreferDatesFrom::Future;
        assert_eq!(
            parse_with_settings("09:00", &s_fut).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 27)
                .unwrap()
                .and_hms_opt(9, 0, 0)
                .unwrap()
        );

        let mut s_past = fixed_settings();
        s_past.prefer_dates_from = PreferDatesFrom::Past;
        assert_eq!(
            parse_with_settings("18:00", &s_past).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 25)
                .unwrap()
                .and_hms_opt(18, 0, 0)
                .unwrap()
        );
    }

    #[test]
    fn test_search_dates_false_positive_guards() {
        let s = fixed_settings();
        let found = search_dates_full("Let us meet tomorrow at 5pm or next Friday.", &s);
        assert!(!found.iter().any(|m| m.text == "Let"));
        assert_eq!(found.len(), 2);

        assert!(search_dates_full("She can see her man, did you say?", &s).is_empty());
        assert!(search_dates_full("Python 3.12.1 on 192.168.1.1", &s).is_empty());
    }
}
