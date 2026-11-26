use crate::fast_path::{days_in_month, expand_two_digit_year, parse_time_str, resolve_missing_day};
use crate::locale::{CompiledLocale, LocaleRegistry};
use crate::settings::{DateOrder, DatePart, Period, PreferDatesFrom, PreferMonthOfYear, Settings};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime};

#[derive(Debug, Clone)]
pub struct AbsoluteMatch {
    pub naive: NaiveDateTime,
    pub period: Period,
    pub locale: String,
}

/// Try parsing a natural-language absolute or partial date across candidate locales.
pub fn try_parse_absolute(
    normalized: &str,
    candidate_ids: &[usize],
    registry: &LocaleRegistry,
    settings: &Settings,
) -> Option<AbsoluteMatch> {
    // Handle CJK structured dates cleanly first: e.g. "2026年11月26日 15時30分45秒"
    if normalized.contains('年') || normalized.contains('月') || normalized.contains('日') {
        if let Some(m) = try_cjk_date(normalized, settings) {
            return Some(m);
        }
    }

    // Handle bare 4-digit year (e.g. "2025") if not strict
    if normalized.len() == 4 && normalized.bytes().all(|b| b.is_ascii_digit()) {
        if !settings.strict_parsing
            && !settings.require_parts.contains(&DatePart::Month)
            && !settings.require_parts.contains(&DatePart::Day)
        {
            if let Ok(year) = normalized.parse::<i32>() {
                if (1000..=2999).contains(&year) {
                    let base = settings.effective_base();
                    let month = match settings.prefer_month_of_year {
                        PreferMonthOfYear::First => 1,
                        PreferMonthOfYear::Last => 12,
                        PreferMonthOfYear::Current => base.month(),
                    };
                    let day = resolve_missing_day(year, month, base.day(), settings.prefer_day_of_month);
                    if let Some(nd) = NaiveDate::from_ymd_opt(year, month, day) {
                        return Some(AbsoluteMatch {
                            naive: nd.and_hms_opt(0, 0, 0)?,
                            period: Period::Year,
                            locale: "en".to_string(),
                        });
                    }
                }
            }
        }
        return None;
    }

    for &lid in candidate_ids {
        let loc = &registry.locales[lid];
        if let Some(m) = try_parse_in_locale(normalized, loc, settings) {
            return Some(m);
        }
    }

    None
}

