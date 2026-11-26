pub mod absolute;
pub mod fast_path;
pub mod locale;
pub mod relative;
pub mod search;
pub mod settings;
pub mod timezone;

use locale::{normalize_digits_and_lower, LocaleRegistry};
use rayon::prelude::*;
use settings::{DateData, ParsedDateTime, Period, Settings};
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

/// Full `dateparser.date.DateDataParser.get_date_data` equivalent returning
/// `DateData { date_obj, period, locale }`.
pub fn get_date_data(input: &str, settings: &Settings) -> DateData {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return DateData {
            date_obj: None,
            period: Period::Day,
            locale: None,
        };
    }

    // 0. Custom user strptime formats (`date_formats`)
    if let Some(m) = fast_path::try_custom_formats(trimmed, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, m.tz, settings)),
            period: m.period,
            locale: Some("en".to_string()),
        };
    }

    // 1. Zero-regex, zero-allocation fast path (ISO-8601, RFC-2822/3339, numeric, Unix timestamps)
    if let Some(m) = fast_path::try_fast_path(trimmed, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, m.tz, settings)),
            period: m.period,
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

    // Re-check fast path if Unicode digits (e.g. Arabic-Indic) were normalized to ASCII
    if normalized.as_bytes()[0].is_ascii_digit() && normalized != without_tz {
        if let Some(m) = fast_path::try_fast_path(&normalized, settings) {
            return DateData {
                date_obj: Some(finalize_datetime(m.naive, m.tz.or(tz), settings)),
                period: m.period,
                locale: Some("en".to_string()),
            };
        }
    }

    // 3. Lazy-load the compact 205-language registry and detect top candidates in O(tokens)
    let registry = LocaleRegistry::global();
    let tokens: Vec<&str> = normalized
        .split(|c: char| c.is_whitespace() || c == ',' || c == '/' || c == '-')
        .map(|t| t.trim_matches(|c: char| c == '.' || c == '(' || c == ')'))
        .filter(|t| !t.is_empty())
        .collect();

    let candidates = registry.detect_candidates(
        &normalized,
        &tokens,
        settings.languages.as_deref(),
        &settings.default_languages,
    );

    if candidates.is_empty() {
        // One last attempt for CJK dates even if tokens didn't split on spaces
        if normalized.contains('年') || normalized.contains('月') || normalized.contains('日') {
            if let Some(m) = absolute::try_parse_absolute(&normalized, &[], registry, settings) {
                return DateData {
                    date_obj: Some(finalize_datetime(m.naive, tz, settings)),
                    period: m.period,
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

    // 4. Multilingual Relative Date Engine (zero-regex structured affixes + exact + compound)
    if let Some(m) = relative::try_parse_relative(&normalized, &candidates, registry, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, tz, settings)),
            period: m.period,
            locale: Some(m.locale),
        };
    }

    // 5. Multilingual Absolute & Partial Date Engine
    if let Some(m) = absolute::try_parse_absolute(&normalized, &candidates, registry, settings) {
        return DateData {
            date_obj: Some(finalize_datetime(m.naive, tz, settings)),
            period: m.period,
            locale: Some(m.locale),
        };
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
    // For small batches, avoid Rayon scheduling overhead
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

        let dt2 = parse_with_settings("26.11.2026 14:45", &s).unwrap();
        assert_eq!(
            dt2.naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 26)
                .unwrap()
                .and_hms_opt(14, 45, 0)
                .unwrap()
        );

        let dt3 = parse_with_settings("Thu, 26 Nov 2026 15:30:00 +0300", &s).unwrap();
        assert!(dt3.is_aware());
        assert_eq!(dt3.naive_local().hour(), 15);
    }

    #[test]
    fn test_multilingual_relative() {
        let s = fixed_settings();

        // English
        let d_en = parse_with_settings("3 days ago", &s).unwrap();
        assert_eq!(d_en.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 23).unwrap());

        // Turkish
        let d_tr = parse_with_settings("3 gün önce", &s).unwrap();
        assert_eq!(d_tr.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 23).unwrap());

        let d_tr2 = parse_with_settings("yarın", &s).unwrap();
        assert_eq!(d_tr2.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 27).unwrap());

        let d_tr3 = parse_with_settings("dün saat 18:30", &s).unwrap();
        assert_eq!(
            d_tr3.naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 25)
                .unwrap()
                .and_hms_opt(18, 30, 0)
                .unwrap()
        );

        // German
        let d_de = parse_with_settings("vor 2 Wochen", &s).unwrap();
        assert_eq!(d_de.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 12).unwrap());

        // French
        let d_fr = parse_with_settings("hier à 14h30", &s).unwrap();
        assert_eq!(
            d_fr.naive_local(),
            NaiveDate::from_ymd_opt(2026, 11, 25)
                .unwrap()
                .and_hms_opt(14, 30, 0)
                .unwrap()
        );

        // Spanish
        let d_es = parse_with_settings("hace 5 horas", &s).unwrap();
        assert_eq!(d_es.naive_local().hour(), 10);

        // Japanese
        let d_ja = parse_with_settings("3日前", &s).unwrap();
        assert_eq!(d_ja.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 23).unwrap());

        // Compound
        let d_comp = parse_with_settings("1 year, 2 months, 3 days ago", &s).unwrap();
        assert_eq!(d_comp.naive_local().date(), NaiveDate::from_ymd_opt(2025, 9, 23).unwrap());
    }

    #[test]
    fn test_multilingual_absolute() {
        let s = fixed_settings();

        let cases = [
            ("12 December 2025", (2025, 12, 12)),
            ("12 Aralık 2025", (2025, 12, 12)),
            ("12 de diciembre de 2025", (2025, 12, 12)),
            ("26. November 2026", (2026, 11, 26)),
            ("le 14 juillet 1789", (1789, 7, 14)),
            ("2026年11月26日", (2026, 11, 26)),
            ("26 ноября 2026", (2026, 11, 26)),
        ];

        for (input, (y, m, d)) in cases {
            let parsed = parse_with_settings(input, &s)
                .unwrap_or_else(|| panic!("failed to parse '{}'", input));
            assert_eq!(
                parsed.naive_local().date(),
                NaiveDate::from_ymd_opt(y, m, d).unwrap(),
                "mismatch for '{}'",
                input
            );
        }
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
            assert!(
                parse_with_settings(bad, &s).is_none(),
                "expected None for '{}'",
                bad
            );
        }
    }

    #[test]
    fn test_settings_behavior() {
        let mut s = fixed_settings();
        s.date_order = Some(DateOrder::DMY);
        let d = parse_with_settings("05/11/2026", &s).unwrap();
        assert_eq!(d.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 5).unwrap());

        s.date_order = Some(DateOrder::MDY);
        let d2 = parse_with_settings("05/11/2026", &s).unwrap();
        assert_eq!(d2.naive_local().date(), NaiveDate::from_ymd_opt(2026, 5, 11).unwrap());

        s.prefer_day_of_month = PreferDayOfMonth::First;
        let d3 = parse_with_settings("December 2025", &s).unwrap();
        assert_eq!(d3.naive_local().date(), NaiveDate::from_ymd_opt(2025, 12, 1).unwrap());

        s.prefer_dates_from = PreferDatesFrom::Past;
        let d4 = parse_with_settings("December 15", &s).unwrap();
        assert_eq!(d4.naive_local().date(), NaiveDate::from_ymd_opt(2025, 12, 15).unwrap());

        s.strict_parsing = true;
        assert!(parse_with_settings("December 2025", &s).is_none());
    }

    #[test]
    fn test_search_dates() {
        let s = fixed_settings();
        let text = "The meeting is on 12 December 2025 and follow-up was 3 days ago in Istanbul.";
        let found = search_dates(text, &s);
        assert_eq!(found.len(), 2, "found: {:?}", found);
        assert_eq!(found[0].1.naive_local().date(), NaiveDate::from_ymd_opt(2025, 12, 12).unwrap());
        assert_eq!(found[1].1.naive_local().date(), NaiveDate::from_ymd_opt(2026, 11, 23).unwrap());
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
}
