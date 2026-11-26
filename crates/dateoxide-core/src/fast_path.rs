use crate::settings::{DateOrder, DatePart, Period, Settings};
use crate::timezone::{extract_trailing_timezone, TzSpec};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};

#[derive(Debug, Clone)]
pub struct FastPathMatch {
    pub naive: NaiveDateTime,
    pub tz: Option<TzSpec>,
    pub period: Period,
}

/// Try explicit user-provided strptime formats (`date_formats`).
pub fn try_custom_formats(input: &str, settings: &Settings) -> Option<FastPathMatch> {
    let formats = settings.date_formats.as_deref()?;
    if formats.is_empty() {
        return None;
    }
    let s = input.trim();
    for fmt in formats {
        if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
            return Some(FastPathMatch {
                naive: dt.naive_local(),
                tz: Some(TzSpec::Fixed(*dt.offset())),
                period: Period::Day,
            });
        }
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(FastPathMatch {
                naive: ndt,
                tz: None,
                period: Period::Day,
            });
        }
        if let Ok(nd) = NaiveDate::parse_from_str(s, fmt) {
            return Some(FastPathMatch {
                naive: nd.and_hms_opt(0, 0, 0)?,
                tz: None,
                period: Period::Day,
            });
        }
    }
    None
}

/// Zero-regex, zero-allocation fast path for ISO-8601, RFC-2822/3339,
/// delimited numeric dates, Unix timestamps, and compact YYYYMMDD dates.
pub fn try_fast_path(input: &str, settings: &Settings) -> Option<FastPathMatch> {
    let s = input.trim();
    if s.is_empty() || s.len() > 80 {
        return None;
    }

    // 1. RFC 2822 (e.g., "Thu, 26 Nov 2026 15:30:00 +0300")
    if s.len() >= 16 && s.contains(',') {
        if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
            return Some(FastPathMatch {
                naive: dt.naive_local(),
                tz: Some(TzSpec::Fixed(*dt.offset())),
                period: Period::Day,
            });
        }
    }

    // 2. RFC 3339 (e.g., "2026-11-26T15:30:00Z" or "2026-11-26T15:30:00.123+03:00")
    if s.len() >= 19 && s.as_bytes().get(4) == Some(&b'-') && s.as_bytes().get(7) == Some(&b'-') {
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Some(FastPathMatch {
                naive: dt.naive_local(),
                tz: Some(TzSpec::Fixed(*dt.offset())),
                period: Period::Day,
            });
        }
    }

    // Peel off optional trailing timezone before numeric checks
    let (core, tz) = extract_trailing_timezone(s);
    if core.is_empty() {
        return None;
    }

    // 3. Pure ASCII digits (Unix timestamp or compact YYYYMMDD)
    if core.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        if let Some(m) = try_timestamp_or_compact(core, tz, settings) {
            return Some(m);
        }
    }

    // 4. ISO-like & delimited numeric dates with optional time (e.g. "2026-11-26 15:30:00", "26.11.2026", "11/26/2026")
    if core.as_bytes()[0].is_ascii_digit() {
        if let Some(m) = try_numeric_datetime(core, tz, settings) {
            return Some(m);
        }
    }

    None
}