fn try_cjk_date(s: &str, settings: &Settings) -> Option<AbsoluteMatch> {
    let mut year: Option<i32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;
    let mut hour: u32 = 0;
    let mut minute: u32 = 0;
    let mut second: u32 = 0;

    let mut cur_num = String::new();
    for ch in s.chars() {
        if ch.is_ascii_digit() {
            cur_num.push(ch);
        } else if !cur_num.is_empty() {
            let val: u32 = cur_num.parse().ok()?;
            cur_num.clear();
            match ch {
                '年' => year = Some(val as i32),
                '月' => month = Some(val),
                '日' | '号' => day = Some(val),
                '時' | '时' => hour = val,
                '分' => minute = val,
                '秒' => second = val,
                ':' => {
                    // Fallback to HH:MM embedded after CJK date
                    if let Some(t_idx) = s.find(|c: char| c == ':') {
                        if t_idx >= 2 {
                            let start = s[..t_idx]
                                .rfind(|c: char| !c.is_ascii_digit())
                                .map(|i| i + 1)
                                .unwrap_or(0);
                            if let Some(t) = parse_time_str(&s[start..]) {
                                hour = t.hour();
                                minute = t.minute();
                                second = t.second();
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if year.is_none() && month.is_none() {
        return None;
    }

    let base = settings.effective_base();
    let (y, m, d, period) = complete_partial_date(year, month, day, base, settings)?;
    let nd = NaiveDate::from_ymd_opt(y, m, d)?;
    let nt = NaiveTime::from_hms_opt(hour, minute, second)?;
    let lang = if s.contains('時') { "ja" } else { "zh" };
    Some(AbsoluteMatch {
        naive: nd.and_time(nt),
        period,
        locale: lang.to_string(),
    })
}

fn try_parse_in_locale(
    normalized: &str,
    loc: &CompiledLocale,
    settings: &Settings,
) -> Option<AbsoluteMatch> {
    // Tokenize splitting on whitespace and common separators, preserving time tokens like "14:30:00" and "14h30"
    let raw_tokens = split_date_tokens(normalized);
    if raw_tokens.is_empty() || raw_tokens.len() > 16 {
        return None;
    }

    let mut year: Option<i32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;
    let mut weekday_seen: Option<u8> = None;
    let mut time: Option<NaiveTime> = None;
    let mut ampm: Option<bool> = None; // Some(true) = PM, Some(false) = AM
    let mut free_nums: Vec<(u32, usize)> = Vec::new();
    let mut matched_locale_signal = false;

    let mut i = 0;
    while i < raw_tokens.len() {
        let tok = raw_tokens[i];
        let clean = tok.trim_matches(|c: char| c == ',' || c == '.' || c == '(' || c == ')');
        if clean.is_empty() {
            i += 1;
            continue;
        }

        // 1. Check time token (e.g., "14:30", "14:30:45", "14h30", "5pm")
        if time.is_none()
            && (clean.contains(':')
                || (clean.len() >= 3
                    && clean.as_bytes()[0].is_ascii_digit()
                    && (clean.contains('h') || clean.ends_with("am") || clean.ends_with("pm"))))
        {
            // Look ahead for separate "am" / "pm" token
            if i + 1 < raw_tokens.len() {
                let next_clean = raw_tokens[i + 1].trim_matches('.');
                if next_clean == "am" || next_clean == "pm" || loc.am.contains(next_clean) || loc.pm.contains(next_clean) {
                    let is_pm = next_clean == "pm" || loc.pm.contains(next_clean);
                    let suffix = if is_pm { "pm" } else { "am" };
                    let combined = format!("{} {}", clean, suffix);
                    if let Some(t) = parse_time_str(&combined) {
                        time = Some(t);
                        i += 2;
                        continue;
                    }
                }
            }
            if let Some(t) = parse_time_str(clean) {
                time = Some(t);
                i += 1;
                continue;
            }
        }

        // 2. Check AM/PM markers
        if clean == "pm" || clean == "p.m" || loc.pm.contains(clean) {
            ampm = Some(true);
            i += 1;
            continue;
        }
        if clean == "am" || clean == "a.m" || loc.am.contains(clean) {
            ampm = Some(false);
            i += 1;
            continue;
        }

        // 3. Check Month name (try 2-word month first, then 1-word)
        if month.is_none() {
            if i + 1 < raw_tokens.len() {
                let bigram = format!("{} {}", clean, raw_tokens[i + 1].trim_matches(','));
                if let Some(&m) = loc.months.get(&bigram) {
                    month = Some(m as u32);
                    matched_locale_signal = true;
                    i += 2;
                    continue;
                }
            }
            if let Some(&m) = loc.months.get(clean) {
                month = Some(m as u32);
                matched_locale_signal = true;
                i += 1;
                continue;
            }
        }

        // 4. Check Weekday name
        if weekday_seen.is_none() {
            if let Some(&wd) = loc.weekdays.get(clean) {
                weekday_seen = Some(wd);
                matched_locale_signal = true;
                i += 1;
                continue;
            }
        }

        // 5. Check numeric token (including ordinals like "1st", "2nd", "3rd", "21st", "1er", "15.")
        if let Some((num, digit_len)) = parse_date_number_or_ordinal(clean) {
            if digit_len == 4 && (1000..=2999).contains(&num) && year.is_none() {
                year = Some(num as i32);
            } else {
                free_nums.push((num, digit_len));
            }
            i += 1;
            continue;
        }

        // 6. Check skip / connector words ("at", "on", "of", "the", "de", "del", "le", "am", "um", "saat", "günü", "yılında")
        if loc.skip.contains(clean)
            || matches!(
                clean,
                "at" | "on" | "of" | "the" | "in" | "de" | "del" | "el" | "la" | "le" | "à"
                    | "am" | "um" | "den" | "saat" | "tarih" | "tarihinde" | "в" | "г" | "года"
                    | "günü" | "yılında" | "yılı"
            )
        {
            i += 1;
            continue;
        }

        // Reject unrecognized non-date words to avoid false positives like "3 days ago in" -> "3 August"
        return None;
    }

    // Must have matched a genuine month, weekday, or year+numeric signal
    if !matched_locale_signal && year.is_none() {
        return None;
    }

    // Apply standalone AM/PM if time was parsed in 12h form
    if let (Some(t), Some(is_pm)) = (time, ampm) {
        let mut h = t.hour();
        if is_pm && h >= 1 && h < 12 {
            h += 12;
        } else if !is_pm && h == 12 {
            h = 0;
        }
        time = NaiveTime::from_hms_nano_opt(h, t.minute(), t.second(), t.nanosecond());
    }

    // Assign remaining free numbers to day / month / year
    let order = settings.date_order.unwrap_or(loc.date_order);
    match free_nums.as_slice() {
        [] => {}
        [(n, len)] => {
            if month.is_some() && day.is_none() && (1..=31).contains(n) && *len <= 2 {
                day = Some(*n);
            } else if year.is_none() && *len == 2 {
                year = Some(expand_two_digit_year(*n));
            } else {
                return None;
            }
        }
        [(n1, l1), (n2, l2)] => {
            if month.is_some() && day.is_none() && year.is_none() {
                // e.g. "Dec 26 25" or "26 Dec 25"
                if (1..=31).contains(n1) && *l1 <= 2 && *l2 == 2 {
                    day = Some(*n1);
                    year = Some(expand_two_digit_year(*n2));
                } else if (1..=31).contains(n2) && *l2 <= 2 && *l1 == 2 {
                    year = Some(expand_two_digit_year(*n1));
                    day = Some(*n2);
                } else {
                    return None;
                }
            } else if month.is_none() && day.is_none() {
                // e.g. "26/11 2026"
                match order {
                    DateOrder::MDY if (1..=12).contains(n1) && (1..=31).contains(n2) => {
                        month = Some(*n1);
                        day = Some(*n2);
                    }
                    _ if (1..=31).contains(n1) && (1..=12).contains(n2) => {
                        day = Some(*n1);
                        month = Some(*n2);
                    }
                    _ if (1..=12).contains(n1) && (1..=31).contains(n2) => {
                        month = Some(*n1);
                        day = Some(*n2);
                    }
                    _ => return None,
                }
            } else {
                return None;
            }
        }
        _ => return None,
    }

    // Weekday-only (e.g., "Monday", "Pazartesi", "Friday at 15:00")
    if year.is_none() && month.is_none() && day.is_none() {
        let wd = weekday_seen?;
        if settings.strict_parsing
            || settings.require_parts.contains(&DatePart::Year)
            || settings.require_parts.contains(&DatePart::Month)
            || settings.require_parts.contains(&DatePart::Day)
        {
            return None;
        }
        let base = settings.effective_base();
        let cur_wd = base.weekday().num_days_from_monday() as i64;
        let tgt_wd = wd as i64;
        let diff = match settings.prefer_dates_from {
            PreferDatesFrom::Past => {
                let mut d = tgt_wd - cur_wd;
                if d > 0 {
                    d -= 7;
                }
                d
            }
            PreferDatesFrom::Future => {
                let mut d = tgt_wd - cur_wd;
                if d < 0 {
                    d += 7;
                }
                d
            }
            PreferDatesFrom::CurrentPeriod => tgt_wd - cur_wd,
        };
        let target_date = base.date() + chrono::Duration::days(diff);
        let nt = time.unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        return Some(AbsoluteMatch {
            naive: target_date.and_time(nt),
            period: Period::Day,
            locale: loc.code.clone(),
        });
    }

    let base = settings.effective_base();
    let (y, m, d, period) = complete_partial_date(year, month, day, base, settings)?;
    let nd = NaiveDate::from_ymd_opt(y, m, d)?;
    let nt = time.unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    Some(AbsoluteMatch {
        naive: nd.and_time(nt),
        period,
        locale: loc.code.clone(),
    })
}

fn complete_partial_date(
    year: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<(i32, u32, u32, Period)> {
    if settings.strict_parsing && (year.is_none() || month.is_none() || day.is_none()) {
        return None;
    }
    if settings.require_parts.contains(&DatePart::Year) && year.is_none() {
        return None;
    }
    if settings.require_parts.contains(&DatePart::Month) && month.is_none() {
        return None;
    }
    if settings.require_parts.contains(&DatePart::Day) && day.is_none() {
        return None;
    }

    let period = if day.is_some() {
        Period::Day
    } else if month.is_some() {
        Period::Month
    } else {
        Period::Year
    };

    let m = month.unwrap_or_else(|| match settings.prefer_month_of_year {
        PreferMonthOfYear::First => 1,
        PreferMonthOfYear::Last => 12,
        PreferMonthOfYear::Current => base.month(),
    });
    if !(1..=12).contains(&m) {
        return None;
    }

    let mut y = year.unwrap_or_else(|| base.year());
    let d = day.unwrap_or_else(|| resolve_missing_day(y, m, base.day(), settings.prefer_day_of_month));
    if d < 1 || d > days_in_month(y, m) {
        return None;
    }

    // Apply PREFER_DATES_FROM when year was omitted
    if year.is_none() {
        if let Some(cand) = NaiveDate::from_ymd_opt(y, m, d) {
            match settings.prefer_dates_from {
                PreferDatesFrom::Past if cand > base.date() => y -= 1,
                PreferDatesFrom::Future if cand < base.date() => y += 1,
                _ => {}
            }
        }
    }

    let final_d = d.min(days_in_month(y, m));
    Some((y, m, final_d, period))
}

fn split_date_tokens(s: &str) -> Vec<&str> {
    let mut out = Vec::with_capacity(8);
    let bytes = s.as_bytes();
    let mut start = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        let b = bytes[i];
        let is_sep = b == b' ' || b == b'\t' || b == b',' || b == b'/' || b == b'-';
        if is_sep {
            if i > start {
                out.push(&s[start..i]);
            }
            start = i + 1;
        }
        i += 1;
    }
    if start < bytes.len() {
        out.push(&s[start..]);
    }
    out
}

fn parse_date_number_or_ordinal(s: &str) -> Option<(u32, usize)> {
    if s.is_empty() || s.len() > 6 {
        return None;
    }
    // Strip English/French/Spanish/Turkish/German ordinal suffixes: 1st, 2nd, 3rd, 4th, 1er, 2e, 1º, 1ª
    let stripped = s
        .strip_suffix("st")
        .or_else(|| s.strip_suffix("nd"))
        .or_else(|| s.strip_suffix("rd"))
        .or_else(|| s.strip_suffix("th"))
        .or_else(|| s.strip_suffix("er"))
        .or_else(|| s.strip_suffix('º'))
        .or_else(|| s.strip_suffix('ª'))
        .unwrap_or(s);

    if stripped.is_empty() || stripped.len() > 4 || !stripped.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let val: u32 = stripped.parse().ok()?;
    Some((val, stripped.len()))
}

use chrono::Timelike;
