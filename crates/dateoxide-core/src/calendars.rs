use crate::fast_path::parse_time_str;
use crate::locale::{fold_diacritics, normalize_digits_and_lower};
use crate::settings::{DateData, Period, Settings};
use crate::timezone::{extract_trailing_timezone_with_abbrevs, finalize_datetime};
use chrono::{Duration, NaiveDate, NaiveTime};

fn is_jalali_leap(jy: i32) -> bool {
    matches!(jy.rem_euclid(33), 1 | 5 | 9 | 13 | 17 | 22 | 26 | 30)
}

/// Convert a Solar Hijri (Jalali) calendar date to Gregorian `NaiveDate`.
pub fn jalali_to_gregorian(jy: i32, jm: u32, jd: u32) -> Option<NaiveDate> {
    if !(1..=12).contains(&jm) || jd == 0 {
        return None;
    }
    let max_d = if jm <= 6 {
        31
    } else if jm <= 11 {
        30
    } else if is_jalali_leap(jy) {
        30
    } else {
        29
    };
    if jd > max_d {
        return None;
    }

    let jy = jy - 979;
    let jm_z = jm as i32 - 1;
    let jd_z = jd as i32 - 1;
    let mut j_day_no = 365 * jy + jy.div_euclid(33) * 8 + (jy.rem_euclid(33) + 3) / 4;
    for i in 0..jm_z {
        j_day_no += if i < 6 { 31 } else { 30 };
    }
    j_day_no += jd_z;

    let mut g_day_no = j_day_no + 79;
    let mut gy = 1600 + 400 * g_day_no.div_euclid(146097);
    g_day_no = g_day_no.rem_euclid(146097);

    let mut leap = true;
    if g_day_no >= 36525 {
        g_day_no -= 1;
        gy += 100 * (g_day_no / 36524);
        g_day_no %= 36524;
        if g_day_no >= 365 {
            g_day_no += 1;
        } else {
            leap = false;
        }
    }
    gy += 4 * (g_day_no / 1461);
    g_day_no %= 1461;
    if g_day_no >= 366 {
        leap = false;
        g_day_no -= 1;
        gy += g_day_no / 365;
        g_day_no %= 365;
    }

    let g_days_in_month = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut gm = 1u32;
    for &dim in &g_days_in_month {
        if g_day_no < dim {
            break;
        }
        g_day_no -= dim;
        gm += 1;
    }
    let gd = (g_day_no + 1) as u32;
    NaiveDate::from_ymd_opt(gy, gm, gd)
}

/// Convert an Islamic Hijri date to Gregorian `NaiveDate`.
/// Uses the official Saudi Umm al-Qura calendar table for years 1343..=1500 AH
/// (1924-08-01 to 2077-11-15 CE), falling back to Civil Tabular Hijri outside that range.
pub fn hijri_to_gregorian(hy: i32, hm: u32, hd: u32) -> Option<NaiveDate> {
    use crate::umm_al_qura::{UMM_AL_QURA_JDN, UMM_AL_QURA_MAX_YEAR, UMM_AL_QURA_MIN_YEAR};

    if !(1..=12).contains(&hm) || hd == 0 {
        return None;
    }

    if (UMM_AL_QURA_MIN_YEAR..=UMM_AL_QURA_MAX_YEAR).contains(&hy) {
        let idx = ((hy - UMM_AL_QURA_MIN_YEAR) * 12 + (hm as i32 - 1)) as usize;
        if idx + 1 < UMM_AL_QURA_JDN.len() {
            let start_jdn = 2_400_000 + UMM_AL_QURA_JDN[idx];
            let month_len = (UMM_AL_QURA_JDN[idx + 1] - UMM_AL_QURA_JDN[idx]) as u32;
            if hd > month_len {
                return None;
            }
            return julian_day_to_gregorian(start_jdn + hd as i32 - 1);
        }
    }

    hijri_to_gregorian_tabular(hy, hm, hd)
}

