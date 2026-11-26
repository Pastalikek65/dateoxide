pub mod absolute;
pub mod calendars;
pub mod fast_path;
pub mod locale;
pub mod relative;
pub mod search;
pub mod settings;
pub mod time_spans;
pub mod timezone;

use locale::{normalize_digits_and_lower, LocaleRegistry};
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
    for chunk in normalized.split(|c: char| c.is_whitespace() || c == ',' || c == '/') {
        let clean = chunk.trim_matches(|c: char| c == '.' || c == '(' || c == ')');
        if clean.is_empty() {
            continue;
        }
        if clean.contains('-') && !registry.token_to_langs.contains_key(clean) {
            for sub in clean.split('-') {
                if !sub.is_empty() {
                    tokens.push(sub);
                }
            }
        } else {
            tokens.push(clean);
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
        if settings.allows_parser(ParserKind::AbsoluteTime)
            && (normalized.contains('年')
                || normalized.contains('月')
                || normalized.contains('日')
                || normalized.contains('년')
                || normalized.contains('월')
                || normalized.contains('일'))
        {
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

/// Parallel batch parsing powered by Rayon across all available CPU cores.
pub fn parse_batch<S: AsRef<str> + Sync>(
    inputs: &[S],
    settings: &Settings,
) -> Vec<Option<ParsedDateTime>> {
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
}
