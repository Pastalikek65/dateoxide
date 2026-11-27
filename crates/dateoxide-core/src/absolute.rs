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
    try_parse_absolute_with_raw(normalized, None, candidate_ids, registry, settings)
}

/// Like `try_parse_absolute`, also receiving pre-lowercased input for strict uppercase Roman checks.
pub fn try_parse_absolute_with_raw(
    normalized: &str,
    raw_input: Option<&str>,
    candidate_ids: &[usize],
    registry: &LocaleRegistry,
    settings: &Settings,
) -> Option<AbsoluteMatch> {
    if (normalized.contains('年')
        || normalized.contains('月')
        || normalized.contains('日')
        || normalized.contains('년')
        || normalized.contains('월')
        || normalized.contains('일'))
        && (crate::locale::settings_allows_locale(settings, "zh")
            || crate::locale::settings_allows_locale(settings, "ja")
            || crate::locale::settings_allows_locale(settings, "ko"))
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
        if let Some(m) = try_parse_in_locale(normalized, raw_input, loc, settings) {
            return Some(m);
        }
    }

    None
}

fn extract_cjk_era_year(s: &str) -> Option<Result<(i32, String, bool), ()>> {
    const ERAS: [(&str, i32, bool); 7] = [
        ("令和", 2018, true),
        ("平成", 1988, true),
        ("昭和", 1925, true),
        ("大正", 1911, true),
        ("明治", 1867, true),
        ("民國", 1911, false),
        ("民国", 1911, false),
    ];
    for &(era, base, is_jp) in &ERAS {
        if let Some(pos) = s.find(era) {
            let after_era = &s[pos + era.len()..];
            let trimmed = after_era.trim_start();
            let ws_skipped = after_era.len() - trimmed.len();
            let mut cursor = pos + era.len() + ws_skipped;

            let era_num: i32 = if let Some(rest) = trimmed.strip_prefix('元') {
                let rest_trim = rest.trim_start();
                let nen_ws = rest.len() - rest_trim.len();
                if !rest_trim.starts_with('年') {
                    return Some(Err(()));
                }
                cursor += '元'.len_utf8() + nen_ws + '年'.len_utf8();
                1
            } else {
                let mut d_end = 0usize;
                for (i, ch) in trimmed.char_indices() {
                    if ch.is_ascii_digit() {
                        d_end = i + 1;
                    } else {
                        break;
                    }
                }
                let max_digits = if is_jp { 2 } else { 3 };
                let max_val = if is_jp { 99 } else { 999 };
                if d_end == 0 || d_end > max_digits {
                    return Some(Err(()));
                }
                let val: i32 = match trimmed[..d_end].parse() {
                    Ok(v) if (1..=max_val).contains(&v) => v,
                    _ => return Some(Err(())),
                };
                let after_digits = &trimmed[d_end..];
                let rest_trim = after_digits.trim_start();
                let nen_ws = after_digits.len() - rest_trim.len();
                if !rest_trim.starts_with('年') {
                    return Some(Err(()));
                }
                cursor += d_end + nen_ws + '年'.len_utf8();
                val
            };

            let mut rem = String::with_capacity(s.len());
            rem.push_str(&s[..pos]);
            rem.push_str(&s[cursor..]);
            return Some(Ok((base + era_num, rem, is_jp)));
        }
    }
    None
}