/// Civil Tabular Islamic (Hijri) calendar fallback for historical/far-future years.
pub fn hijri_to_gregorian_tabular(hy: i32, hm: u32, hd: u32) -> Option<NaiveDate> {
    if !(1..=12).contains(&hm) || !(1..=30).contains(&hd) {
        return None;
    }
    let is_leap = (11 * hy + 14).rem_euclid(30) < 11;
    let max_d = if hm % 2 == 1 || (hm == 12 && is_leap) {
        30
    } else {
        29
    };
    if hd > max_d {
        return None;
    }

    let jd = ((11 * hy + 3) / 30)
        + 354 * hy
        + 30 * (hm as i32)
        - ((hm as i32 - 1) / 2)
        + (hd as i32)
        + 1948440
        - 385;

    julian_day_to_gregorian(jd)
}

fn julian_day_to_gregorian(jd: i32) -> Option<NaiveDate> {
    let mut l = jd + 68569;
    let n = (4 * l) / 146097;
    l -= (146097 * n + 3) / 4;
    let i = (4000 * (l + 1)) / 1461001;
    l = l - (1461 * i) / 4 + 31;
    let j = (80 * l) / 2447;
    let d = l - (2447 * j) / 80;
    let l2 = j / 11;
    let m = j + 2 - 12 * l2;
    let y = 100 * (n - 49) + i + l2;
    NaiveDate::from_ymd_opt(y, m as u32, d as u32)
}

fn is_french_republican_leap(year: i32) -> bool {
    matches!(year, 3 | 7 | 11 | 15) || (year > 15 && (year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)))
}

/// Convert a French Republican calendar date to Gregorian `NaiveDate`.
pub fn french_republican_to_gregorian(ry: i32, rm: u32, rd: u32) -> Option<NaiveDate> {
    if ry < 1 || !(1..=13).contains(&rm) || rd == 0 {
        return None;
    }
    let max_d = if rm <= 12 {
        30
    } else if is_french_republican_leap(ry) {
        6
    } else {
        5
    };
    if rd > max_d {
        return None;
    }

    let epoch = NaiveDate::from_ymd_opt(1792, 9, 22)?;
    let mut days = 0i64;
    for y in 1..ry {
        days += if is_french_republican_leap(y) { 366 } else { 365 };
    }
    days += ((rm - 1) * 30 + (rd - 1)) as i64;
    epoch.checked_add_signed(Duration::days(days))
}

fn jalali_month_from_token(tok: &str) -> Option<u32> {
    let f = fold_diacritics(tok);
    match f.as_str() {
        "فروردین" | "farvardin" => Some(1),
        "اردیبهشت" | "ordibehesht" => Some(2),
        "خرداد" | "khordad" => Some(3),
        "تیر" | "tir" => Some(4),
        "مرداد" | "mordad" | "امرداد" => Some(5),
        "شهریور" | "shahrivar" => Some(6),
        "مهر" | "mehr" => Some(7),
        "آبان" | "ابان" | "aban" => Some(8),
        "آذر" | "اذر" | "azar" => Some(9),
        "دی" | "dey" | "dai" => Some(10),
        "بهمن" | "bahman" => Some(11),
        "اسفند" | "esfand" | "isfand" => Some(12),
        _ => None,
    }
}

fn hijri_month_from_phrase(tok: &str) -> Option<u32> {
    let f = fold_diacritics(tok);
    match f.as_str() {
        "محرم" | "muharram" => Some(1),
        "صفر" | "safar" => Some(2),
        "ربيع الاول" | "rabi al-awwal" | "rabi al awwal" | "rabi i" => Some(3),
        "ربيع الاخر" | "ربيع الثاني" | "rabi al-thani" | "rabi al thani" | "rabi ii" => Some(4),
        "جمادي الاولي" | "جمادى الاولى" | "jumada al-ula" | "jumada al ula" | "jumada i" => Some(5),
        "جمادي الاخرة" | "جمادي الثانية" | "جمادى الآخرة" | "جمادى الثانية" | "jumada al-thani"
        | "jumada al thani" | "jumada ii" => Some(6),
        "رجب" | "rajab" => Some(7),
        "شعبان" | "shaban" | "sha'ban" => Some(8),
        "رمضان" | "ramadan" => Some(9),
        "شوال" | "shawwal" => Some(10),
        "ذو القعدة" | "ذي القعدة" | "dhu al-qadah" | "dhu al qadah" | "dhul-qadah" => Some(11),
        "ذو الحجة" | "ذي الحجة" | "dhu al-hijjah" | "dhu al hijjah" | "dhul-hijjah" => Some(12),
        _ => None,
    }
}

