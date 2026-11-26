use crate::fast_path::parse_time_str;
use crate::parse_with_settings;
use crate::settings::{ParsedDateTime, Settings};
use crate::timezone::finalize_datetime;
use chrono::Datelike;

const RANGE_INTRODUCERS: &[&str] = &[
    "between ", "from ", "von ", "zwischen ", "du ", "entre ", "desde ", "de ",
];

const RANGE_CONNECTORS: &[&str] = &[
    " jusqu'au ",
    " jusqu'à ",
    " through ",
    " until ",
    " thru ",
    " entre ",
    " hasta ",
    " and ",
    " bis ",
    " ile ",
    " to ",
    " au ",
    " al ",
    " ve ",
    " - ",
    " – ",
    " — ",
    " 〜 ",
    " ～ ",
    " ~ ",
    "..",
    "〜",
    "～",
];

/// Parse natural-language or numeric date/time ranges such as:
/// `"12-15 Ocak 2026"`, `"March 10-14, 2026"`, `"12 Ocak - 15 Şubat 2026"`,
/// `"from 2026-11-01 to 2026-11-15"`, or `"10:00 - 12:30"`.
pub fn parse_date_range(
    input: &str,
    settings: &Settings,
) -> Option<(ParsedDateTime, ParsedDateTime)> {
    let mut s = input.trim();
    if s.is_empty() || s.len() > 200 {
        return None;
    }

    // Never treat a plain ISO-8601 date or timestamp (e.g. "2026-11-26") as a range.
    if is_single_iso_like(s) {
        return None;
    }

    let lower = s.to_lowercase();
    for prefix in RANGE_INTRODUCERS {
        if lower.starts_with(prefix) && s.len() > prefix.len() {
            // Ensure prefix boundary aligns cleanly before slicing original string
            let char_count = prefix.chars().count();
            let byte_cut = s
                .char_indices()
                .nth(char_count)
                .map(|(idx, _)| idx)
                .unwrap_or(s.len());
            s = s[byte_cut..].trim_start();
            break;
        }
    }

    // 1. Explicit binary connector split ("12 Ocak - 15 Şubat 2026", "from 2026-11-01 to 2026-11-15")
    if let Some(pair) = try_split_connector_range(s, settings) {
        return Some(pair);
    }

    // 2. Compact same-month day range: "12-15 Ocak 2026", "12–15 December 2025", "March 10-14, 2026"
    if let Some(pair) = try_compact_same_month_range(s, settings) {
        return Some(pair);
    }

    None
}

fn is_single_iso_like(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 10
        && b[0..4].iter().all(|c| c.is_ascii_digit())
        && b[4] == b'-'
        && !s.contains([' ', '–', '—', '~'])
        && !s.contains("..")
}

fn try_split_connector_range(
    s: &str,
    settings: &Settings,
) -> Option<(ParsedDateTime, ParsedDateTime)> {
    let lower = s.to_lowercase();
    for &conn in RANGE_CONNECTORS {
        // Case-insensitive search when connector is ASCII, exact search otherwise
        let split_pos = if conn.is_ascii() {
            lower.find(conn).and_then(|pos| {
                // Map char index from lowercased string to original string safely
                let chars_before = lower[..pos].chars().count();
                let chars_conn = conn.chars().count();
                let start = s.char_indices().nth(chars_before).map(|(i, _)| i)?;
                let end = s
                    .char_indices()
                    .nth(chars_before + chars_conn)
                    .map(|(i, _)| i)
                    .unwrap_or(s.len());
                Some((start, end))
            })
        } else {
            s.find(conn).map(|pos| (pos, pos + conn.len()))
        };

        let Some((start, end)) = split_pos else {
            continue;
        };
        let left = s[..start].trim();
        let right = s[end..].trim();
        if left.is_empty() || right.is_empty() {
            continue;
        }
        if conn == " and " && !lower.starts_with("between ") && !input_had_between(s) {
            // Only parse "A and B" if left looks date-like; check right and left cleanly
        }

        // Fast handling for standalone time ranges like "10:00 - 12:30"
        if let (Some(t1), Some(t2)) = (parse_time_str(left), parse_time_str(right)) {
            let base = settings.effective_base();
            let d1 = base.date().and_time(t1);
            let d2_date = if t2 < t1 {
                base.date().succ_opt().unwrap_or(base.date())
            } else {
                base.date()
            };
            let d2 = d2_date.and_time(t2);
            return Some((
                finalize_datetime(d1, None, settings),
                finalize_datetime(d2, None, settings),
            ));
        }

        let Some(end_dt) = parse_with_settings(right, settings) else {
            continue;
        };

        // Inherit year from right endpoint when left lacks a 4-digit year (e.g. "12 Ocak - 15 Şubat 2026")
        let left_has_year = has_four_digit_year(left);
        if !left_has_year {
            let end_year = end_dt.naive_local().year();
            let left_with_year = format!("{} {}", left.trim_end_matches(','), end_year);
            if let Some(mut start_dt) = parse_with_settings(&left_with_year, settings) {
                if start_dt.naive_local() > end_dt.naive_local() {
                    let prev_year = format!("{} {}", left.trim_end_matches(','), end_year - 1);
                    if let Some(earlier) = parse_with_settings(&prev_year, settings) {
                        start_dt = earlier;
                    }
                }
                return Some((start_dt, end_dt));
            }
        }

        if let Some(start_dt) = parse_with_settings(left, settings) {
            return Some((start_dt, end_dt));
        }
    }
    None
}

