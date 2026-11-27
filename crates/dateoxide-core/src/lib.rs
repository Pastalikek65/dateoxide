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
pub mod numerals;
pub mod ranges;
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
        settings.use_given_language_order,
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

#[inline]
fn resolve_fast_path_locale(match_locale: Option<String>, settings: &Settings) -> String {
    if let Some(loc) = match_locale {
        return loc;
    }
    if let Some(first) = settings.languages.as_deref().and_then(|ls| ls.first()) {
        let reg = LocaleRegistry::global();
        if let Some(id) = reg.resolve_lang_id(first) {
            return reg.locales[id].code.clone();
        }
    }
    "en".to_string()
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn build_datedata(
    naive: chrono::NaiveDateTime,
    tz: Option<timezone::TzSpec>,
    period: Period,
    has_time: bool,
    locale: String,
    parts: Vec<&'static str>,
    part_of_day: Option<settings::PartOfDay>,
    settings: &Settings,
) -> DateData {
    let dt = finalize_datetime(naive, tz, settings);
    if !timezone::is_valid_year_range(&dt) {
        return DateData::empty();
    }
    DateData {
        date_obj: Some(dt),
        period: apply_time_period(period, has_time, settings),
        locale: Some(locale),
        parts,
        part_of_day,
    }
}

/// Full `dateparser.date.DateDataParser.get_date_data` equivalent returning
/// `DateData { date_obj, period, locale, parts, part_of_day }`.
pub fn get_date_data(input: &str, settings: &Settings) -> DateData {
    let trimmed_raw = input.trim();
    if trimmed_raw.is_empty() || trimmed_raw.contains('\0') {
        return DateData::empty();
    }

    let owned_cleaned;
    let skip_trimmed: &str = if settings.skip_tokens.is_empty() {
        trimmed_raw
    } else {
        owned_cleaned = strip_user_skip_tokens(trimmed_raw, &settings.skip_tokens);
        if owned_cleaned.trim().is_empty() {
            return DateData::empty();
        }
        owned_cleaned.trim()
    };

    let unicode_cleaned = locale::clean_unicode_surface(skip_trimmed);
    let trimmed = unicode_cleaned.as_ref();
    if trimmed.is_empty() {
        return DateData::empty();
    }

    // Currency-amount guard in auto-detect mode (e.g. "25 EUR", "100 SEK", "50 NGN")
    if settings.languages.is_none() {
        let mut parts = trimmed.split_whitespace();
        if let (Some(n_tok), Some(c_tok), None) = (parts.next(), parts.next(), parts.next()) {
            let n_clean = n_tok.trim_matches(|c: char| matches!(c, '+' | '-'));
            if !n_clean.is_empty()
                && n_clean.bytes().all(|b| b.is_ascii_digit() || b == b'.' || b == b',')
                && locale::is_iso4217_currency(c_tok)
            {
                return DateData::empty();
            }
        }
    }

    // 0. Custom user strptime formats (`date_formats`)
    if let Some(m) = fast_path::try_custom_formats(trimmed, settings) {
        return build_datedata(
            m.naive,
            m.tz,
            m.period,
            m.has_time,
            resolve_fast_path_locale(m.locale, settings),
            m.parts,
            None,
            settings,
        );
    }

    // 1. Zero-regex, zero-allocation fast path
    if let Some(m) = fast_path::try_fast_path(trimmed, settings) {
        return build_datedata(
            m.naive,
            m.tz,
            m.period,
            m.has_time,
            resolve_fast_path_locale(m.locale, settings),
            m.parts,
            None,
            settings,
        );
    }

    // 2. Peel off optional trailing timezone (or Unix ctime mid-string TZ) before normalization
    let (without_tz_initial, mut tz) =
        extract_trailing_timezone_with_abbrevs(trimmed, &settings.timezone_abbreviations);
    let owned_embedded_tz;
    let without_tz: &str = if tz.is_none() {
        if let Some((cleaned, etz)) = timezone::extract_embedded_timezone_before_year(
            without_tz_initial,
            &settings.timezone_abbreviations,
        ) {
            tz = Some(etz);
            owned_embedded_tz = cleaned;
            &owned_embedded_tz
        } else {
            without_tz_initial
        }
    } else {
        without_tz_initial
    };
    let norm_nd = normalize_digits_and_lower(without_tz);
    if norm_nd.is_empty() {
        return DateData::empty();
    }
    let norm_script = numerals::normalize_non_decimal_numerals(&norm_nd);
    let normalized = norm_script.as_ref();

    // Re-check fast path if Unicode digits (e.g. Arabic-Indic / Persian / all 68 Nd scripts) were normalized to ASCII
    if normalized.as_bytes()[0].is_ascii_digit() && normalized != without_tz {
        if let Some(m) = fast_path::try_fast_path(normalized, settings) {
            return build_datedata(
                m.naive,
                m.tz.or(tz),
                m.period,
                m.has_time,
                resolve_fast_path_locale(m.locale, settings),
                m.parts,
                None,
                settings,
            );
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
        normalized,
        &tokens,
        settings.languages.as_deref(),
        &settings.default_languages,
        settings.normalize,
        settings.use_given_language_order,
    );

    if candidates.is_empty() {
        if settings.allows_parser(ParserKind::RelativeTime) {
            if let Some(m) = relative::try_conversational_clock_or_fraction(normalized, settings) {
                return build_datedata(
                    m.naive,
                    tz,
                    m.period,
                    m.has_time,
                    m.locale,
                    m.parts,
                    m.part_of_day,
                    settings,
                );
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
        let has_roman_month_cand = trimmed.split_whitespace().count() >= 2
            && trimmed
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|t| numerals::parse_roman_numeral(t).is_some())
            && normalized.bytes().any(|b| b.is_ascii_digit());
        if settings.allows_parser(ParserKind::AbsoluteTime) && (is_cjk_or_bare_year || has_roman_month_cand) {
            let fallback_cands: Vec<usize> = if has_roman_month_cand {
                registry.resolve_lang_id("en").into_iter().collect()
            } else {
                Vec::new()
            };
            if let Some(m) = absolute::try_parse_absolute_with_raw(
                normalized,
                Some(trimmed),
                &fallback_cands,
                registry,
                settings,
            ) {
                return build_datedata(
                    m.naive,
                    tz,
                    m.period,
                    m.has_time,
                    m.locale,
                    m.parts,
                    m.part_of_day,
                    settings,
                );
            }
        }
        return ignore_surrounding_fallback(trimmed, settings);
    }

    // 4. Multilingual Relative Date Engine
    if settings.allows_parser(ParserKind::RelativeTime) {
        match relative::try_parse_relative_outcome(normalized, &candidates, registry, settings) {
            relative::RelativeOutcome::Matched(m) => {
                return build_datedata(
                    m.naive,
                    tz,
                    m.period,
                    m.has_time,
                    m.locale,
                    m.parts,
                    m.part_of_day,
                    settings,
                );
            }
            relative::RelativeOutcome::OutOfBounds => return DateData::empty(),
            relative::RelativeOutcome::NoMatch => {}
        }
    }

    // 5. Multilingual Absolute & Partial Date Engine
    if settings.allows_parser(ParserKind::AbsoluteTime) {
        if let Some(m) =
            absolute::try_parse_absolute_with_raw(normalized, Some(trimmed), &candidates, registry, settings)
        {
            return build_datedata(
                m.naive,
                tz,
                m.period,
                m.has_time,
                m.locale,
                m.parts,
                m.part_of_day,
                settings,
            );
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
            parts: first.parts,
            part_of_day: first.part_of_day,
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
        inputs
            .par_iter()
            .map(|s| parse_with_settings(s.as_ref(), settings))
            .collect()
    }
    #[cfg(not(feature = "rayon"))]
    {
        inputs
            .iter()
            .map(|s| parse_with_settings(s.as_ref(), settings))
            .collect()
    }
}

/// Nullable parallel batch parsing (`Option<String>` inputs skip `None` cleanly).
pub fn parse_batch_opt<S: AsRef<str> + Sync>(
    inputs: &[Option<S>],
    settings: &Settings,
) -> Vec<Option<ParsedDateTime>> {
    #[cfg(feature = "rayon")]
    {
        if inputs.len() < 256 {
            return inputs
                .iter()
                .map(|opt| opt.as_ref().and_then(|s| parse_with_settings(s.as_ref(), settings)))
                .collect();
        }
        inputs
            .par_iter()
            .map(|opt| opt.as_ref().and_then(|s| parse_with_settings(s.as_ref(), settings)))
            .collect()
    }
    #[cfg(not(feature = "rayon"))]
    {
        inputs
            .iter()
            .map(|opt| opt.as_ref().and_then(|s| parse_with_settings(s.as_ref(), settings)))
            .collect()
    }
}

/// Parse natural-language or numeric date/time ranges into `(start, end)`.
pub fn parse_date_range(
    input: &str,
    settings: &Settings,
) -> Option<(ParsedDateTime, ParsedDateTime)> {
    ranges::parse_date_range(input, settings)
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
    use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
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

    #[test]
    fn test_v050_locale_region_date_order() {
        let mut s_gb = fixed_settings();
        s_gb.languages = Some(vec!["en-GB".to_string()]);
        assert_eq!(
            parse_with_settings("05/06/2026", &s_gb).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 6, 5).unwrap()
        );

        let mut s_us = fixed_settings();
        s_us.languages = Some(vec!["en-US".to_string()]);
        assert_eq!(
            parse_with_settings("05/06/2026", &s_us).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 5, 6).unwrap()
        );

        let mut s_tr = fixed_settings();
        s_tr.languages = Some(vec!["tr".to_string()]);
        assert_eq!(
            parse_with_settings("05/06/2026", &s_tr).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 6, 5).unwrap()
        );

        // Script subtag like "uz-Cyrl" must NOT be mistaken for a 2-letter region
        assert_eq!(settings::extract_region_from_locale("uz-Cyrl"), None);
        assert_eq!(settings::extract_region_from_locale("en-GB"), Some("GB"));
        assert_eq!(settings::extract_region_from_locale("zh-Hans-TW"), Some("TW"));

        // PREFER_LOCALE_DATE_ORDER = false falls back to MDY
        let mut s_no_pref = fixed_settings();
        s_no_pref.languages = Some(vec!["en-GB".to_string()]);
        s_no_pref.prefer_locale_date_order = false;
        assert_eq!(
            parse_with_settings("05/06/2026", &s_no_pref).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 5, 6).unwrap()
        );
    }

    #[test]
    fn test_v050_apostrophe_year_japanese_eras_and_european_connectors() {
        let s = fixed_settings();

        // Apostrophe 2-digit years
        assert_eq!(
            parse_with_settings("Nov 26 '26", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );
        assert_eq!(
            parse_with_settings("15 Jan ’99", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(1999, 1, 15).unwrap()
        );
        let nov_26 = get_date_data("Nov '26", &s);
        assert_eq!(nov_26.period, Period::Month);
        assert_eq!(nov_26.date_obj.as_ref().unwrap().naive_local().year(), 2026);
        assert_eq!(nov_26.date_obj.as_ref().unwrap().naive_local().month(), 11);

        // Japanese Imperial Eras
        let reiwa = get_date_data("令和8年11月26日", &s);
        assert_eq!(reiwa.locale.as_deref(), Some("ja"));
        assert_eq!(
            reiwa.date_obj.unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );
        assert_eq!(
            parse_with_settings("令和元年5月1日", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2019, 5, 1).unwrap()
        );
        assert_eq!(
            parse_with_settings("平成31年4月30日", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2019, 4, 30).unwrap()
        );
        assert_eq!(
            parse_with_settings("昭和64年1月7日", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(1989, 1, 7).unwrap()
        );
        assert_eq!(
            parse_with_settings("大正15年12月25日", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(1926, 12, 25).unwrap()
        );
        assert_eq!(
            parse_with_settings("明治45年7月30日", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(1912, 7, 30).unwrap()
        );
        assert!(parse_with_settings("令和0年11月26日", &s).is_none());

        // Italian & Spanish prepositions/connectors
        let it_dt = get_date_data("il 15 gennaio 2026 alle 14:30", &s);
        assert_eq!(it_dt.locale.as_deref(), Some("it"));
        assert_eq!(
            it_dt.date_obj.unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 1, 15)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap()
        );
        assert_eq!(
            parse_with_settings("el 15 de enero de 2026 a las 14:30", &s)
                .unwrap()
                .naive_local(),
            NaiveDate::from_ymd_opt(2026, 1, 15)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap()
        );
    }

    #[test]
    fn test_v050_negative_timestamp_micros_and_localized_date_formats() {
        let s = fixed_settings();
        // Disabled by default
        assert!(parse_with_settings("-86400", &s).is_none());

        // Enabled explicitly via PARSERS=[NegativeTimestamp]
        let mut s_neg = fixed_settings();
        s_neg.parsers = Some(vec![ParserKind::NegativeTimestamp]);
        assert_eq!(
            parse_with_settings("-86400", &s_neg).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(1969, 12, 31)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        );
        assert_eq!(
            parse_with_settings("-86400.5", &s_neg).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("1969-12-30 23:59:59.500", "%Y-%m-%d %H:%M:%S%.3f")
                .unwrap()
        );
        assert!(parse_with_settings("-2026", &s_neg).is_none());

        // 16-digit positive microsecond timestamp
        assert_eq!(
            parse_with_settings("1700000000123456", &s)
                .unwrap()
                .naive_local(),
            DateTime::from_timestamp_micros(1700000000123456)
                .unwrap()
                .naive_utc()
        );

        // Localized strict custom date_formats (%B in Turkish & French)
        let mut s_fmt = fixed_settings();
        s_fmt.parsers = Some(vec![ParserKind::CustomFormats]);
        s_fmt.date_formats = Some(vec!["%d %B %Y".to_string()]);
        let tr_m = get_date_data("15 Ocak 2026", &s_fmt);
        assert_eq!(tr_m.locale.as_deref(), Some("tr"));
        assert_eq!(
            tr_m.date_obj.unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()
        );
        let fr_m = get_date_data("15 janvier 2026", &s_fmt);
        assert_eq!(fr_m.locale.as_deref(), Some("fr"));
        assert_eq!(
            fr_m.date_obj.unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 1, 15).unwrap()
        );
        assert!(parse_with_settings("15-01-2026", &s_fmt).is_none());
    }

    #[test]
    fn test_v050_js_ctime_tz_world_abbrevs_and_parenthetical_guard() {
        let s = fixed_settings();

        // JavaScript Date.toString() formats with parenthesized TZ comments
        let js1 = parse_with_settings("Thu Nov 26 2026 14:30:00 GMT+0300 (GMT+03:00)", &s).unwrap();
        assert!(js1.is_aware());
        assert_eq!(
            js1.naive_local(),
            NaiveDateTime::parse_from_str("2026-11-26 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        let js2 = parse_with_settings("Thu Nov 26 2026 14:30:00 GMT+0300 (Turkey Time)", &s).unwrap();
        assert!(js2.is_aware());
        assert_eq!(
            js2.naive_local(),
            NaiveDateTime::parse_from_str("2026-11-26 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        // Guard against arbitrary parenthesized text
        assert!(parse_with_settings("Meeting (urgent)", &s).is_none());

        // Unix ctime mid-string timezone before trailing 4-digit year
        let ct1 = parse_with_settings("Thu Nov 26 14:30:00 UTC 2026", &s).unwrap();
        assert!(ct1.is_aware());
        assert_eq!(
            ct1.naive_local(),
            NaiveDateTime::parse_from_str("2026-11-26 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        let ct2 = parse_with_settings("Mon Jan 15 09:00:00 CEST 2026", &s).unwrap();
        assert!(ct2.is_aware());
        assert_eq!(
            ct2.naive_local(),
            NaiveDateTime::parse_from_str("2026-01-15 09:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );

        // Extended world timezone abbreviations + IST/BST backward-compat preserved
        for tz_str in [
            "15 Jan 2026 14:30 PKT",
            "15 Jan 2026 14:30 WIB",
            "15 Jan 2026 14:30 WITA",
            "15 Jan 2026 14:30 WIT",
            "15 Jan 2026 14:30 PHT",
            "15 Jan 2026 14:30 NPT",
            "15 Jan 2026 14:30 ACST",
            "15 Jan 2026 14:30 ACDT",
            "15 Jan 2026 14:30 CLT",
            "15 Jan 2026 14:30 COT",
            "15 Jan 2026 14:30 IRST",
            "15 Jan 2026 14:30 AFT",
            "15 Jan 2026 14:30 UZT",
            "15 Jan 2026 14:30 ALMT",
        ] {
            let parsed = parse_with_settings(tz_str, &s).unwrap();
            assert!(parsed.is_aware(), "expected aware for {}", tz_str);
            assert_eq!(parsed.naive_local().hour(), 14);
        }
    }

    #[test]
    fn test_v050_no_spaces_12_14_prefixed_time_and_cjk_korean_relatives() {
        let s = fixed_settings();

        // 12/14-digit no-spaces-time gated behind PARSERS=[NoSpacesTime]
        assert!(parse_with_settings("202601151430", &s).is_none());
        let mut s_ns = fixed_settings();
        s_ns.parsers = Some(vec![ParserKind::NoSpacesTime]);
        assert_eq!(
            parse_with_settings("202601151430", &s_ns).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-01-15 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("20260115143045", &s_ns).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-01-15 14:30:45", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        s_ns.date_order = Some(DateOrder::DMY);
        assert_eq!(
            parse_with_settings("150120261430", &s_ns).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-01-15 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );

        // "just now", "at 5:30 PM", "2026-11-26 t 14:30", "tomorrow at 5"
        assert_eq!(
            parse_with_settings("just now", &s).unwrap().naive_local(),
            s.relative_base.unwrap()
        );
        assert_eq!(
            parse_with_settings("at 5:30 PM", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-26 17:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("2026-11-26 t 14:30", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-26 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("tomorrow at 5", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-27 05:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );

        // Multi-unit attached & CJK/Korean relative+time
        assert_eq!(
            parse_with_settings("3日2時間前", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-23 13:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("昨日の14:30", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-25 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("昨日の午後2時30分", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-25 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("어제 오후 2시 30분", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-25 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("2일 3시간 전", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-24 12:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("1h30m ago", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-26 13:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
    }

    #[test]
    fn test_v050_partial_formats_search_cjk_and_remaining_idioms() {
        let s = fixed_settings();

        // Partial date_formats
        let mut s_fmt = fixed_settings();
        s_fmt.parsers = Some(vec![ParserKind::CustomFormats]);
        for (inp, fmt, exp) in [
            ("November 2026", "%B %Y", "2026-11-26 00:00:00"),
            ("2026-11", "%Y-%m", "2026-11-26 00:00:00"),
            ("15 Ocak", "%d %B", "2026-01-15 00:00:00"),
            ("14:30", "%H:%M", "2026-11-26 14:30:00"),
            ("2026-330", "%Y-%j", "2026-11-26 00:00:00"),
        ] {
            s_fmt.date_formats = Some(vec![fmt.to_string()]);
            assert_eq!(
                parse_with_settings(inp, &s_fmt).unwrap().naive_local(),
                NaiveDateTime::parse_from_str(exp, "%Y-%m-%d %H:%M:%S").unwrap(),
                "failed partial custom format {} with {}",
                inp,
                fmt
            );
        }

        // search_dates custom date_formats + precise CJK spans
        let mut s_search_fmt = fixed_settings();
        s_search_fmt.date_formats = Some(vec!["%Y_%m_%d".to_string()]);
        let custom_found = search_dates_full("Release 2026_11_26 is live now.", &s_search_fmt);
        assert_eq!(custom_found.len(), 2);
        assert_eq!(custom_found[0].text, "2026_11_26");
        assert_eq!(custom_found[1].text, "now");

        let ja_found = search_dates_full("締め切りは2026年11月26日です。", &s);
        assert_eq!(ja_found.len(), 1);
        assert_eq!(ja_found[0].text, "2026年11月26日");

        let zh_found = search_dates_full("会议定于2026年11月26日举行。", &s);
        assert_eq!(zh_found.len(), 1);
        assert_eq!(zh_found[0].text, "2026年11月26日");

        let multi_cjk = search_dates_full("2026年11月20日から2026年11月26日まで", &s);
        assert_eq!(multi_cjk.len(), 2);
        assert_eq!(multi_cjk[0].text, "2026年11月20日");
        assert_eq!(multi_cjk[1].text, "2026年11月26日");

        // Remaining 9 multilingual idioms + approx markers
        for (inp, exp) in [
            ("~3 days ago", "2026-11-23 15:00:00"),
            ("about 2 hours ago", "2026-11-26 13:00:00"),
            ("yaklaşık 2 saat önce", "2026-11-26 13:00:00"),
            ("約2時間前", "2026-11-26 13:00:00"),
            ("12 noon", "2026-11-26 12:00:00"),
            ("12 midnight", "2026-11-26 00:00:00"),
            ("next Monday morning", "2026-11-30 09:00:00"),
            ("через полчаса", "2026-11-26 15:30:00"),
            ("半小时前", "2026-11-26 14:30:00"),
            ("منذ يومين", "2026-11-24 15:00:00"),
            ("לפני שעתיים", "2026-11-26 13:00:00"),
        ] {
            assert_eq!(
                parse_with_settings(inp, &s).unwrap().naive_local(),
                NaiveDateTime::parse_from_str(exp, "%Y-%m-%d %H:%M:%S").unwrap(),
                "failed idiom {}",
                inp
            );
        }
    }

    #[test]
    fn test_v060_time_period_iso_month_guard_and_205_scripts() {
        let mut s_tp = fixed_settings();
        s_tp.return_time_as_period = true;
        for expr in ["2 hours ago", "30 minutes ago", "half an hour ago", "через полчаса"] {
            let dd = get_date_data(expr, &s_tp);
            assert!(dd.date_obj.is_some(), "failed {}", expr);
            assert_eq!(dd.period, Period::Time, "expected Period::Time for {}", expr);
        }

        // Invalid month "2026-13-01" falls back to YDM under default None, returns None under StrictDateOrder::All
        let mut s_strict = fixed_settings();
        s_strict.strict_date_order = StrictDateOrder::All;
        assert!(parse_with_settings("2026-13-01", &s_strict).is_none());
        let s = fixed_settings();
        assert_eq!(
            parse_with_settings("2026-13-01", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 1, 13).unwrap()
        );
        let mut s_ydm = fixed_settings();
        s_ydm.date_order = Some(DateOrder::YDM);
        assert_eq!(
            parse_with_settings("2026-13-01", &s_ydm).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 1, 13).unwrap()
        );

        // Newly enriched scripts & languages (bo, or, chr, zgh, ii, tl, eo, yo, haw, qu)
        for (lang, expr) in [
            ("bo", "གཉིས ཉིན སྔོན་ལ"),
            ("or", "ସକାଳ"),
            ("chr", "ᏌᎾᎴ"),
            ("zgh", "ⵜⵉⴼⴰⵡⵜ"),
            ("ii", "ꎮꂷ"),
            ("tl", "kahapon"),
            ("eo", "antaŭ 3 jaroj"),
            ("yo", "àárọ̀"),
            ("haw", "3 lā i hala"),
            ("qu", "ñawpaq 3 wata"),
        ] {
            let mut s_l = fixed_settings();
            s_l.languages = Some(vec![lang.to_string()]);
            assert!(
                parse_with_settings(expr, &s_l).is_some(),
                "failed {} for {}",
                expr,
                lang
            );
        }
    }

    #[test]
    fn test_v061_parity_fixes() {
        // 1. %f microseconds in date_formats
        let mut s_fmt = fixed_settings();
        s_fmt.date_formats = Some(vec!["%Y-%m-%d %H:%M:%S.%f".to_string()]);
        let dt_us = parse_with_settings("2024-05-15 14:30:00.123456", &s_fmt)
            .unwrap()
            .naive_local();
        assert_eq!(dt_us.and_utc().timestamp_subsec_micros(), 123456);
        let mut s_custom_only = s_fmt.clone();
        s_custom_only.parsers = Some(vec![ParserKind::CustomFormats]);
        assert!(parse_with_settings("2024-05-15 14:30:00", &s_custom_only).is_none());

        // 2. STRICT_PARSING=True allows relative dates while rejecting incomplete absolutes
        let mut s_strict = fixed_settings();
        s_strict.strict_parsing = true;
        assert_eq!(
            parse_with_settings("yesterday", &s_strict).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2026-11-25 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert!(parse_with_settings("May 2024", &s_strict).is_none());

        // 3. Attached short hour relatives (in 3h, il y a 3h, dans 3h)
        let s = fixed_settings();
        for (inp, exp) in [
            ("in 3h", "2026-11-26 18:00:00"),
            ("il y a 3h", "2026-11-26 12:00:00"),
            ("dans 3h", "2026-11-26 18:00:00"),
        ] {
            assert_eq!(
                parse_with_settings(inp, &s).unwrap().naive_local(),
                NaiveDateTime::parse_from_str(exp, "%Y-%m-%d %H:%M:%S").unwrap(),
                "failed {}",
                inp
            );
        }

        // 4. ro / sk connectors & Minguo direct era
        assert_eq!(
            parse_with_settings("15 mai 2024 la ora 15:30", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2024-05-15 15:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("15. mája 2024 o 15:30", &s).unwrap().naive_local(),
            NaiveDateTime::parse_from_str("2024-05-15 15:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
        assert_eq!(
            parse_with_settings("民國113年5月15日", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2024, 5, 15).unwrap()
        );

        // 5. search_dates CJK multi-date & Arabic/Spanish connectors
        let ja_hits = crate::search::search_dates("会議は2024年5月15日14:30からです。3日前に通知済み。", &s);
        assert_eq!(ja_hits.len(), 2);
        assert_eq!(ja_hits[0].0, "2024年5月15日14:30");
        assert_eq!(ja_hits[1].0, "3日前");

        let zh_hits = crate::search::search_dates("会议定于2024年5月15日下午3点，2天前已发送邀请。", &s);
        assert_eq!(zh_hits.len(), 2);
        assert_eq!(zh_hits[0].0, "2024年5月15日下午3点");
        assert_eq!(zh_hits[1].0, "2天前");
    }

    #[test]
    fn test_p0_safety_and_correctness() {
        let s = fixed_settings();

        // Non-ASCII whitespace must never panic
        assert!(parse_with_settings("2024-03-15\u{00A0}14:30", &s).is_some());
        assert!(parse_with_settings("15\u{202F}March\u{202F}2024", &s).is_some());
        assert!(parse_with_settings("2024年3月15日\u{3000}14時30分", &s).is_some());
        assert!(parse_with_settings("hello\u{00A0}world", &s).is_none());

        // Range UTF-8 & Turkish 'İ' safety
        assert_eq!(
            parse_date_range("12 NİSAN - 15 MAYIS 2026", &s)
                .unwrap()
                .1
                .naive_local()
                .date(),
            NaiveDate::from_ymd_opt(2026, 5, 15).unwrap()
        );
        assert!(parse_date_range("2月-5日", &s).is_none());

        // CJK standalone tokens in search_dates + Japanese PM/AM
        let cjk = crate::search::search_dates("Meeting: 2026年5月10日", &s);
        assert_eq!(cjk.len(), 1);
        assert_eq!(cjk[0].0, "2026年5月10日");
        assert_eq!(
            parse_with_settings("2026年3月15日 午後2時30分", &s)
                .unwrap()
                .naive_local(),
            NaiveDateTime::parse_from_str("2026-03-15 14:30:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );

        // Locale scoring dominance ("Jun" -> en month June, not fy evening)
        let jun_dd = get_date_data("Jun", &s);
        assert_eq!(jun_dd.locale.as_deref(), Some("en"));
        assert_eq!(jun_dd.date_obj.unwrap().naive_local().month(), 6);
        assert!(get_date_data("15 mar 2026 alle 14:30", &s).date_obj.is_some());
        assert!(get_date_data("15 mai 2026 kell 14:30", &s).date_obj.is_some());
        assert!(get_date_data("15 jun 2026 pukul 14:30", &s).date_obj.is_some());

        // Calendar bounds & rollovers
        assert!(calendars::jalali_to_gregorian(11767034, 1, 1).is_none());
        assert!(calendars::hijri_to_gregorian(6066335, 1, 1).is_none());
        assert!(calendars::french_republican_to_gregorian(50_000_000, 7, 14).is_none());
        assert!(parse_with_settings("0000-W01-1", &s).is_none());
        let mut s_tz = s.clone();
        s_tz.to_timezone = Some("Asia/Tokyo".to_string());
        assert!(parse_with_settings("9999-12-31 23:00:00 UTC", &s_tz).is_none());
    }
}