fn try_cjk_or_korean_date(s: &str, settings: &Settings) -> Option<AbsoluteMatch> {
    if s.contains("四半期") || s.contains("季度") || s.contains("분기") {
        return None;
    }
    let mut year: Option<i32> = None;
    let mut month: Option<u32> = None;
    let mut day: Option<u32> = None;
    let mut hour: u32 = 0;
    let mut minute: u32 = 0;
    let mut second: u32 = 0;
    let mut has_time = false;
    let mut is_japanese = false;
    let owned_rem;
    let scan_s: &str = match extract_cjk_era_year(s) {
        Some(Ok((gy, rem, era_is_jp))) => {
            year = Some(gy);
            is_japanese = era_is_jp;
            owned_rem = rem;
            &owned_rem
        }
        Some(Err(())) => return None,
        None => s,
    };

    let is_korean =
        scan_s.contains('년') || scan_s.contains('월') || scan_s.contains('일') || scan_s.contains('시');
    let is_pm = scan_s.contains("오후")
        || scan_s.contains("pm")
        || scan_s.contains("下午")
        || scan_s.contains("午後")
        || scan_s.contains("晚上");
    let is_am = scan_s.contains("오전")
        || scan_s.contains("am")
        || scan_s.contains("上午")
        || scan_s.contains("午前")
        || scan_s.contains("早上");

    let mut loose_nums: Vec<(u32, usize)> = Vec::new();
    let mut cur_num = String::new();
    let chars: Vec<(usize, char)> = scan_s.char_indices().collect();
    let mut idx = 0usize;

    while idx < chars.len() {
        let (byte_idx, ch) = chars[idx];
        if ch.is_ascii_digit() {
            cur_num.push(ch);
            idx += 1;
            continue;
        }
        if ch.is_whitespace() {
            // Look ahead past whitespace: if followed by CJK/Korean suffix, keep cur_num attached!
            let mut la = idx + 1;
            while la < chars.len() && chars[la].1.is_whitespace() {
                la += 1;
            }
            if la < chars.len()
                && matches!(
                    chars[la].1,
                    '年' | '년' | '月' | '월' | '日' | '号' | '일' | '時' | '时' | '点' | '點' | '시' | '分' | '분' | '秒' | '초'
                )
            {
                idx += 1;
                continue;
            }
        }
        if !cur_num.is_empty() {
            let val: u32 = cur_num.parse().ok()?;
            let len = cur_num.len();
            cur_num.clear();
            match ch {
                '年' | '년' => {
                    year = Some(if len <= 2 {
                        expand_two_digit_year(val)
                    } else {
                        val as i32
                    });
                }
                '月' | '월' => month = Some(val),
                '日' | '号' | '일' => day = Some(val),
                '時' | '时' | '点' | '點' | '시' => {
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
                    // Handle attached day+hour like "2016年6月2911:30" -> day=29, hour=11
                    if len == 4 && day.is_none() && scan_s.len() - byte_idx <= 32 {
                        let d_cand = val / 100;
                        let h_cand = val % 100;
                        if (1..=31).contains(&d_cand) && h_cand <= 23 {
                            day = Some(d_cand);
                            if let Some(t) = parse_time_str(&format!("{}{}", h_cand, &scan_s[byte_idx..])) {
                                hour = t.hour();
                                minute = t.minute();
                                second = t.second();
                                has_time = true;
                                break;
                            }
                        }
                    }
                    let start = scan_s[..byte_idx]
                        .char_indices()
                        .rev()
                        .find(|&(_, c)| !c.is_ascii_digit())
                        .map(|(p, c)| p + c.len_utf8())
                        .unwrap_or(0);
                    if scan_s.len() - start <= 32 {
                        if let Some(t) = parse_time_str(scan_s[start..].trim()) {
                            hour = t.hour();
                            minute = t.minute();
                            second = t.second();
                            has_time = true;
                            break;
                        }
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

    let mut part_of_day: Option<PartOfDay> = None;
    let mut tod_time: Option<NaiveTime> = None;
    if has_time {
        if is_pm && (1..12).contains(&hour) {
            hour += 12;
        } else if is_am && hour == 12 {
            hour = 0;
        }
    } else {
        let tod_hour: Option<u8> = [
            ("真夜中", 0u8),
            ("午夜", 0),
            ("자정", 0),
            ("早上", 9),
            ("上午", 9),
            ("아침", 9),
            ("오전", 9),
            ("朝", 9),
            ("正午", 12),
            ("中午", 12),
            ("정오", 12),
            ("午後", 15),
            ("下午", 15),
            ("오후", 15),
            ("夕方", 18),
            ("傍晚", 18),
            ("저녁", 18),
            ("晚上", 20),
            ("夜", 20),
            ("밤", 20),
        ]
        .into_iter()
        .find(|(tok, _)| scan_s.contains(tok))
        .map(|(_, h)| h);

        if let Some(h) = tod_hour {
            has_time = true;
            part_of_day = PartOfDay::from_hour(h);
            if h == 0 || h == 12 {
                tod_time = NaiveTime::from_hms_opt(h as u32, 0, 0);
            } else if let Some(p) = part_of_day {
                tod_time = Some(settings.parts_of_day.time_for(p));
            }
        }
    }

    let base = settings.effective_base();
    let (y, m, d, period, parts) = complete_partial_date(year, month, day, has_time, base, settings)?;
    let nd = NaiveDate::from_ymd_opt(y, m, d)?;
    let nt = tod_time.or_else(|| NaiveTime::from_hms_opt(hour, minute, second))?;
    let lang = if is_korean {
        "ko"
    } else if is_japanese
        || scan_s.contains('時')
        || scan_s.contains('朝')
        || scan_s.contains('夜')
        || scan_s.contains("夕方")
        || scan_s.contains("正午")
        || scan_s.contains("真夜中")
        || settings
            .languages
            .as_deref()
            .map(|ls| {
                ls.iter().any(|l| l.eq_ignore_ascii_case("ja"))
                    && !ls.iter().any(|l| l.to_ascii_lowercase().starts_with("zh"))
            })
            .unwrap_or(false)
    {
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
        part_of_day,
    })
}

fn lookup_month(s: &str, loc: &CompiledLocale, normalize: bool) -> Option<u8> {
    if let Some(&m) = loc.months.get(s) {
        return Some(m);
    }
    if normalize {
        let f = fold_diacritics(s);
        if let Some(&m) = loc.folded_months.get(f.as_ref()) {
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
        if let Some(&w) = loc.folded_weekdays.get(f.as_ref()) {
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
        if let Some(&h) = loc
            .folded_time_of_day
            .get(f.as_ref())
            .or_else(|| loc.time_of_day.get(f.as_ref()))
        {
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

fn raw_has_uppercase_roman(raw_input: Option<&str>, clean_lower: &str) -> Option<u32> {
    let raw = raw_input?;
    if !clean_lower
        .bytes()
        .all(|b| matches!(b, b'i' | b'v' | b'x' | b'l' | b'c' | b'd' | b'm'))
    {
        return None;
    }
    for tok in raw.split(|c: char| !c.is_ascii_alphanumeric()) {
        if tok.eq_ignore_ascii_case(clean_lower) && tok.bytes().all(|b| b.is_ascii_uppercase()) {
            if let Some(v) = crate::numerals::parse_roman_numeral(tok) {
                return Some(v);
            }
        }
    }
    None
}

fn try_parse_in_locale(
    normalized: &str,
    raw_input: Option<&str>,
    loc: &CompiledLocale,
    settings: &Settings,
) -> Option<AbsoluteMatch> {
    let raw_tokens = split_date_tokens(normalized, loc, settings.normalize);
    if raw_tokens.is_empty() || raw_tokens.len() > 18 {
        return None;
    }

    let mut year: Option<i32> = None;
    let mut year_expanded_from_two = false;
    let mut year_pos: Option<usize> = None;
    let mut month: Option<u32> = None;
    let mut month_pos: Option<usize> = None;
    let mut month_token_len: usize = 0;
    let mut month_token_ascii: bool = false;
    let mut day: Option<u32> = None;
    let mut day_pos: Option<usize> = None;
    let mut weekday_seen: Option<u8> = None;
    let mut time: Option<NaiveTime> = None;
    let mut part_of_day: Option<PartOfDay> = None;
    let mut ampm: Option<bool> = None;
    let mut free_nums: Vec<(u32, usize, usize)> = Vec::new();
    let mut roman_cands: Vec<(u32, usize)> = Vec::new();
    let mut matched_locale_signal = false;
    let mut is_thai_be = false;
    let mut ambiguous_month_weekday: Option<u8> = None;

    let mut i = 0;
    while i < raw_tokens.len() {
        let tok = raw_tokens[i];
        let clean = tok.trim_matches(|c: char| matches!(c, ',' | '.' | '(' | ')' | '[' | ']'));
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
                        if (1..=9999).contains(&y_val) {
                            year = Some(y_val);
                            year_pos = Some(i);
                            time = Some(t_val);
                            i += 1;
                            continue;
                        }
                    }
                }
            }
        }

        // 2. Time token (including dot-separated HH.MM with optional German Uhr / Finnish ip.)
        if time.is_none()
            && (clean.contains(':')
                || (clean.len() >= 4
                    && clean.contains('.')
                    && clean.as_bytes()[0].is_ascii_digit()
                    && clean.matches('.').count() == 1)
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
                let next_clean = raw_tokens[i + 1].trim_matches(|c: char| c == '.' || c == ',');
                if next_clean == "am"
                    || next_clean == "pm"
                    || next_clean == "a.m"
                    || next_clean == "p.m"
                    || next_clean == "ip"
                    || next_clean == "ap"
                    || next_clean == "uhr"
                    || loc.am.contains(next_clean)
                    || loc.pm.contains(next_clean)
                    || (settings.normalize
                        && (loc.folded_am.contains(next_clean)
                            || loc.folded_pm.contains(next_clean)))
                {
                    let combined = format!("{} {}", time_cand, raw_tokens[i + 1].trim_matches(','));
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
        // Also re-evaluates if an earlier month token was actually an ambiguous weekday (e.g. Spanish "mar, 16 jun 2020").
        if month.is_none() || (weekday_seen.is_none() && ambiguous_month_weekday.is_some()) {
            let mut matched_span = 0usize;
            for span in (1..=5).rev() {
                if i + span <= raw_tokens.len() {
                    let cand = if span == 1 {
                        clean.to_string()
                    } else {
                        let mut s = String::from(clean);
                        for k in 1..span {
                            s.push(' ');
                            s.push_str(raw_tokens[i + k].trim_matches(','));
                        }
                        s
                    };
                    if let Some(m) = lookup_month(&cand, loc, settings.normalize) {
                        if month.is_none() {
                            ambiguous_month_weekday = if weekday_seen.is_none() {
                                lookup_weekday(&cand, loc, settings.normalize)
                            } else {
                                None
                            };
                        } else if lookup_weekday(&cand, loc, settings.normalize).is_some() {
                            continue;
                        } else if let Some(prev_wd) = ambiguous_month_weekday.take() {
                            weekday_seen = Some(prev_wd);
                        }
                        month = Some(m as u32);
                        month_pos = Some(i);
                        month_token_len = cand.chars().count();
                        month_token_ascii = cand.is_ascii();
                        matched_locale_signal = true;
                        matched_span = span;
                        break;
                    }
                }
            }
            if matched_span > 0 {
                i += matched_span;
                continue;
            }
        }

        // 4. AM/PM markers (never mistake German preposition "am" = "on" for English a.m.)
        if clean == "pm"
            || clean == "p.m"
            || loc.pm.contains(clean)
            || (settings.normalize && loc.folded_pm.contains(clean))
        {
            ampm = Some(true);
            i += 1;
            continue;
        }
        let is_germanic_prep_am = clean == "am"
            && matches!(loc.code.as_str(), "de" | "gsw" | "lb" | "ksh" | "nds");
        if !is_germanic_prep_am
            && (clean == "am"
                || clean == "a.m"
                || loc.am.contains(clean)
                || (settings.normalize && loc.folded_am.contains(clean)))
        {
            ampm = Some(false);
            i += 1;
            continue;
        }

        // 5. Weekday name: match up to 4-word phrases -> unigram
        if weekday_seen.is_none() {
            let mut matched_span = 0usize;
            for span in (1..=4).rev() {
                if i + span <= raw_tokens.len() {
                    let cand = if span == 1 {
                        clean.to_string()
                    } else {
                        let mut s = String::from(clean);
                        for k in 1..span {
                            s.push(' ');
                            s.push_str(raw_tokens[i + k].trim_matches(','));
                        }
                        s
                    };
                    if let Some(wd) = lookup_weekday(&cand, loc, settings.normalize) {
                        weekday_seen = Some(wd);
                        matched_locale_signal = true;
                        matched_span = span;
                        break;
                    }
                }
            }
            if matched_span > 0 {
                i += matched_span;
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

        // 6b. Apostrophe 2-digit year (e.g. "Nov 26 '26", "15 Jan ’99")
        if year.is_none() {
            if let Some(rest) = clean
                .strip_prefix('\'')
                .or_else(|| clean.strip_prefix('\u{2019}'))
                .or_else(|| clean.strip_prefix('\u{2018}'))
            {
                if rest.len() == 2 && rest.bytes().all(|b| b.is_ascii_digit()) {
                    if let Ok(y2) = rest.parse::<u32>() {
                        year = Some(expand_two_digit_year(y2));
                        year_pos = Some(i);
                        year_expanded_from_two = true;
                        i += 1;
                        continue;
                    }
                }
            }
        }

        // 6c. Word ordinals (must run BEFORE step 8 unit skip so "second of June 2024" -> day=2, not TimeUnit::Second)
        if day.is_none() {
            if let Some(ord) = crate::numerals::lookup_ordinal(clean, loc, settings.normalize) {
                if (1..=31).contains(&ord) {
                    day = Some(ord);
                    day_pos = Some(i);
                    matched_locale_signal = true;
                    i += 1;
                    continue;
                }
            }
        }

        // 7. Numeric token (including ordinals)
        if let Some((num, digit_len)) = parse_date_number_or_ordinal(clean) {
            if digit_len == 4 && (1..=9999).contains(&num) && year.is_none() {
                year = Some(num as i32);
                year_pos = Some(i);
            } else {
                free_nums.push((num, digit_len, i));
            }
            i += 1;
            continue;
        }

        // 7b. Strictly gated uppercase Roman numerals (only in genuine date expressions like "15 V 2024", "XV May MMXXIV")
        if raw_tokens.len() >= 2 {
            if let Some(rv) = raw_has_uppercase_roman(raw_input, clean) {
                roman_cands.push((rv, i));
                i += 1;
                continue;
            }
        }

        // 8. Skip / connector words (including localized "year" markers & locale-gated European connectors)
        if loc.code == "th" && matches!(clean, "พ.ศ" | "พศ") {
            is_thai_be = true;
            matched_locale_signal = true;
            i += 1;
            continue;
        }
        if loc.skip.contains(clean)
            || loc.units.contains_key(clean)
            || is_locale_connector(loc.code.as_str(), clean)
            || matches!(
                clean,
                "at" | "on" | "of" | "the" | "in" | "de" | "del" | "el" | "la" | "le" | "à"
                    | "am" | "um" | "den" | "saat" | "tarih" | "tarihinde" | "в" | "г" | "года"
                    | "год" | "günü" | "gunu" | "yılında" | "yilinda" | "yılı" | "year" | "klo"
                    | "u" | "у" | "سنة" | "år"
            )
        {
            i += 1;
            continue;
        }

        return None;
    }

    if !roman_cands.is_empty() {
        let mut used_roman_month = false;
        // Pass 1: 4-digit/century year Roman numerals (e.g. MMXXIV)
        for &(rv, rpos) in &roman_cands {
            if year.is_none() && (100..=3999).contains(&rv) {
                year = Some(rv as i32);
                year_pos = Some(rpos);
            }
        }
        if settings.languages.is_none() && year.is_none() {
            return None;
        }
        // Pass 2: Roman month (1..=12) when month is still missing and anchored by day/year/free_nums
        for &(rv, rpos) in &roman_cands {
            if year_pos == Some(rpos) {
                continue;
            }
            if month.is_none()
                && (1..=12).contains(&rv)
                && (year.is_some() || day.is_some() || !free_nums.is_empty())
            {
                month = Some(rv);
                month_pos = Some(rpos);
                matched_locale_signal = true;
                used_roman_month = true;
                break;
            }
        }
        // Pass 3: Roman day (1..=31)
        for &(rv, rpos) in &roman_cands {
            if year_pos == Some(rpos) || (used_roman_month && month_pos == Some(rpos)) {
                continue;
            }
            if day.is_none() && (1..=31).contains(&rv) {
                free_nums.push((rv, 2, rpos));
            } else {
                return None;
            }
        }
    }

    if !matched_locale_signal && year.is_none() {
        return None;
    }

    if loc.code == "th" {
        if let Some(y) = year {
            if (is_thai_be && y > 543) || (2400..=2700).contains(&y) {
                year = Some(y - 543);
            }
        }
    }

    let has_time = time.is_some();
    if let (Some(t), Some(is_pm)) = (time, ampm) {
        let mut h = t.hour();
        if h == 0 || h > 12 {
            return None;
        }
        if is_pm && h < 12 {
            h += 12;
        } else if !is_pm && h == 12 {
            h = 0;
        }
        time = NaiveTime::from_hms_nano_opt(h, t.minute(), t.second(), t.nanosecond());
    }

    let year_first = year.is_some()
        && raw_tokens
            .first()
            .and_then(|t| parse_date_number_or_ordinal(t))
            .map(|(n, l)| l == 4 && n as i32 == year.unwrap())
            .unwrap_or(false);

    let order = if settings.date_order.is_some() || settings.region.is_some() {
        settings.effective_date_order(loc.date_order)
    } else if year_first {
        DateOrder::YMD
    } else if settings.prefer_locale_date_order {
        loc.date_order
    } else {
        DateOrder::MDY
    };

    match free_nums.as_slice() {
        [] => {}
        [(n, len, pos)] => {
            if month.is_some() && day.is_none() && (1..=31).contains(n) && *len <= 2 {
                day = Some(*n);
                day_pos = Some(*pos);
            } else if year.is_none() && *len == 2 && weekday_seen.is_none() {
                year = Some(expand_two_digit_year(*n));
                year_pos = Some(*pos);
                year_expanded_from_two = true;
            } else {
                return None;
            }
        }
        [(n1, l1, p1), (n2, l2, p2)] => {
            if month.is_some() && day.is_none() && year.is_none() {
                if (1..=31).contains(n1) && *l1 <= 2 && *l2 == 2 {
                    day = Some(*n1);
                    day_pos = Some(*p1);
                    year = Some(expand_two_digit_year(*n2));
                    year_pos = Some(*p2);
                    year_expanded_from_two = true;
                } else if (1..=31).contains(n2) && *l2 <= 2 && *l1 == 2 {
                    year = Some(expand_two_digit_year(*n1));
                    year_pos = Some(*p1);
                    year_expanded_from_two = true;
                    day = Some(*n2);
                    day_pos = Some(*p2);
                } else if (1..=31).contains(n1) && (1..=31).contains(n2) && n1 <= n2 {
                    // Range without year: "June 12-14"
                    day = Some(*n1);
                    day_pos = Some(*p1);
                } else {
                    return None;
                }
            } else if month.is_some()
                && year.is_some()
                && day.is_none()
                && (1..=31).contains(n1)
                && (1..=31).contains(n2)
                && n1 <= n2
            {
                // Date range like "June 12-14, 2021" or "12-14 June 2021" -> take start day
                day = Some(*n1);
                day_pos = Some(*p1);
            } else if month.is_none() && day.is_none() && weekday_seen.is_none() {
                match order {
                    DateOrder::YMD | DateOrder::MDY
                        if (1..=12).contains(n1) && (1..=31).contains(n2) =>
                    {
                        month = Some(*n1);
                        month_pos = Some(*p1);
                        day = Some(*n2);
                        day_pos = Some(*p2);
                    }
                    DateOrder::YMD | DateOrder::MDY
                        if settings.strict_date_order == StrictDateOrder::All =>
                    {
                        return None;
                    }
                    _ if (1..=31).contains(n1) && (1..=12).contains(n2) => {
                        day = Some(*n1);
                        day_pos = Some(*p1);
                        month = Some(*n2);
                        month_pos = Some(*p2);
                    }
                    _ if settings.strict_date_order == StrictDateOrder::All => {
                        return None;
                    }
                    _ if (1..=12).contains(n1) && (1..=31).contains(n2) => {
                        month = Some(*n1);
                        month_pos = Some(*p1);
                        day = Some(*n2);
                        day_pos = Some(*p2);
                    }
                    _ => return None,
                }
            } else {
                return None;
            }
        }
        _ => return None,
    }

    if settings.languages.is_none()
        && !crate::locale::is_major_auto_locale(&loc.code)
        && time.is_none()
        && month.is_some()
        && month_token_ascii
        && month_token_len <= 3
        && ((day.is_some() && year.is_none()) || year_expanded_from_two)
    {
        return None;
    }

    if settings.strict_date_order == StrictDateOrder::All {
        if let (Some(yp), Some(mp), Some(dp)) = (year_pos, month_pos, day_pos) {
            let order_ok = match order {
                DateOrder::YMD => yp < mp && mp < dp,
                DateOrder::YDM => yp < dp && dp < mp,
                DateOrder::MDY => mp < dp && dp < yp,
                DateOrder::DMY => dp < mp && mp < yp,
                DateOrder::MYD => mp < yp && yp < dp,
                DateOrder::DYM => dp < yp && yp < mp,
            };
            if !order_ok {
                return None;
            }
        }
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
                if d >= 0 {
                    d -= 7;
                }
                d
            }
            PreferDatesFrom::CurrentPeriod => {
                let mut d = tgt_wd - cur_wd;
                if d > 0 {
                    d -= 7;
                }
                d
            }
            PreferDatesFrom::Future => {
                let mut d = tgt_wd - cur_wd;
                if d <= 0 {
                    d += 7;
                }
                d
            }
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
    if year.is_none() && m == 2 && d == 29 {
        match settings.prefer_dates_from {
            PreferDatesFrom::Past | PreferDatesFrom::CurrentPeriod => {
                while days_in_month(y, 2) < 29
                    || NaiveDate::from_ymd_opt(y, 2, 29).is_none_or(|nd| nd > base.date())
                {
                    y -= 1;
                    if y < 1 {
                        return None;
                    }
                }
            }
            PreferDatesFrom::Future => {
                while days_in_month(y, 2) < 29
                    || NaiveDate::from_ymd_opt(y, 2, 29).is_none_or(|nd| nd <= base.date())
                {
                    y += 1;
                    if y > 9999 {
                        return None;
                    }
                }
            }
        }
        return Some((y, m, d, period, parts));
    }

    if d < 1 || d > days_in_month(y, m) {
        return None;
    }

    if year.is_none() {
        if let Some(cand) = NaiveDate::from_ymd_opt(y, m, d) {
            match settings.prefer_dates_from {
                PreferDatesFrom::Past if cand > base.date() => y -= 1,
                PreferDatesFrom::Future
                    if cand < base.date() || (day.is_some() && cand == base.date()) =>
                {
                    y += 1;
                }
                _ => {}
            }
        }
    }

    let final_d = d.min(days_in_month(y, m));
    Some((y, m, final_d, period, parts))
}

fn split_date_tokens<'a>(s: &'a str, loc: &CompiledLocale, normalize: bool) -> Vec<&'a str> {
    let mut out = Vec::with_capacity(8);
    for chunk in s.split(|c: char| {
        c.is_whitespace() || c == ',' || c == '/' || c == '[' || c == ']' || c == '–' || c == '—'
    }) {
        if chunk.is_empty() {
            continue;
        }
        // Split dot-separated month expressions like "08.Haziran.2014" or "2005г.", but keep fractional times like "13:11:23.678" and Thai BE "พ.ศ." intact
        if chunk.contains('.')
            && chunk.trim_end_matches('.') != "พ.ศ"
            && !chunk.contains(':')
            && !chunk.ends_with("a.m.")
            && !chunk.ends_with("p.m.")
            && !(chunk.matches('.').count() == 1
                && chunk.chars().all(|c| c.is_ascii_digit() || c == '.'))
        {
            let clean = chunk.trim_matches('.');
            if lookup_month(clean, loc, normalize).is_some()
                || lookup_weekday(clean, loc, normalize).is_some()
            {
                out.push(clean);
                continue;
            }
            for sub in chunk.split('.') {
                if !sub.is_empty() {
                    push_splitting_attached_digits(sub, loc, normalize, &mut out);
                }
            }
            continue;
        }
        if chunk.contains('-') {
            let clean = chunk.trim_matches('.');
            if parse_date_number_or_ordinal(clean).is_some()
                || lookup_month(clean, loc, normalize).is_some()
                || lookup_weekday(clean, loc, normalize).is_some()
                || crate::numerals::lookup_ordinal(clean, loc, normalize).is_some()
                || loc.months.keys().any(|k| k.split_whitespace().any(|w| w == clean))
            {
                out.push(chunk);
                continue;
            }
            for sub in chunk.split('-') {
                if !sub.is_empty() {
                    push_splitting_attached_digits(sub, loc, normalize, &mut out);
                }
            }
        } else {
            push_splitting_attached_digits(chunk, loc, normalize, &mut out);
        }
    }
    out
}

#[inline]
fn is_locale_connector(lang: &str, clean: &str) -> bool {
    match lang {
        "it" => matches!(clean, "il" | "alle" | "ore" | "nel" | "della" | "all" | "di" | "lo"),
        "es" => matches!(clean, "a" | "las" | "al" | "horas"),
        "pt" => matches!(clean, "às" | "as" | "em" | "no" | "na" | "dia" | "horas"),
        "fr" => matches!(clean, "au" | "du" | "les" | "heures" | "vers"),
        "nl" => matches!(clean, "op" | "om" | "uur"),
        "pl" => matches!(clean, "dnia" | "o" | "godz" | "roku" | "r"),
        "sv" | "nb" | "da" | "nn" => matches!(clean, "kl" | "klockan" | "klokken"),
        "ro" => matches!(clean, "la" | "ora" | "orele" | "în" | "in" | "pe" | "din"),
        "sk" => matches!(clean, "o" | "v" | "vo" | "dňa" | "dna" | "hod"),
        "cs" => matches!(clean, "v" | "ve" | "hod" | "dne"),
        "hr" | "bs" | "sr" | "sr-Latn" | "sl" => matches!(clean, "u" | "ob" | "sati" | "dana"),
        "et" => matches!(clean, "kell" | "kl"),
        "lt" => matches!(clean, "m" | "d" | "val"),
        "lv" => matches!(clean, "gada" | "g" | "plkst"),
        "uk" => matches!(clean, "р" | "о" | "об" | "року"),
        "bg" => matches!(clean, "г" | "в" | "ч" | "часа"),
        "el" => matches!(clean, "στις" | "την" | "τον"),
        "id" | "ms" => matches!(clean, "pukul" | "jam" | "pada"),
        "vi" => matches!(clean, "ngày" | "ngay" | "lúc" | "luc" | "vào" | "vao"),
        "ar" => matches!(clean, "الساعة" | "ساعة" | "في" | "يوم" | "بتاريخ"),
        _ => false,
    }
}

fn push_splitting_attached_digits<'a>(
    tok: &'a str,
    loc: &CompiledLocale,
    normalize: bool,
    out: &mut Vec<&'a str>,
) {
    // Keep apostrophe 2-digit years ('26, ’99), pure numbers, times, ordinals, and known month/weekday names intact
    if let Some(rest) = tok
        .strip_prefix('\'')
        .or_else(|| tok.strip_prefix('\u{2019}'))
        .or_else(|| tok.strip_prefix('\u{2018}'))
    {
        if rest.len() == 2 && rest.bytes().all(|b| b.is_ascii_digit()) {
            out.push(tok);
            return;
        }
    }
    if tok.contains(':')
        || tok.ends_with("am")
        || tok.ends_with("pm")
        || tok.ends_with("a.m.")
        || tok.ends_with("p.m.")
        || tok.bytes().all(|b| b.is_ascii_digit())
        || parse_date_number_or_ordinal(tok).is_some()
        || lookup_month(tok, loc, normalize).is_some()
        || lookup_weekday(tok, loc, normalize).is_some()
    {
        out.push(tok);
        return;
    }
    // Check if token mixes digits and non-digits (e.g. "2005г", "12Dec2025")
    let has_digit = tok.chars().any(|c| c.is_ascii_digit());
    let has_alpha = tok.chars().any(|c| c.is_alphabetic());
    if !has_digit || !has_alpha {
        out.push(tok);
        return;
    }
    // Keep French/European "14h30" intact
    if tok.contains('h') && tok.chars().all(|c| c.is_ascii_digit() || c == 'h' || c == 'm' || c == 's') {
        out.push(tok);
        return;
    }
    let mut start = 0usize;
    let mut prev_digit = tok.chars().next().unwrap().is_ascii_digit();
    for (idx, ch) in tok.char_indices().skip(1) {
        let cur_digit = ch.is_ascii_digit();
        if cur_digit != prev_digit {
            out.push(&tok[start..idx]);
            start = idx;
            prev_digit = cur_digit;
        }
    }
    if start < tok.len() {
        out.push(&tok[start..]);
    }
}

#[inline]
fn parse_date_number_or_ordinal(s: &str) -> Option<(u32, usize)> {
    crate::numerals::parse_date_number_or_ordinal(s)
}