fn french_republican_month(tok: &str) -> Option<u32> {
    let f = fold_diacritics(tok);
    match f.as_str() {
        "vendemiaire" => Some(1),
        "brumaire" => Some(2),
        "frimaire" => Some(3),
        "nivose" => Some(4),
        "pluviose" => Some(5),
        "ventose" => Some(6),
        "germinal" => Some(7),
        "floreal" => Some(8),
        "prairial" => Some(9),
        "messidor" => Some(10),
        "thermidor" => Some(11),
        "fructidor" => Some(12),
        "sansculottides" | "complementaires" | "jours complementaires" => Some(13),
        _ => None,
    }
}

fn parse_roman_numeral(s: &str) -> Option<i32> {
    if s.is_empty() {
        return None;
    }
    let val = |c: char| -> Option<i32> {
        match c {
            'i' | 'I' => Some(1),
            'v' | 'V' => Some(5),
            'x' | 'X' => Some(10),
            'l' | 'L' => Some(50),
            'c' | 'C' => Some(100),
            _ => None,
        }
    };
    let chars: Vec<char> = s.chars().collect();
    let mut total = 0i32;
    for i in 0..chars.len() {
        let cur = val(chars[i])?;
        if i + 1 < chars.len() {
            let next = val(chars[i + 1])?;
            if cur < next {
                total -= cur;
                continue;
            }
        }
        total += cur;
    }
    if total > 0 { Some(total) } else { None }
}

fn split_optional_trailing_time(s: &str) -> (&str, Option<NaiveTime>) {
    if let Some(idx) = s.rfind(' ') {
        let tail = s[idx + 1..].trim();
        if tail.contains(':') || tail.ends_with("am") || tail.ends_with("pm") {
            if let Some(t) = parse_time_str(tail) {
                return (s[..idx].trim(), Some(t));
            }
        }
    }
    (s, None)
}

fn parse_numeric_ymd_or_dmy(s: &str) -> Option<(i32, u32, u32)> {
    let sep = if s.contains('/') {
        '/'
    } else if s.contains('-') {
        '-'
    } else if s.contains('.') {
        '.'
    } else {
        return None;
    };
    let parts: Vec<&str> = s.split(sep).collect();
    if parts.len() != 3 || parts.iter().any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit())) {
        return None;
    }
    let p0: i32 = parts[0].parse().ok()?;
    let p1: u32 = parts[1].parse().ok()?;
    let p2: u32 = parts[2].parse().ok()?;
    if parts[0].len() == 4 || p0 > 31 {
        Some((p0, p1, p2))
    } else if parts[2].len() == 4 || p2 > 31 {
        Some((p2 as i32, p1, p0 as u32))
    } else {
        None
    }
}

