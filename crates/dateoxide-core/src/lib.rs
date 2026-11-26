pub mod absolute;
pub mod fast_path;
pub mod locale;
pub mod relative;
pub mod search;
pub mod settings;
pub mod timezone;

use locale::{normalize_digits_and_lower, LocaleRegistry};
use rayon::prelude::*;
use settings::{DateData, ParsedDateTime, ParserKind, Period, Settings};
use timezone::{extract_trailing_timezone, finalize_datetime};

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

/// Full `dateparser.date.DateDataParser.get_date_data` equivalent returning
/// `DateData { date_obj, period, locale }`.
pub fn get_date_data(input: &str, settings: &Settings) -> DateData {
    let trimmed_raw = input.trim();
    if trimmed_raw.is_empty() {
        return DateData {
            date_obj: None,
            period: Period::Day,
            locale: None,
        };
    }

    let owned_cleaned;
    let trimmed: &str = if settings.skip_tokens.is_empty() {
        trimmed_raw
    } else {
        owned_cleaned = strip_user_skip_tokens(trimmed_raw, &settings.skip_tokens);
        if owned_cleaned.trim().is_empty() {
            return DateData {
                date_obj: None,
                period: Period::Day,
                locale: None,
            };
        }
        owned_cleaned.trim()
    };

    // 0. Custom user strptime formats (`date_formats`)
    if let Some(m) = fast_path::try_custom_formats(trimmed, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, m.tz, settings)),
            period: apply_time_period(m.period, m.has_time, settings),
            locale: Some("en".to_string()),
        };
    }

    // 1. Zero-regex, zero-allocation fast path (ISO-8601, RFC-2822/3339, numeric, Unix timestamps)
    if let Some(m) = fast_path::try_fast_path(trimmed, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, m.tz, settings)),
            period: apply_time_period(m.period, m.has_time, settings),
            locale: Some("en".to_string()),
        };
    }

    // 2. Peel off optional trailing timezone before natural-language normalization
    let (without_tz, tz) = extract_trailing_timezone(trimmed);
    let normalized = normalize_digits_and_lower(without_tz);
    if normalized.is_empty() {
        return DateData {
            date_obj: None,
            period: Period::Day,
            locale: None,
        };
    }

    // Re-check fast path if Unicode digits (e.g. Arabic-Indic / Persian) were normalized to ASCII
    if normalized.as_bytes()[0].is_ascii_digit() && normalized != without_tz {
        if let Some(m) = fast_path::try_fast_path(&normalized, settings) {
            return DateData {
                date_obj: Some(finalize_datetime(m.naive, m.tz.or(tz), settings)),
                period: apply_time_period(m.period, m.has_time, settings),
                locale: Some("en".to_string()),
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
        // One last attempt for CJK & Korean dates even if tokens didn't split on spaces
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
                };
            }
        }
        return DateData {
            date_obj: None,
            period: Period::Day,
            locale: None,
        };
    }

    // 4. Multilingual Relative Date Engine
    if settings.allows_parser(ParserKind::RelativeTime) {
        if let Some(m) = relative::try_parse_relative(&normalized, &candidates, registry, settings) {
            return DateData {
                date_obj: Some(finalize_datetime(m.naive, tz, settings)),
                period: apply_time_period(m.period, m.has_time, settings),
                locale: Some(m.locale),
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
            };
        }
    }

    DateData {
        date_obj: None,
        period: Period::Day,
        locale: None,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, NaiveDateTime, Timelike};
    use settings::{DateOrder, PreferDatesFrom, PreferDayOfMonth};

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
    fn test_fast_path_iso_rfc_numeric() {
        let s = fixed_settings();
        let dt = parse_with_settings("2026-11-26T15:30:00Z", &s).unwrap();
        assert!(dt.is_aware());
        assert_eq!(
            dt.naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 26)
                .unwrap()
                .and_hms_opt(15, 30, 0)
                .unwrap()
        );

        let dt_compact = parse_with_settings("20261126T153000Z", &s).unwrap();
        assert!(dt_compact.is_aware());
        assert_eq!(dt_compact.naive_local(), dt.naive_local());

        let dt_pm = parse_with_settings("5:30p.m.", &s).unwrap();
        assert_eq!(dt_pm.naive_local().hour(), 17);
        assert_eq!(dt_pm.naive_local().minute(), 30);

        let dt_fr_time = parse_with_settings("09h15", &s).unwrap();
        assert_eq!(dt_fr_time.naive_local().hour(), 9);
        assert_eq!(dt_fr_time.naive_local().minute(), 15);
    }

    #[test]
    fn test_diacritic_folding_and_time_of_day() {
        let s = fixed_settings();

        // Turkish ASCII-folded
        assert_eq!(
            parse_with_settings("3 gun once", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 23).unwrap()
        );
        assert_eq!(
            parse_with_settings("dun saat 14:30", &s).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 25)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap()
        );
        assert_eq!(
            parse_with_settings("15 Subat 2026", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 2, 15).unwrap()
        );

        // Time-of-day
        assert_eq!(
            parse_with_settings("yesterday at noon", &s).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 25)
                .unwrap()
                .and_hms_opt(12, 0, 0)
                .unwrap()
        );
        assert_eq!(
            parse_with_settings("tonight", &s).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 26)
                .unwrap()
                .and_hms_opt(20, 0, 0)
                .unwrap()
        );
        assert_eq!(
            parse_with_settings("dün gece", &s).unwrap().naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 25)
                .unwrap()
                .and_hms_opt(20, 0, 0)
                .unwrap()
        );

        // Korean
        assert_eq!(
            parse_with_settings("2026년 11월 26일", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 26).unwrap()
        );
    }

    #[test]
    fn test_settings_and_search() {
        let mut s = fixed_settings();
        s.region = Some("US".to_string());
        assert_eq!(
            parse_with_settings("05-11-2026", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 5, 11).unwrap()
        );

        s.region = Some("GB".to_string());
        assert_eq!(
            parse_with_settings("05-11-2026", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
        );

        s.return_time_as_period = true;
        let dd = get_date_data("2026-11-26 14:30", &s);
        assert_eq!(dd.period, Period::Time);

        let found = search_dates_with_locale("Meeting on 12 December 2025", &fixed_settings());
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].2.as_deref(), Some("en"));
    }

    #[test]
    fn test_invalid_strings_return_none() {
        let s = fixed_settings();
        for bad in [
            "",
            "not a date at all xyz",
            "hello world",
            "foo bar baz 12345",
            "...........................",
        ] {
            assert!(parse_with_settings(bad, &s).is_none());
        }
    }

    #[test]
    fn test_timezone_conversion() {
        let mut s = fixed_settings();
        s.to_timezone = Some("Europe/Istanbul".to_string());
        s.return_as_timezone_aware = Some(true);
        let dt = parse_with_settings("2026-11-26T12:00:00Z", &s).unwrap();
        assert_eq!(
            dt.naive_local(),
            NaiveDateTime::parse_from_str("2026-11-26 15:00:00", "%Y-%m-%d %H:%M:%S").unwrap()
        );
    }

    #[test]
    fn test_prefer_dates_from_and_day_of_month() {
        let mut s = fixed_settings();
        s.date_order = Some(DateOrder::DMY);
        assert_eq!(
            parse_with_settings("05/11/2026", &s).unwrap().naive_local().date(),
            NaiveDate::from_ymd_opt(2026, 11, 5).unwrap()
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