#[inline]
fn input_had_between(_s: &str) -> bool {
    false
}

fn has_four_digit_year(s: &str) -> bool {
    let mut run = 0usize;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            run += 1;
            if run == 4 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

fn try_compact_same_month_range(
    s: &str,
    settings: &Settings,
) -> Option<(ParsedDateTime, ParsedDateTime)> {
    // Normalize en-dash / em-dash / tilde inside compact day spans to '-'
    let normalized_dashes: String = s
        .chars()
        .map(|c| if matches!(c, '–' | '—' | '~' | '～') { '-' } else { c })
        .collect();
    let work = normalized_dashes.as_str();

    // Case A: Starts with "<D1>-<D2> <Month...>" (e.g. "12-15 Ocak 2026", "12-15 December 2025")
    if work.as_bytes().first().is_some_and(|b| b.is_ascii_digit()) {
        if let Some(space_idx) = work.find(char::is_whitespace) {
            let first_tok = &work[..space_idx];
            let rest = work[space_idx..].trim();
            if !rest.is_empty() && first_tok.contains('-') {
                if let Some((d1, d2)) = first_tok.split_once('-') {
                    if is_day_num(d1) && is_day_num(d2) {
                        let left_str = format!("{} {}", d1, rest);
                        let right_str = format!("{} {}", d2, rest);
                        if let (Some(a), Some(b)) = (
                            parse_with_settings(&left_str, settings),
                            parse_with_settings(&right_str, settings),
                        ) {
                            return Some((a, b));
                        }
                    }
                }
            }
        }
    }

    // Case B: "<Month> <D1>-<D2>[, <Year>]" (e.g. "March 10-14, 2026", "December 1-5")
    if let Some(dash_idx) = work.find('-') {
        let before_dash = &work[..dash_idx];
        let after_dash = &work[dash_idx + 1..];
        let d1_start = before_dash
            .rfind(|c: char| !c.is_ascii_digit())
            .map(|i| i + 1)
            .unwrap_or(0);
        let d1 = &before_dash[d1_start..];
        let prefix = before_dash[..d1_start].trim_end();

        let d2_end = after_dash
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after_dash.len());
        let d2 = &after_dash[..d2_end];
        let suffix = after_dash[d2_end..].trim();

        if !prefix.is_empty()
            && prefix.chars().any(|c| c.is_alphabetic())
            && is_day_num(d1)
            && is_day_num(d2)
        {
            let left_str = if suffix.is_empty() {
                format!("{} {}", prefix, d1)
            } else {
                format!("{} {}{}", prefix, d1, if suffix.starts_with(',') { suffix.to_string() } else { format!(" {}", suffix) })
            };
            let right_str = if suffix.is_empty() {
                format!("{} {}", prefix, d2)
            } else {
                format!("{} {}{}", prefix, d2, if suffix.starts_with(',') { suffix.to_string() } else { format!(" {}", suffix) })
            };
            if let (Some(a), Some(b)) = (
                parse_with_settings(&left_str, settings),
                parse_with_settings(&right_str, settings),
            ) {
                return Some((a, b));
            }
        }
    }

    None
}

#[inline]
fn is_day_num(s: &str) -> bool {
    if !(1..=2).contains(&s.len()) || !s.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    matches!(s.parse::<u32>(), Ok(1..=31))
}