/// Parse a Solar Hijri (Jalali) date string into Gregorian `DateData`.
pub fn parse_jalali(input: &str, settings: &Settings) -> DateData {
    let (without_tz, tz) = extract_trailing_timezone_with_abbrevs(input.trim(), &settings.timezone_abbreviations);
    let norm = normalize_digits_and_lower(without_tz);
    if norm.is_empty() {
        return DateData::empty();
    }
    let (core, opt_time) = split_optional_trailing_time(&norm);
    let has_time = opt_time.is_some();
    let time = opt_time.unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    let ymd = if let Some(triple) = parse_numeric_ymd_or_dmy(core) {
        Some(triple)
    } else {
        let tokens: Vec<&str> = core.split_whitespace().collect();
        let mut month: Option<u32> = None;
        let mut nums: Vec<(i32, usize)> = Vec::new();
        for tok in tokens {
            let clean = tok.trim_matches(|c: char| c == ',' || c == '.');
            if let Some(m) = jalali_month_from_token(clean) {
                month = Some(m);
            } else if clean.bytes().all(|b| b.is_ascii_digit()) && !clean.is_empty() {
                if let Ok(v) = clean.parse::<i32>() {
                    nums.push((v, clean.len()));
                }
            }
        }
        match (month, nums.as_slice()) {
            (Some(m), [(d, l1), (y, l2)]) if *l2 >= 3 && *l1 <= 2 => Some((*y, m, *d as u32)),
            (Some(m), [(y, l1), (d, l2)]) if *l1 >= 3 && *l2 <= 2 => Some((*y, m, *d as u32)),
            _ => None,
        }
    };

    let Some((jy, jm, jd)) = ymd else {
        return DateData::empty();
    };
    let Some(nd) = jalali_to_gregorian(jy, jm, jd) else {
        return DateData::empty();
    };
    let mut parts = vec!["year", "month", "day"];
    if has_time {
        parts.push("time");
    }
    DateData {
        date_obj: Some(finalize_datetime(nd.and_time(time), tz, settings)),
        period: if settings.return_time_as_period && has_time {
            Period::Time
        } else {
            Period::Day
        },
        locale: Some("fa".to_string()),
        parts,
        part_of_day: None,
    }
}

/// Parse an Islamic Hijri date string into Gregorian `DateData`.
pub fn parse_hijri(input: &str, settings: &Settings) -> DateData {
    let (without_tz, tz) = extract_trailing_timezone_with_abbrevs(input.trim(), &settings.timezone_abbreviations);
    let norm = normalize_digits_and_lower(without_tz);
    if norm.is_empty() {
        return DateData::empty();
    }
    let (core, opt_time) = split_optional_trailing_time(&norm);
    let has_time = opt_time.is_some();
    let time = opt_time.unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    let ymd = if let Some(triple) = parse_numeric_ymd_or_dmy(core) {
        Some(triple)
    } else {
        let tokens: Vec<&str> = core
            .split_whitespace()
            .map(|t| t.trim_matches(|c: char| c == ',' || c == '.'))
            .filter(|t| !t.is_empty())
            .collect();
        let mut month: Option<u32> = None;
        let mut nums: Vec<(i32, usize)> = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            if month.is_none() && i + 2 < tokens.len() {
                let tri = format!("{} {} {}", tokens[i], tokens[i + 1], tokens[i + 2]);
                if let Some(m) = hijri_month_from_phrase(&tri) {
                    month = Some(m);
                    i += 3;
                    continue;
                }
            }
            if month.is_none() && i + 1 < tokens.len() {
                let bi = format!("{} {}", tokens[i], tokens[i + 1]);
                if let Some(m) = hijri_month_from_phrase(&bi) {
                    month = Some(m);
                    i += 2;
                    continue;
                }
            }
            if month.is_none() {
                if let Some(m) = hijri_month_from_phrase(tokens[i]) {
                    month = Some(m);
                    i += 1;
                    continue;
                }
            }
            if tokens[i].bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(v) = tokens[i].parse::<i32>() {
                    nums.push((v, tokens[i].len()));
                }
            }
            i += 1;
        }
        match (month, nums.as_slice()) {
            (Some(m), [(d, l1), (y, l2)]) if *l2 >= 3 && *l1 <= 2 => Some((*y, m, *d as u32)),
            (Some(m), [(y, l1), (d, l2)]) if *l1 >= 3 && *l2 <= 2 => Some((*y, m, *d as u32)),
            _ => None,
        }
    };

    let Some((hy, hm, hd)) = ymd else {
        return DateData::empty();
    };
    let Some(nd) = hijri_to_gregorian(hy, hm, hd) else {
        return DateData::empty();
    };
    let mut parts = vec!["year", "month", "day"];
    if has_time {
        parts.push("time");
    }
    DateData {
        date_obj: Some(finalize_datetime(nd.and_time(time), tz, settings)),
        period: if settings.return_time_as_period && has_time {
            Period::Time
        } else {
            Period::Day
        },
        locale: Some("ar".to_string()),
        parts,
        part_of_day: None,
    }
}

