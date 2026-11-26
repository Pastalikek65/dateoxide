use crate::fast_path::{days_in_month, expand_two_digit_year, parse_time_str, resolve_missing_day};
use crate::locale::{fold_diacritics, CompiledLocale, LocaleRegistry};
use crate::settings::{
    DateOrder, DatePart, PartOfDay, Period, PreferDatesFrom, PreferMonthOfYear, Settings,
    StrictDateOrder,
};
use chrono::{Datelike, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

#[derive(Debug, Clone)]
pub struct AbsoluteMatch {
    pub naive: NaiveDateTime,
    pub period: Period,
    pub locale: String,
    pub has_time: bool,
    pub parts: Vec<&'static str>,
    pub part_of_day: Option<PartOfDay>,
}

/// Try parsing a natural-language absolute or partial date across candidate locales.
pub fn try_parse_absolute(
    normalized: &str,
    candidate_ids: &[usize],
    registry: &LocaleRegistry,
    settings: &Settings,
) -> Option<AbsoluteMatch> {
    if normalized.contains('年')
        || normalized.contains('月')
        || normalized.contains('日')
        || normalized.contains('년')
        || normalized.contains('월')
        || normalized.contains('일')
    {
        if let Some(m) = try_cjk_or_korean_date(normalized, settings) {
            return Some(m);
        }
    }

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
                            has_time: false,
                            parts: vec!["year"],
                            part_of_day: None,
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

fn try_cjk_or_korean_date(s: &str, settings: &Settings) -> Option<AbsoluteMatch> {
    let mut year: Option<i32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;
    let mut hour: u32 = 0;
    let mut minute: u32 = 0;
    let mut second: u32 = 0;
    let mut has_time = false;
    let is_korean = s.contains('년') || s.contains('월') || s.contains('일') || s.contains('시');
    let is_pm = s.contains("오후") || s.contains("pm") || s.contains("下午");
    let is_am = s.contains("오전") || s.contains("am") || s.contains("上午");

    let mut loose_nums: Vec<(u32, usize)> = Vec::new();
    let mut cur_num = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut idx = 0usize;

    while idx < chars.len() {
        let ch = chars[idx];
        if ch.is_ascii_digit() {
            cur_num.push(ch);
            idx += 1;
            continue;
        }
        if !cur_num.is_empty() {
            let val: u32 = cur_num.parse().ok()?;
            let len = cur_num.len();
            cur_num.clear();
            match ch {
                '年' | '년' => year = Some(val as i32),
                '月' | '월' => month = Some(val),
                '日' | '号' | '일' => day = Some(val),
                '時' | '时' | '시' => {
                    hour = val;
                    has_time = true;
                }
                '分' | '분' => {
                    minute = val;
                    has_time = true;
                }
                '秒' | '초' => {
                    second = val;
                    has_time = true;
                }
                ':' => {
                    let byte_idx = s
                        .char_indices()
                        .nth(idx)
                        .map(|(b, _)| b)
                        .unwrap_or(0);
                    let start = s[..byte_idx]
                        .rfind(|c: char| !c.is_ascii_digit())
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    if let Some(t) = parse_time_str(s[start..].trim()) {
                        hour = t.hour();
                        minute = t.minute();
                        second = t.second();
                        has_time = true;
                        break;
                    }
                }
                _ => loose_nums.push((val, len)),
            }
        }
        idx += 1;
    }

    if !cur_num.is_empty() {
        let len = cur_num.len();
        if let Ok(val) = cur_num.parse::<u32>() {
            loose_nums.push((val, len));
        }
    }

    for (val, len) in loose_nums {
        if year.is_none() && len == 4 && (1000..=2999).contains(&val) {
            year = Some(val as i32);
        } else if day.is_none() && (1..=31).contains(&val) && len <= 2 {
            day = Some(val);
        }
    }

    if year.is_none() && month.is_none() {
        return None;
    }

    if has_time {
        if is_pm && hour >= 1 && hour < 12 {
            hour += 12;
        } else if is_am && hour == 12 {
            hour = 0;
        }
    }

    let base = settings.effective_base();
    let (y, m, d, period, parts) = complete_partial_date(year, month, day, has_time, base, settings)?;
    let nd = NaiveDate::from_ymd_opt(y, m, d)?;
    let nt = NaiveTime::from_hms_opt(hour, minute, second)?;
    let lang = if is_korean {
        "ko"
    } else if s.contains('時') {
        "ja"
    } else {
        "zh"
    };

    Some(AbsoluteMatch {
        naive: nd.and_time(nt),
        period,
        locale: lang.to_string(),
        has_time,
        parts,
        part_of_day: None,
    })
}

fn lookup_month(s: &str, loc: &CompiledLocale, normalize: bool) -> Option<u8> {
    if let Some(&m) = loc.months.get(s) {
        return Some(m);
    }
    if normalize {
        let f = fold_diacritics(s);
        if let Some(&m) = loc.folded_months.get(&f) {
            return Some(m);
        }
    }
    None
}

fn lookup_weekday(s: &str, loc: &CompiledLocale, normalize: bool) -> Option<u8> {
    if let Some(&w) = loc.weekdays.get(s) {
        return Some(w);
    }
    if normalize {
        let f = fold_diacritics(s);
        if let Some(&w) = loc.folded_weekdays.get(&f) {
            return Some(w);
        }
    }
    None
}

fn lookup_tod_hour(s: &str, loc: &CompiledLocale, normalize: bool) -> Option<u8> {
    if let Some(&h) = loc.time_of_day.get(s) {
        return Some(h);
    }
    if normalize {
        let f = fold_diacritics(s);
        if let Some(&h) = loc.folded_time_of_day.get(&f).or_else(|| loc.time_of_day.get(&f)) {
            return Some(h);
        }
    }
    None
}

fn tod_hour_to_time_and_pod(hour: u8, settings: &Settings) -> (NaiveTime, Option<PartOfDay>) {
    match hour {
        3 | 6 => (settings.parts_of_day.time_for(PartOfDay::EarlyMorning), Some(PartOfDay::EarlyMorning)),
        9 => (settings.parts_of_day.time_for(PartOfDay::Morning), Some(PartOfDay::Morning)),
        15 => (settings.parts_of_day.time_for(PartOfDay::Afternoon), Some(PartOfDay::Afternoon)),
        18 => (settings.parts_of_day.time_for(PartOfDay::Evening), Some(PartOfDay::Evening)),
        20 => (settings.parts_of_day.time_for(PartOfDay::Night), Some(PartOfDay::Night)),
        12 => (NaiveTime::from_hms_opt(12, 0, 0).unwrap(), None),
        0 => (NaiveTime::from_hms_opt(0, 0, 0).unwrap(), None),
        h => (NaiveTime::from_hms_opt(h as u32, 0, 0).unwrap(), None),
    }
}

fn try_parse_in_locale(
    normalized: &str,
    loc: &CompiledLocale,
    settings: &Settings,
) -> Option<AbsoluteMatch> {
    let raw_tokens = split_date_tokens(normalized, loc, settings.normalize);
    if raw_tokens.is_empty() || raw_tokens.len() > 18 {
        return None;
    }

    let mut year: Option<i32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;
    let mut weekday_seen: Option<u8> = None;
    let mut time: Option<NaiveTime> = None;
    let mut part_of_day: Option<PartOfDay> = None;
    let mut ampm: Option<bool> = None;
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

        if settings
            .skip_tokens
            .iter()
            .any(|st| st.eq_ignore_ascii_case(clean))
        {
            i += 1;
            continue;
        }

        // 1. Apache / Nginx combined year:HH:MM:SS token (e.g. "2026:15:30:00")
        if year.is_none() && time.is_none() && clean.len() >= 12 {
            if let Some((y_part, t_part)) = clean.split_once(':') {
                if y_part.len() == 4 && y_part.bytes().all(|b| b.is_ascii_digit()) {
                    if let (Ok(y_val), Some(t_val)) = (y_part.parse::<i32>(), parse_time_str(t_part)) {
                        if (1000..=2999).contains(&y_val) {
                            year = Some(y_val);
                            time = Some(t_val);
                            i += 1;
                            continue;
                        }
                    }
                }
            }
        }

        // 2. Time token
        if time.is_none()
            && (clean.contains(':')
                || (clean.len() >= 3
                    && clean.as_bytes()[0].is_ascii_digit()
                    && (clean.contains('h')
                        || clean.ends_with("am")
                        || clean.ends_with("pm")
                        || clean.ends_with("a.m")
                        || clean.ends_with("p.m")
                        || tok.ends_with("a.m.")
                        || tok.ends_with("p.m."))))
        {
            let time_cand = if tok.ends_with("a.m.") || tok.ends_with("p.m.") {
                tok.trim_matches(',')
            } else {
                clean
            };
            if i + 1 < raw_tokens.len() {
                let next_clean = raw_tokens[i + 1].trim_matches('.');
                if next_clean == "am"
                    || next_clean == "pm"
                    || loc.am.contains(next_clean)
                    || loc.pm.contains(next_clean)
                {
                    let is_pm = next_clean == "pm" || loc.pm.contains(next_clean);
                    let suffix = if is_pm { "pm" } else { "am" };
                    let combined = format!("{} {}", time_cand, suffix);
                    if let Some(t) = parse_time_str(&combined) {
                        time = Some(t);
                        i += 2;
                        continue;
                    }
                }
            }
            if let Some(t) = parse_time_str(time_cand) {
                time = Some(t);
                i += 1;
                continue;
            }
        }

        // 3. Month name: trigram -> bigram -> unigram
        if month.is_none() {
            if i + 2 < raw_tokens.len() {
                let trigram = format!(
                    "{} {} {}",
                    clean,
                    raw_tokens[i + 1].trim_matches(','),
                    raw_tokens[i + 2].trim_matches(',')
                );
                if let Some(m) = lookup_month(&trigram, loc, settings.normalize) {
                    month = Some(m as u32);
                    matched_locale_signal = true;
                    i += 3;
                    continue;
                }
            }
            if i + 1 < raw_tokens.len() {
                let bigram = format!("{} {}", clean, raw_tokens[i + 1].trim_matches(','));
                if let Some(m) = lookup_month(&bigram, loc, settings.normalize) {
                    month = Some(m as u32);
                    matched_locale_signal = true;
                    i += 2;
                    continue;
                }
            }
            if let Some(m) = lookup_month(clean, loc, settings.normalize) {
                month = Some(m as u32);
                matched_locale_signal = true;
                i += 1;
                continue;
            }
        }

        // 4. AM/PM markers
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

        // 5. Weekday name
        if weekday_seen.is_none() {
            if let Some(wd) = lookup_weekday(clean, loc, settings.normalize) {
                weekday_seen = Some(wd);
                matched_locale_signal = true;
                i += 1;
                continue;
            }
        }

        // 6. Time-of-day word on absolute date ("12 December 2025 morning")
        if time.is_none() {
            if i + 1 < raw_tokens.len() {
                let bigram = format!("{} {}", clean, raw_tokens[i + 1].trim_matches(','));
                if let Some(h) = lookup_tod_hour(&bigram, loc, settings.normalize) {
                    let (t, pod) = tod_hour_to_time_and_pod(h, settings);
                    time = Some(t);
                    part_of_day = pod;
                    i += 2;
                    continue;
                }
            }
            if let Some(h) = lookup_tod_hour(clean, loc, settings.normalize) {
                let (t, pod) = tod_hour_to_time_and_pod(h, settings);
                time = Some(t);
                part_of_day = pod;
                i += 1;
                continue;
            }
        }

        // 7. Numeric token (including ordinals)
        if let Some((num, digit_len)) = parse_date_number_or_ordinal(clean) {
            if digit_len == 4 && (1000..=2999).contains(&num) && year.is_none() {
                year = Some(num as i32);
            } else {
                free_nums.push((num, digit_len));
            }
            i += 1;
            continue;
        }

        // 8. Skip / connector words
        if loc.skip.contains(clean)
            || matches!(
                clean,
                "at" | "on" | "of" | "the" | "in" | "de" | "del" | "el" | "la" | "le" | "à"
                    | "am" | "um" | "den" | "saat" | "tarih" | "tarihinde" | "в" | "г" | "года"
                    | "günü" | "gunu" | "yılında" | "yilinda" | "yılı"
            )
        {
            i += 1;
            continue;
        }

        return None;
    }

    if !matched_locale_signal && year.is_none() {
        return None;
    }

    let has_time = time.is_some();
    if let (Some(t), Some(is_pm)) = (time, ampm) {
        let mut h = t.hour();
        if is_pm && h >= 1 && h < 12 {
            h += 12;
        } else if !is_pm && h == 12 {
            h = 0;
        }
        time = NaiveTime::from_hms_nano_opt(h, t.minute(), t.second(), t.nanosecond());
    }

    let order = if settings.date_order.is_some() || settings.region.is_some() {
        settings.effective_date_order(loc.date_order)
    } else if settings.prefer_locale_date_order {
        loc.date_order
    } else {
        DateOrder::MDY
    };

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
                match order {
                    DateOrder::MDY if (1..=12).contains(n1) && (1..=31).contains(n2) => {
                        month = Some(*n1);
                        day = Some(*n2);
                    }
                    DateOrder::MDY if settings.strict_date_order == StrictDateOrder::All => {
                        return None;
                    }
                    _ if (1..=31).contains(n1) && (1..=12).contains(n2) => {
                        day = Some(*n1);
                        month = Some(*n2);
                    }
                    _ if settings.strict_date_order == StrictDateOrder::All => {
                        return None;
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
        let mut p = vec!["year", "month", "day"];
        if has_time {
            p.push("time");
        }
        return Some(AbsoluteMatch {
            naive: target_date.and_time(nt),
            period: Period::Day,
            locale: loc.code.clone(),
            has_time,
            parts: p,
            part_of_day,
        });
    }

    let base = settings.effective_base();
    let (y, m, d, period, parts) =
        complete_partial_date(year, month, day, has_time, base, settings)?;
    let nd = NaiveDate::from_ymd_opt(y, m, d)?;
    let nt = time.unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    Some(AbsoluteMatch {
        naive: nd.and_time(nt),
        period,
        locale: loc.code.clone(),
        has_time,
        parts,
        part_of_day,
    })
}

fn complete_partial_date(
    year: Option<i32>,
    month: Option<u32>,
    day: Option<u32>,
    has_time: bool,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<(i32, u32, u32, Period, Vec<&'static str>)> {
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

    let mut parts = Vec::with_capacity(4);
    if year.is_some() {
        parts.push("year");
    }
    if month.is_some() {
        parts.push("month");
    }
    if day.is_some() {
        parts.push("day");
    }
    if has_time {
        parts.push("time");
    }

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
    Some((y, m, final_d, period, parts))
}

fn split_date_tokens<'a>(s: &'a str, loc: &CompiledLocale, normalize: bool) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(8);
    for chunk in s.split(|c: char| c.is_whitespace() || c == ',' || c == '/') {
        if chunk.is_empty() {
            continue;
        }
        if chunk.contains('-') {
            let clean = chunk.trim_matches('.');
            if lookup_month(clean, loc, normalize).is_some()
                || lookup_weekday(clean, loc, normalize).is_some()
                || loc.months.keys().any(|k| k.split_whitespace().any(|w| w == clean))
            {
                out.push(chunk);
                continue;
            }
            for sub in chunk.split('-') {
                if !sub.is_empty() {
                    out.push(sub);
                }
            }
        } else {
            out.push(chunk);
        }
    }
    out
}

fn parse_date_number_or_ordinal(s: &str) -> Option<(u32, usize)> {
    if s.is_empty() || s.len() > 6 {
        return None;
    }
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
