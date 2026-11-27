use crate::fast_path::{days_in_month, parse_time_str};
use crate::locale::TimeUnit;
use crate::relative::apply_relative_offset;
use crate::settings::{ParsedDateTime, Period, Settings};
use crate::timezone::{finalize_datetime_checked, is_valid_year_range, resolve_iana_local};
use crate::{get_date_data, parse_with_settings};
use chrono::{Datelike, Duration, NaiveDate};

const RANGE_INTRODUCERS: &[&str] = &[
    "between ", "from ", "von ", "zwischen ", "du ", "entre ", "desde ", "del ", "de ",
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
    " по ",
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
    "到",
];

/// Parse natural-language or numeric date/time ranges such as:
/// `"12-15 Ocak 2026"`, `"March 10-14, 2026"`, `"12 Ocak - 15 Şubat 2026"`,
/// `"from 2026-11-01 to 2026-11-15"`, `"2026-05-01/P14D"`, or `"14:00-16:30"`.
pub fn parse_date_range(
    input: &str,
    settings: &Settings,
) -> Option<(ParsedDateTime, ParsedDateTime)> {
    let mut s = input.trim();
    if s.is_empty() || s.len() > 200 {
        return None;
    }

    // 0. ISO-8601 intervals ("2026-05-01/2026-05-15", "2026-05-01/P14D")
    if let Some(pair) = try_iso_interval(s, settings) {
        return Some(pair);
    }

    // Never treat a plain ISO-8601 date or timestamp (e.g. "2026-11-26") as a range.
    if is_single_iso_like(s) {
        return None;
    }

    // 0b. Tight time ranges without spaces ("14:00-16:30")
    if let Some(pair) = try_tight_time_range(s, settings) {
        return Some(pair);
    }

    let mut stripped_intro = false;
    for prefix in ["с ", "С ", "от ", "От "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim_start();
            stripped_intro = true;
            break;
        }
    }
    if !stripped_intro {
        let ascii_lower = s.to_ascii_lowercase();
        for prefix in RANGE_INTRODUCERS {
            if ascii_lower.starts_with(prefix) && s.len() > prefix.len() {
                s = s[prefix.len()..].trim_start();
                break;
            }
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
        && !s.contains([' ', '–', '—', '~', '/'])
        && !s.contains("..")
}

fn shift_parsed_days(dt: &ParsedDateTime, delta_days: i64) -> Option<ParsedDateTime> {
    if delta_days == 0 {
        return Some(dt.clone());
    }
    let delta = Duration::days(delta_days);
    Some(match dt {
        ParsedDateTime::Naive(ndt) => ParsedDateTime::Naive(ndt.checked_add_signed(delta)?),
        ParsedDateTime::FixedOffset(fdt) => {
            ParsedDateTime::FixedOffset(fdt.checked_add_signed(delta)?)
        }
        ParsedDateTime::Tz(tdt) => ParsedDateTime::Tz(tdt.checked_add_signed(delta)?),
    })
}

fn snap_range_endpoint(
    dt: ParsedDateTime,
    period: Period,
    parts: &[&'static str],
    is_end: bool,
) -> ParsedDateTime {
    let naive = dt.naive_local();
    if period == Period::Month && !parts.contains(&"day") {
        let target_day = if is_end {
            days_in_month(naive.year(), naive.month())
        } else {
            1
        };
        let delta = target_day as i64 - naive.day() as i64;
        return shift_parsed_days(&dt, delta).unwrap_or(dt);
    }
    if period == Period::Year && !parts.contains(&"month") {
        let target = if is_end {
            NaiveDate::from_ymd_opt(naive.year(), 12, 31)
        } else {
            NaiveDate::from_ymd_opt(naive.year(), 1, 1)
        };
        if let Some(td) = target {
            let delta = (td - naive.date()).num_days();
            return shift_parsed_days(&dt, delta).unwrap_or(dt);
        }
    }
    dt
}

fn try_iso_interval(s: &str, settings: &Settings) -> Option<(ParsedDateTime, ParsedDateTime)> {
    let (left, right) = s.split_once('/')?;
    let lb = left.as_bytes();
    if right.contains('/') || lb.len() < 10 || !lb[0..4].iter().all(|b| b.is_ascii_digit()) || lb[4] != b'-' {
        return None;
    }
    let start_dt = parse_with_settings(left.trim(), settings)?;
    let r = right.trim();
    if r.starts_with('P') || r.starts_with('p') {
        let (date_shifted, time_delta) = apply_iso_duration_split(start_dt.naive_local(), r)?;
        let end_dt = match start_dt {
            ParsedDateTime::Naive(_) => ParsedDateTime::Naive(date_shifted.checked_add_signed(time_delta)?),
            ParsedDateTime::FixedOffset(f) => {
                let d_delta = date_shifted - start_dt.naive_local();
                ParsedDateTime::FixedOffset(f.checked_add_signed(d_delta + time_delta)?)
            }
            ParsedDateTime::Tz(t) => {
                let localized = resolve_iana_local(t.timezone(), date_shifted);
                ParsedDateTime::Tz(localized.checked_add_signed(time_delta)?)
            }
        };
        if !is_valid_year_range(&end_dt) {
            return None;
        }
        return Some((start_dt, end_dt));
    }
    if r.len() >= 10 && r.as_bytes()[0..4].iter().all(|b| b.is_ascii_digit()) && r.as_bytes()[4] == b'-' {
        let end_dt = parse_with_settings(r, settings)?;
        return Some((start_dt, end_dt));
    }
    None
}

fn apply_iso_duration_split(
    mut cur: chrono::NaiveDateTime,
    dur: &str,
) -> Option<(chrono::NaiveDateTime, Duration)> {
    let body = dur.strip_prefix('P').or_else(|| dur.strip_prefix('p'))?;
    if body.is_empty() {
        return None;
    }
    let mut in_time = false;
    let mut num_buf = String::new();
    let mut time_delta = Duration::zero();
    let mut applied = false;
    for ch in body.chars() {
        if ch == 'T' || ch == 't' {
            if !num_buf.is_empty() {
                return None;
            }
            in_time = true;
            continue;
        }
        if ch.is_ascii_digit() {
            num_buf.push(ch);
            continue;
        }
        let val_i: i64 = num_buf.parse().ok()?;
        let val: f64 = num_buf.parse().ok()?;
        num_buf.clear();
        if in_time {
            let add = match ch.to_ascii_uppercase() {
                'H' => Duration::hours(val_i),
                'M' => Duration::minutes(val_i),
                'S' => Duration::seconds(val_i),
                _ => return None,
            };
            time_delta += add;
        } else {
            let unit = match ch.to_ascii_uppercase() {
                'Y' => TimeUnit::Year,
                'M' => TimeUnit::Month,
                'W' => TimeUnit::Week,
                'D' => TimeUnit::Day,
                _ => return None,
            };
            cur = apply_relative_offset(cur, 1, val, unit)?;
        }
        applied = true;
    }
    if !num_buf.is_empty() || !applied {
        return None;
    }
    Some((cur, time_delta))
}

fn try_tight_time_range(s: &str, settings: &Settings) -> Option<(ParsedDateTime, ParsedDateTime)> {
    for sep in ['-', '–', '—'] {
        if let Some((l, r)) = s.split_once(sep) {
            if let (Some(t1), Some(t2)) = (parse_time_str(l.trim()), parse_time_str(r.trim())) {
                let base = settings.effective_base();
                let d1 = base.date().and_time(t1);
                let d2_date = if t2 < t1 {
                    base.date()
                        .succ_opt()
                        .filter(|d| (1..=9999).contains(&d.year()))?
                } else {
                    base.date()
                };
                return Some((
                    finalize_datetime_checked(d1, None, settings)?,
                    finalize_datetime_checked(d2_date.and_time(t2), None, settings)?,
                ));
            }
        }
    }
    None
}

fn try_split_connector_range(
    s: &str,
    settings: &Settings,
) -> Option<(ParsedDateTime, ParsedDateTime)> {
    let ascii_lower = s.to_ascii_lowercase();
    for &conn in RANGE_CONNECTORS {
        // Search ASCII connectors case-insensitively with 1:1 byte offsets (avoids 'İ' case-expansion drift)
        let split_pos = if conn.is_ascii() {
            ascii_lower.find(conn).map(|pos| (pos, pos + conn.len()))
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

        // Fast handling for standalone time ranges like "10:00 - 12:30"
        if let (Some(t1), Some(t2)) = (parse_time_str(left), parse_time_str(right)) {
            let base = settings.effective_base();
            let d1 = base.date().and_time(t1);
            let d2_date = if t2 < t1 {
                base.date()
                    .succ_opt()
                    .filter(|d| (1..=9999).contains(&d.year()))?
            } else {
                base.date()
            };
            let d2 = d2_date.and_time(t2);
            return Some((
                finalize_datetime_checked(d1, None, settings)?,
                finalize_datetime_checked(d2, None, settings)?,
            ));
        }

        // Date+Time on left, bare Time on right: "May 10, 2026 14:00 - 16:30"
        if let Some(t2) = parse_time_str(right) {
            if let Some(start_dt) = parse_with_settings(left, settings) {
                let sn = start_dt.naive_local();
                let d2_date = if t2 < sn.time() {
                    sn.date()
                        .succ_opt()
                        .filter(|d| (1..=9999).contains(&d.year()))?
                } else {
                    sn.date()
                };
                let end_naive = d2_date.and_time(t2);
                let end_dt = finalize_datetime_checked(end_naive, None, settings)?;
                return Some((start_dt, end_dt));
            }
        }

        // Left is full CJK/Korean date, right is bare day ("2026年5月10日〜15日", "2026년 5월 10일 - 15일")
        let end_dd = get_date_data(right, settings);
        if end_dd.date_obj.is_none()
            && (right.ends_with('日') || right.ends_with('号') || right.ends_with('일'))
        {
            if let Some(start_dt) = parse_with_settings(left, settings) {
                let sn = start_dt.naive_local();
                let inherited = if right.ends_with('일') {
                    format!("{}년 {}월 {}", sn.year(), sn.month(), right)
                } else {
                    format!("{}年{}月{}", sn.year(), sn.month(), right)
                };
                if let Some(end_dt) = parse_with_settings(&inherited, settings) {
                    return Some((start_dt, end_dt));
                }
            }
        }

        let Some(raw_end_dt) = end_dd.date_obj else {
            continue;
        };
        let end_dt = snap_range_endpoint(raw_end_dt, end_dd.period, &end_dd.parts, true);

        // Shared-month bare day on left ("10 to 15 May 2026", "du 10 au 15 mai 2026", "с 10 по 15 мая 2026")
        if is_day_num(left) {
            if let Ok(d) = left.parse::<u32>() {
                let en = end_dt.naive_local();
                if let Some(sd) = NaiveDate::from_ymd_opt(en.year(), en.month(), d) {
                    if let Some(start_dt) = finalize_datetime_checked(sd.and_time(en.time()), None, settings) {
                        return Some((start_dt, end_dt));
                    }
                }
            }
        }

        // Inherit year from right endpoint only when needed (preserve direct weekday/word parses like "from Monday to Friday")
        let left_has_year = has_four_digit_year(left);
        if !left_has_year {
            let end_year = end_dt.naive_local().year();
            if !left.chars().any(|c| c.is_ascii_digit()) {
                let direct_dd = get_date_data(left, settings);
                if let Some(raw_direct) = direct_dd.date_obj {
                    if raw_direct.naive_local().year() == end_year {
                        let start_dt =
                            snap_range_endpoint(raw_direct, direct_dd.period, &direct_dd.parts, false);
                        let mut final_end = end_dt;
                        if start_dt.naive_local() > final_end.naive_local()
                            && !right.chars().any(|c| c.is_ascii_digit())
                        {
                            if let Some(shifted) = shift_parsed_days(&final_end, 7) {
                                if is_valid_year_range(&shifted) {
                                    final_end = shifted;
                                }
                            }
                        }
                        return Some((start_dt, final_end));
                    }
                }
            }
            let left_with_year = format!("{} {}", left.trim_end_matches(','), end_year);
            let left_dd = get_date_data(&left_with_year, settings);
            if let Some(raw_start) = left_dd.date_obj {
                let mut start_dt =
                    snap_range_endpoint(raw_start, left_dd.period, &left_dd.parts, false);
                if start_dt.naive_local() > end_dt.naive_local() {
                    let prev_year = format!("{} {}", left.trim_end_matches(','), end_year - 1);
                    let prev_dd = get_date_data(&prev_year, settings);
                    if let Some(raw_prev) = prev_dd.date_obj {
                        start_dt =
                            snap_range_endpoint(raw_prev, prev_dd.period, &prev_dd.parts, false);
                    }
                }
                return Some((start_dt, end_dt));
            }
        }

        let start_dd = get_date_data(left, settings);
        if let Some(raw_start) = start_dd.date_obj {
            let start_dt =
                snap_range_endpoint(raw_start, start_dd.period, &start_dd.parts, false);
            return Some((start_dt, end_dt));
        }
    }
    None
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
            .char_indices()
            .rev()
            .find(|&(_, c)| !c.is_ascii_digit())
            .map(|(i, c)| i + c.len_utf8())
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