/// Parse a Minguo (Taiwan ROC) calendar date string (year = Minguo + 1911).
pub fn parse_minguo(input: &str, settings: &Settings) -> DateData {
    let (without_tz, tz) = extract_trailing_timezone_with_abbrevs(input.trim(), &settings.timezone_abbreviations);
    let mut norm = normalize_digits_and_lower(without_tz);
    for prefix in ["民國", "民国", "roc"] {
        if let Some(rest) = norm.strip_prefix(prefix) {
            norm = rest.trim().to_string();
        }
    }
    if norm.is_empty() {
        return DateData::empty();
    }
    let (core, opt_time) = split_optional_trailing_time(&norm);
    let has_time = opt_time.is_some();
    let time = opt_time.unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());

    let ymd = if let Some((my, mm, md)) = parse_numeric_ymd_or_dmy(core) {
        Some((my + 1911, mm, md))
    } else if core.contains('年') && core.contains('月') && core.contains('日') {
        let mut my: Option<i32> = None;
        let mut mm: Option<u32> = None;
        let mut md: Option<u32> = None;
        let mut buf = String::new();
        for ch in core.chars() {
            if ch.is_ascii_digit() {
                buf.push(ch);
            } else if !buf.is_empty() {
                if let Ok(v) = buf.parse::<i32>() {
                    match ch {
                        '年' => my = Some(v + 1911),
                        '月' => mm = Some(v as u32),
                        '日' | '號' | '号' => md = Some(v as u32),
                        _ => {}
                    }
                }
                buf.clear();
            }
        }
        match (my, mm, md) {
            (Some(y), Some(m), Some(d)) => Some((y, m, d)),
            _ => None,
        }
    } else {
        None
    };

    let Some((gy, gm, gd)) = ymd else {
        return DateData::empty();
    };
    let Some(nd) = NaiveDate::from_ymd_opt(gy, gm, gd) else {
        return DateData::empty();
    };
    let mut parts = vec!["year", "month", "day"];
    if has_time {
        parts.push("time");
    }
    DateData {
        date_obj: Some(finalize_datetime(nd.and_time(time), tz, settings)),
        period: if settings.return_time_as_period && has_time {
            Period::Time
        } else {
            Period::Day
        },
        locale: Some("zh-Hant".to_string()),
        parts,
        part_of_day: None,
    }
}

/// Parse a French Republican calendar date string (e.g. `"14 Germinal an III"`).
pub fn parse_french_republican(input: &str, settings: &Settings) -> DateData {
    let norm = normalize_digits_and_lower(input.trim());
    if norm.is_empty() {
        return DateData::empty();
    }
    let tokens: Vec<&str> = norm
        .split_whitespace()
        .map(|t| t.trim_matches(|c: char| c == ',' || c == '.'))
        .filter(|t| !t.is_empty() && *t != "an" && *t != "de" && *t != "l'an")
        .collect();

    let mut day: Option<u32> = None;
    let mut month: Option<u32> = None;
    let mut year: Option<i32> = None;

    for tok in tokens {
        if month.is_none() {
            if let Some(m) = french_republican_month(tok) {
                month = Some(m);
                continue;
            }
        }
        if tok.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(v) = tok.parse::<i32>() {
                if day.is_none() && (1..=30).contains(&v) {
                    day = Some(v as u32);
                } else if year.is_none() {
                    year = Some(v);
                }
                continue;
            }
        }
        if year.is_none() {
            if let Some(ry) = parse_roman_numeral(tok) {
                year = Some(ry);
            }
        }
    }

    let (Some(ry), Some(rm), Some(rd)) = (year, month, day) else {
        return DateData::empty();
    };
    let Some(nd) = french_republican_to_gregorian(ry, rm, rd) else {
        return DateData::empty();
    };
    let time = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
    DateData {
        date_obj: Some(finalize_datetime(nd.and_time(time), None, settings)),
        period: Period::Day,
        locale: Some("fr".to_string()),
        parts: vec!["year", "month", "day"],
        part_of_day: None,
    }
}