fn try_timestamp_or_compact(
    core: &str,
    tz: Option<TzSpec>,
    settings: &Settings,
) -> Option<FastPathMatch> {
    if !core.contains('.') {
        // Reject tiny numbers like "42" or "2026" here (year-only handled carefully below)
        match core.len() {
            8 => {
                // Check YYYYMMDD first if valid calendar date
                let y: i32 = core[0..4].parse().ok()?;
                let m: u32 = core[4..6].parse().ok()?;
                let d: u32 = core[6..8].parse().ok()?;
                if (1900..=2199).contains(&y) {
                    if let Some(nd) = NaiveDate::from_ymd_opt(y, m, d) {
                        return Some(FastPathMatch {
                            naive: nd.and_hms_opt(0, 0, 0)?,
                            tz,
                            period: Period::Day,
                        });
                    }
                }
                None
            }
            9..=10 => {
                // Unix epoch in seconds (1973 .. 2286)
                if settings.strict_parsing {
                    return None;
                }
                let secs: i64 = core.parse().ok()?;
                let dt = DateTime::from_timestamp(secs, 0)?;
                let naive = dt.naive_utc();
                Some(FastPathMatch {
                    naive,
                    tz: tz.or_else(|| FixedOffset::east_opt(0).map(TzSpec::Fixed)),
                    period: Period::Day,
                })
            }
            13 => {
                // Unix epoch in milliseconds
                if settings.strict_parsing {
                    return None;
                }
                let ms: i64 = core.parse().ok()?;
                let dt = DateTime::from_timestamp_millis(ms)?;
                Some(FastPathMatch {
                    naive: dt.naive_utc(),
                    tz: tz.or_else(|| FixedOffset::east_opt(0).map(TzSpec::Fixed)),
                    period: Period::Day,
                })
            }
            _ => None,
        }
    } else {
        // Float Unix timestamp like "1700000000.123"
        let (int_part, frac_part) = core.split_once('.')?;
        if (9..=10).contains(&int_part.len())
            && !frac_part.is_empty()
            && frac_part.len() <= 9
            && int_part.bytes().all(|b| b.is_ascii_digit())
            && frac_part.bytes().all(|b| b.is_ascii_digit())
        {
            let secs: i64 = int_part.parse().ok()?;
            let mut nanos_str = frac_part.to_string();
            while nanos_str.len() < 9 {
                nanos_str.push('0');
            }
            let nanos: u32 = nanos_str.parse().ok()?;
            let dt = DateTime::from_timestamp(secs, nanos)?;
            return Some(FastPathMatch {
                naive: dt.naive_utc(),
                tz: tz.or_else(|| FixedOffset::east_opt(0).map(TzSpec::Fixed)),
                period: Period::Day,
            });
        }
        None
    }
}

fn try_numeric_datetime(
    core: &str,
    tz: Option<TzSpec>,
    settings: &Settings,
) -> Option<FastPathMatch> {
    // Split date and optional time at 'T', 't', or first whitespace
    let (date_part, time_part) = if let Some(idx) = core.find(['T', 't', ' ']) {
        let (d, rest) = core.split_at(idx);
        let t = rest[1..].trim();
        (d, if t.is_empty() { None } else { Some(t) })
    } else {
        (core, None)
    };

    // Pure time-only like "14:30" or "5:30 pm" (no date separator)
    if time_part.is_none() && date_part.contains(':') && !date_part.contains(['-', '/', '.']) {
        if settings.strict_parsing
            || settings.require_parts.contains(&DatePart::Year)
            || settings.require_parts.contains(&DatePart::Month)
            || settings.require_parts.contains(&DatePart::Day)
        {
            return None;
        }
        let time = parse_time_str(date_part)?;
        let base = settings.effective_base();
        return Some(FastPathMatch {
            naive: base.date().and_time(time),
            tz,
            period: Period::Day,
        });
    }

    // Handle "14:30 pm" where space separated time and meridiem
    if let Some(tp) = time_part {
        if date_part.contains(':') && !date_part.contains(['-', '/', '.']) {
            if settings.strict_parsing {
                return None;
            }
            let combined = format!("{} {}", date_part, tp);
            let time = parse_time_str(&combined)?;
            let base = settings.effective_base();
            return Some(FastPathMatch {
                naive: base.date().and_time(time),
                tz,
                period: Period::Day,
            });
        }
    }

    // Parse delimited numeric date: YYYY-MM-DD, DD.MM.YYYY, MM/DD/YYYY, YYYY-MM
    let sep = if date_part.contains('-') {
        '-'
    } else if date_part.contains('/') {
        '/'
    } else if date_part.contains('.') {
        '.'
    } else {
        return None;
    };

    // Ensure consistent separator and all parts are ASCII digits
    let mut parts = [0u32; 3];
    let mut lens = [0usize; 3];
    let mut count = 0usize;
    for seg in date_part.split(sep) {
        if seg.is_empty() || seg.len() > 4 || count >= 3 || !seg.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        parts[count] = seg.parse().ok()?;
        lens[count] = seg.len();
        count += 1;
    }

    let time = match time_part {
        Some(tp) => parse_time_str(tp)?,
        None => NaiveTime::from_hms_opt(0, 0, 0)?,
    };

    if count == 2 {
        // YYYY-MM or MM/YYYY (partial date)
        if settings.strict_parsing || settings.require_parts.contains(&DatePart::Day) {
            return None;
        }
        let (y, m) = if lens[0] == 4 && (1..=12).contains(&parts[1]) {
            (parts[0] as i32, parts[1])
        } else if lens[1] == 4 && (1..=12).contains(&parts[0]) {
            (parts[1] as i32, parts[0])
        } else {
            return None;
        };
        let base = settings.effective_base();
        let day = resolve_missing_day(y, m, base.day(), settings.prefer_day_of_month);
        let nd = NaiveDate::from_ymd_opt(y, m, day)?;
        return Some(FastPathMatch {
            naive: nd.and_time(time),
            tz,
            period: Period::Month,
        });
    }

    if count != 3 {
        return None;
    }

    let order = settings.date_order.unwrap_or_else(|| {
        if lens[0] == 4 {
            DateOrder::YMD
        } else if sep == '.' {
            DateOrder::DMY
        } else if sep == '/' {
            DateOrder::MDY
        } else {
            DateOrder::DMY
        }
    });

    let nd = resolve_ymd_triplet(parts, lens, order)?;
    Some(FastPathMatch {
        naive: nd.and_time(time),
        tz,
        period: Period::Day,
    })
}

pub fn expand_two_digit_year(y: u32) -> i32 {
    if y >= 100 {
        y as i32
    } else if y >= 70 {
        1900 + y as i32
    } else {
        2000 + y as i32
    }
}

fn resolve_ymd_triplet(parts: [u32; 3], lens: [usize; 3], order: DateOrder) -> Option<NaiveDate> {
    // If first component has 4 digits, strongly prefer YMD (or YDM if configured)
    if lens[0] == 4 {
        let y = parts[0] as i32;
        if order == DateOrder::YDM {
            if let Some(d) = NaiveDate::from_ymd_opt(y, parts[2], parts[1]) {
                return Some(d);
            }
        }
        if let Some(d) = NaiveDate::from_ymd_opt(y, parts[1], parts[2]) {
            return Some(d);
        }
        return NaiveDate::from_ymd_opt(y, parts[2], parts[1]);
    }

    // If last component has 4 digits, order between first two is DMY vs MDY
    if lens[2] == 4 {
        let y = parts[2] as i32;
        return match order {
            DateOrder::MDY => NaiveDate::from_ymd_opt(y, parts[0], parts[1])
                .or_else(|| NaiveDate::from_ymd_opt(y, parts[1], parts[0])),
            _ => NaiveDate::from_ymd_opt(y, parts[1], parts[0])
                .or_else(|| NaiveDate::from_ymd_opt(y, parts[0], parts[1])),
        };
    }

    // All 2-digit components: follow explicit order with graceful fallback
    let (y, m, d) = match order {
        DateOrder::YMD => (expand_two_digit_year(parts[0]), parts[1], parts[2]),
        DateOrder::YDM => (expand_two_digit_year(parts[0]), parts[2], parts[1]),
        DateOrder::MDY => (expand_two_digit_year(parts[2]), parts[0], parts[1]),
        DateOrder::DMY => (expand_two_digit_year(parts[2]), parts[1], parts[0]),
        DateOrder::MYD => (expand_two_digit_year(parts[1]), parts[0], parts[2]),
        DateOrder::DYM => (expand_two_digit_year(parts[1]), parts[2], parts[0]),
    };

    NaiveDate::from_ymd_opt(y, m, d).or_else(|| {
        // Fallback swap month/day if invalid (e.g. 26/11/25 with MDY default)
        NaiveDate::from_ymd_opt(y, d, m)
    })
}

/// Parse time strings like "15:30", "15:30:45", "15:30:45.123456", "5:30 pm", "5pm", "14h30", "14h30m10s".
pub fn parse_time_str(input: &str) -> Option<NaiveTime> {
    let mut s = input.trim();
    if s.is_empty() {
        return None;
    }

    // Strip optional trailing AM / PM / a.m. / p.m.
    let mut is_pm = false;
    let mut is_am = false;
    let lower = s.to_ascii_lowercase();
    for suffix in ["p.m.", "p.m", "pm"] {
        if let Some(rest) = lower.strip_suffix(suffix) {
            if !rest.is_empty() {
                s = s[..rest.len()].trim_end();
                is_pm = true;
                break;
            }
        }
    }
    if !is_pm {
        for suffix in ["a.m.", "a.m", "am"] {
            if let Some(rest) = lower.strip_suffix(suffix) {
                if !rest.is_empty() {
                    s = s[..rest.len()].trim_end();
                    is_am = true;
                    break;
                }
            }
        }
    }

    // French / European "14h30" or "14h"
    if (s.contains('h') || s.contains('H')) && !s.contains(':') {
        let idx = s.find(['h', 'H'])?;
        let h_str = &s[..idx];
        let rest = &s[idx + 1..];
        let mut h: u32 = h_str.parse().ok()?;
        let (m, sec) = if rest.is_empty() {
            (0u32, 0u32)
        } else if let Some(mi) = rest.find(['m', 'M']) {
            let mm: u32 = rest[..mi].parse().ok()?;
            let s_rest = rest[mi + 1..].trim_end_matches(['s', 'S']);
            let ss: u32 = if s_rest.is_empty() { 0 } else { s_rest.parse().ok()? };
            (mm, ss)
        } else {
            (rest.parse().ok()?, 0u32)
        };
        apply_ampm(&mut h, is_am, is_pm)?;
        return NaiveTime::from_hms_opt(h, m, sec);
    }

    // Bare hour with AM/PM: "5pm", "12am"
    if (is_am || is_pm) && s.bytes().all(|b| b.is_ascii_digit()) && s.len() <= 2 {
        let mut h: u32 = s.parse().ok()?;
        apply_ampm(&mut h, is_am, is_pm)?;
        return NaiveTime::from_hms_opt(h, 0, 0);
    }

    // Standard HH:MM[:SS[.ffffff]]
    let mut colon_parts = s.split(':');
    let h_str = colon_parts.next()?;
    let m_str = colon_parts.next()?;
    let s_part = colon_parts.next();
    if colon_parts.next().is_some() {
        return None;
    }

    if h_str.is_empty()
        || h_str.len() > 2
        || m_str.is_empty()
        || m_str.len() > 2
        || !h_str.bytes().all(|b| b.is_ascii_digit())
        || !m_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }

    let mut h: u32 = h_str.parse().ok()?;
    let m: u32 = m_str.parse().ok()?;
    let (sec, nano) = match s_part {
        None => (0u32, 0u32),
        Some(sp) => {
            if let Some((sec_s, frac_s)) = sp.split_once(['.', ',']) {
                if sec_s.is_empty()
                    || sec_s.len() > 2
                    || frac_s.is_empty()
                    || !sec_s.bytes().all(|b| b.is_ascii_digit())
                    || !frac_s.bytes().all(|b| b.is_ascii_digit())
                {
                    return None;
                }
                let ss: u32 = sec_s.parse().ok()?;
                let mut padded = [b'0'; 9];
                for (i, b) in frac_s.bytes().take(9).enumerate() {
                    padded[i] = b;
                }
                let nn: u32 = std::str::from_utf8(&padded).ok()?.parse().ok()?;
                (ss, nn)
            } else {
                if sp.is_empty() || sp.len() > 2 || !sp.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                (sp.parse().ok()?, 0u32)
            }
        }
    };

    apply_ampm(&mut h, is_am, is_pm)?;
    NaiveTime::from_hms_nano_opt(h, m, sec, nano)
}

fn apply_ampm(h: &mut u32, is_am: bool, is_pm: bool) -> Option<()> {
    if is_pm {
        if *h == 0 || *h > 12 {
            return None;
        }
        if *h < 12 {
            *h += 12;
        }
    } else if is_am {
        if *h == 0 || *h > 12 {
            return None;
        }
        if *h == 12 {
            *h = 0;
        }
    }
    Some(())
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 30,
    }
}

pub fn resolve_missing_day(
    year: i32,
    month: u32,
    base_day: u32,
    pref: crate::settings::PreferDayOfMonth,
) -> u32 {
    let max_d = days_in_month(year, month);
    match pref {
        crate::settings::PreferDayOfMonth::First => 1,
        crate::settings::PreferDayOfMonth::Last => max_d,
        crate::settings::PreferDayOfMonth::Current => base_day.min(max_d),
    }
}
