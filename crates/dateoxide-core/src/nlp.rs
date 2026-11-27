//! Phase 4 conversational NLP parser: period boundaries, quarters, seasons,
//! chained & ordinal relatives, business days, indefinite/fractional offsets,
//! conversational clocks, and compound day + time-of-day expressions.

use crate::fast_path::{days_in_month, parse_time_str, resolve_time_only_date};
use crate::locale::{fold_diacritics, settings_allows_locale, LocaleRegistry, TimeUnit};
use crate::relative::{apply_relative_offset, RelativeMatch};
use crate::settings::{ParserKind, Period, PreferDatesFrom, Settings};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};

/// Try parsing advanced conversational NLP date/time expressions.
pub fn try_parse_nlp(normalized: &str, settings: &Settings) -> Option<RelativeMatch> {
    let base = settings.effective_base();
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return None;
    }
    let folded_cow = fold_diacritics(trimmed);
    let s = folded_cow.as_ref();

    let allow_abs = settings.allows_parser(ParserKind::AbsoluteTime);
    let allow_rel = settings.allows_parser(ParserKind::RelativeTime);

    // 1. Quarters & Seasons (AbsoluteTime || RelativeTime)
    if allow_abs || allow_rel {
        if let Some(m) = try_quarters_and_seasons(s, trimmed, base, settings) {
            return Some(m);
        }
        if let Some(m) = try_ordinal_weekday_of_month(s, base, settings) {
            return Some(m);
        }
    }

    if !allow_rel {
        return None;
    }

    // 2. Anchored period boundaries
    if let Some(m) = try_period_boundaries(s, trimmed, base, settings) {
        return Some(m);
    }

    // 3. Business / working days
    if let Some(m) = try_business_days(s, trimmed, base, settings) {
        return Some(m);
    }

    // 4. Chained & over-next relatives
    if let Some(m) = try_chained_relatives(s, trimmed, base, settings) {
        return Some(m);
    }

    // 5. Indefinite & multilingual fractionals (birkaç, полтора, 1時間半前, 3点半, 3시 반)
    if let Some(m) = try_indefinite_and_cjk_fractionals(s, trimmed, base, settings) {
        return Some(m);
    }

    // 6. Conversational clocks & compound day + TOD + time
    if let Some(m) = try_conversational_clocks_ext(s, trimmed, base, settings) {
        return Some(m);
    }

    None
}

fn matched_day(naive: NaiveDateTime, locale: &str, has_time: bool) -> RelativeMatch {
    RelativeMatch {
        naive,
        period: Period::Day,
        locale: locale.to_string(),
        has_time,
        parts: if has_time {
            vec!["year", "month", "day", "time"]
        } else {
            vec!["year", "month", "day"]
        },
        part_of_day: None,
    }
}

fn pick_allowed_locale<'a>(settings: &Settings, preferred: &[&'a str]) -> Option<&'a str> {
    preferred
        .iter()
        .find(|&&code| settings_allows_locale(settings, code))
        .copied()
}

fn try_quarters_and_seasons(
    s: &str,
    orig: &str,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<RelativeMatch> {
    // CJK & Korean quarters
    // Japanese: "2026年第2四半期" or "第2四半期"
    if (orig.contains("四半期")) && settings_allows_locale(settings, "ja") {
        if let Some((y, q)) = parse_cjk_quarter(orig, "四半期") {
            let m = (q - 1) * 3 + 1;
            let nd = NaiveDate::from_ymd_opt(y.unwrap_or_else(|| base.year()), m, 1)?;
            return Some(RelativeMatch {
                naive: nd.and_hms_opt(0, 0, 0)?,
                period: Period::Month,
                locale: "ja".to_string(),
                has_time: false,
                parts: vec!["year", "month"],
                part_of_day: None,
            });
        }
    }
    // Chinese: "2026年第2季度" or "第2季度"
    if orig.contains("季度") && settings_allows_locale(settings, "zh") {
        if let Some((y, q)) = parse_cjk_quarter(orig, "季度") {
            let m = (q - 1) * 3 + 1;
            let nd = NaiveDate::from_ymd_opt(y.unwrap_or_else(|| base.year()), m, 1)?;
            return Some(RelativeMatch {
                naive: nd.and_hms_opt(0, 0, 0)?,
                period: Period::Month,
                locale: "zh".to_string(),
                has_time: false,
                parts: vec!["year", "month"],
                part_of_day: None,
            });
        }
    }
    // Korean: "2026년 2분기" / "2분기 2026"
    if orig.contains("분기") && settings_allows_locale(settings, "ko") {
        let mut y: Option<i32> = None;
        let mut q: Option<u32> = None;
        for tok in orig.split_whitespace() {
            let clean = tok.trim_end_matches('년');
            if let Some(q_str) = clean.strip_suffix("분기") {
                if let Ok(qv) = q_str.parse::<u32>() {
                    if (1..=4).contains(&qv) {
                        q = Some(qv);
                    }
                }
            } else if clean.len() == 4 && clean.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(yv) = clean.parse::<i32>() {
                    if (1..=9999).contains(&yv) {
                        y = Some(yv);
                    }
                }
            }
        }
        if let Some(qv) = q {
            let m = (qv - 1) * 3 + 1;
            let nd = NaiveDate::from_ymd_opt(y.unwrap_or_else(|| base.year()), m, 1)?;
            return Some(RelativeMatch {
                naive: nd.and_hms_opt(0, 0, 0)?,
                period: Period::Month,
                locale: "ko".to_string(),
                has_time: false,
                parts: vec!["year", "month"],
                part_of_day: None,
            });
        }
    }

    // ISO / English Q1..Q4: "Q1 2026", "2026-Q4", "2026 Q2", "Q3"
    let toks: Vec<&str> = s
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|t| !t.is_empty())
        .collect();
    if (1..=2).contains(&toks.len()) {
        let mut y: Option<i32> = None;
        let mut q: Option<u32> = None;
        let mut valid = true;
        for &t in &toks {
            if t.len() == 2 && t.starts_with('q') {
                if let Some(d) = t.as_bytes()[1].checked_sub(b'0') {
                    if (1..=4).contains(&d) {
                        q = Some(d as u32);
                        continue;
                    }
                }
            }
            if t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()) {
                if let Ok(yv) = t.parse::<i32>() {
                    if (1..=9999).contains(&yv) {
                        y = Some(yv);
                        continue;
                    }
                }
            }
            valid = false;
            break;
        }
        if valid {
            if let Some(qv) = q {
                let loc = if settings_allows_locale(settings, "en") {
                    "en"
                } else {
                    settings.languages.as_ref().and_then(|l| l.first())?.as_str()
                };
                let m = (qv - 1) * 3 + 1;
                let nd = NaiveDate::from_ymd_opt(y.unwrap_or_else(|| base.year()), m, 1)?;
                return Some(RelativeMatch {
                    naive: nd.and_hms_opt(0, 0, 0)?,
                    period: Period::Month,
                    locale: loc.to_string(),
                    has_time: false,
                    parts: vec!["year", "month"],
                    part_of_day: None,
                });
            }
        }
    }

    // Turkish "2026 3. çeyrek" / German "3. Quartal 2026"
    if (2..=3).contains(&toks.len()) {
        let has_tr = toks.contains(&"ceyrek") && settings_allows_locale(settings, "tr");
        let has_de = toks.contains(&"quartal") && settings_allows_locale(settings, "de");
        if has_tr || has_de {
            let mut y: Option<i32> = None;
            let mut q: Option<u32> = None;
            let mut ok = true;
            for &t in &toks {
                if t == "ceyrek" || t == "quartal" {
                    continue;
                }
                let tc = t.trim_end_matches('.');
                if tc.len() == 1 && (b'1'..=b'4').contains(&tc.as_bytes()[0]) && q.is_none() {
                    q = Some((tc.as_bytes()[0] - b'0') as u32);
                } else if tc.len() == 4 && tc.bytes().all(|b| b.is_ascii_digit()) && y.is_none() {
                    y = tc.parse::<i32>().ok().filter(|v| (1..=9999).contains(v));
                } else {
                    ok = false;
                    break;
                }
            }
            if ok {
                if let Some(qv) = q {
                    let m = (qv - 1) * 3 + 1;
                    let nd = NaiveDate::from_ymd_opt(y.unwrap_or_else(|| base.year()), m, 1)?;
                    return Some(RelativeMatch {
                        naive: nd.and_hms_opt(0, 0, 0)?,
                        period: Period::Month,
                        locale: if has_tr { "tr" } else { "de" }.to_string(),
                        has_time: false,
                        parts: vec!["year", "month"],
                        part_of_day: None,
                    });
                }
            }
        }
    }

    // Seasons with explicit 4-digit year ("summer 2026", "winter 2026", etc.)
    if toks.len() == 2 {
        let mut y: Option<i32> = None;
        let mut sm: Option<(u32, &'static str)> = None;
        for &t in &toks {
            if t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()) {
                y = t.parse::<i32>().ok().filter(|v| (1..=9999).contains(v));
            } else {
                sm = match t {
                    "spring" if settings_allows_locale(settings, "en") => Some((3, "en")),
                    "summer" if settings_allows_locale(settings, "en") => Some((6, "en")),
                    "autumn" | "fall" if settings_allows_locale(settings, "en") => Some((9, "en")),
                    "winter" if settings_allows_locale(settings, "en") => Some((12, "en")),
                    "ilkbahar" if settings_allows_locale(settings, "tr") => Some((3, "tr")),
                    "sonbahar" if settings_allows_locale(settings, "tr") => Some((9, "tr")),
                    "kis" if orig.contains("kış") && settings_allows_locale(settings, "tr") => Some((12, "tr")),
                    "fruhling" if settings_allows_locale(settings, "de") => Some((3, "de")),
                    "sommer" if settings_allows_locale(settings, "de") => Some((6, "de")),
                    "herbst" if settings_allows_locale(settings, "de") => Some((9, "de")),
                    "printemps" if settings_allows_locale(settings, "fr") => Some((3, "fr")),
                    "automne" if settings_allows_locale(settings, "fr") => Some((9, "fr")),
                    "hiver" if settings_allows_locale(settings, "fr") => Some((12, "fr")),
                    _ => None,
                };
            }
        }
        if let (Some(yv), Some((m, loc))) = (y, sm) {
            let nd = NaiveDate::from_ymd_opt(yv, m, 1)?;
            return Some(RelativeMatch {
                naive: nd.and_hms_opt(0, 0, 0)?,
                period: Period::Month,
                locale: loc.to_string(),
                has_time: false,
                parts: vec!["year", "month"],
                part_of_day: None,
            });
        }
    }

    None
}

fn parse_cjk_quarter(s: &str, suffix: &str) -> Option<(Option<i32>, u32)> {
    let idx = s.find(suffix)?;
    let before = s[..idx].trim();
    if before.is_empty() || !s[idx + suffix.len()..].trim().is_empty() {
        return None;
    }
    let q_ch = before.chars().last()?;
    let q = q_ch.to_digit(10)?;
    if !(1..=4).contains(&q) {
        return None;
    }
    let prefix = before[..before.len() - q_ch.len_utf8()].trim();
    if prefix.is_empty() {
        return Some((None, q));
    }
    let prefix_clean = prefix.trim_end_matches('第').trim();
    if prefix_clean.is_empty() {
        return Some((None, q));
    }
    let y_str = prefix_clean.trim_end_matches('年').trim();
    if y_str.len() == 4 && y_str.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(yv) = y_str.parse::<i32>() {
            if (1..=9999).contains(&yv) {
                return Some((Some(yv), q));
            }
        }
    }
    None
}

fn try_period_boundaries(
    s: &str,
    orig: &str,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<RelativeMatch> {
    // CJK boundaries
    if matches!(orig, "月末" | "月底") {
        if let Some(loc) = pick_allowed_locale(settings, &["ja", "zh"]) {
            let d = days_in_month(base.year(), base.month());
            let nd = NaiveDate::from_ymd_opt(base.year(), base.month(), d)?;
            return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, loc, false));
        }
    }
    if orig == "月初" {
        if let Some(loc) = pick_allowed_locale(settings, &["ja", "zh"]) {
            let nd = NaiveDate::from_ymd_opt(base.year(), base.month(), 1)?;
            return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, loc, false));
        }
    }
    if matches!(orig, "年末" | "年底") {
        if let Some(loc) = pick_allowed_locale(settings, &["ja", "zh"]) {
            let nd = NaiveDate::from_ymd_opt(base.year(), 12, 31)?;
            return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, loc, false));
        }
    }
    if orig == "年初" {
        if let Some(loc) = pick_allowed_locale(settings, &["ja", "zh"]) {
            let nd = NaiveDate::from_ymd_opt(base.year(), 1, 1)?;
            return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, loc, false));
        }
    }

    // English exact boundaries
    if settings_allows_locale(settings, "en") {
        let en_boundary = match s {
            "end of month" | "end of the month" | "end of this month" => {
                let d = days_in_month(base.year(), base.month());
                NaiveDate::from_ymd_opt(base.year(), base.month(), d)
            }
            "start of month" | "start of the month" | "start of this month"
            | "beginning of month" | "beginning of the month" | "beginning of this month" => {
                NaiveDate::from_ymd_opt(base.year(), base.month(), 1)
            }
            "end of year" | "end of the year" | "end of this year" => {
                NaiveDate::from_ymd_opt(base.year(), 12, 31)
            }
            "start of year" | "start of the year" | "start of this year"
            | "beginning of year" | "beginning of the year" | "beginning of this year" => {
                NaiveDate::from_ymd_opt(base.year(), 1, 1)
            }
            "mid-month" | "mid month" | "middle of month" | "middle of the month" => {
                NaiveDate::from_ymd_opt(base.year(), base.month(), 15)
            }
            "mid-year" | "mid year" | "middle of year" | "middle of the year" => {
                NaiveDate::from_ymd_opt(base.year(), 7, 1)
            }
            _ => None,
        };
        if let Some(nd) = en_boundary {
            return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, "en", false));
        }

        // "mid-July", "mid July 2026", "end of July [2026]", "start of July [2026]"
        let en_loc = LocaleRegistry::global().resolve_lang_id("en").map(|id| &LocaleRegistry::global().locales[id]);
        if let Some(loc) = en_loc {
            enum ModKind {
                Start,
                Mid,
                End,
            }
            let (kind, rest) = if let Some(r) = s.strip_prefix("mid-").or_else(|| s.strip_prefix("mid ")).or_else(|| s.strip_prefix("middle of ")) {
                (Some(ModKind::Mid), r.trim())
            } else if let Some(r) = s.strip_prefix("end of ") {
                (Some(ModKind::End), r.trim())
            } else if let Some(r) = s.strip_prefix("start of ").or_else(|| s.strip_prefix("beginning of ")) {
                (Some(ModKind::Start), r.trim())
            } else {
                (None, "")
            };
            if let Some(k) = kind {
                let mut parts = rest.split_whitespace();
                if let Some(m_tok) = parts.next() {
                    let y_opt = parts
                        .next()
                        .and_then(|t| if t.len() == 4 { t.parse::<i32>().ok() } else { None })
                        .filter(|y| (1..=9999).contains(y));
                    if parts.next().is_none() {
                        if let Some(&m) = loc.months.get(m_tok) {
                            let y = y_opt.unwrap_or_else(|| base.year());
                            let d = match k {
                                ModKind::Start => 1,
                                ModKind::Mid => 15,
                                ModKind::End => days_in_month(y, m as u32),
                            };
                            if let Some(nd) = NaiveDate::from_ymd_opt(y, m as u32, d) {
                                return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, "en", false));
                            }
                        }
                    }
                }
            }
        }
    }

    // Turkish boundaries
    if settings_allows_locale(settings, "tr") {
        let tr_boundary = match s {
            "ay sonu" | "ayin sonu" => {
                let d = days_in_month(base.year(), base.month());
                NaiveDate::from_ymd_opt(base.year(), base.month(), d)
            }
            "ay basi" | "ayin basi" => NaiveDate::from_ymd_opt(base.year(), base.month(), 1),
            "yil sonu" | "yilin sonu" => NaiveDate::from_ymd_opt(base.year(), 12, 31),
            "yil basi" | "yilin basi" | "yilbasi" => NaiveDate::from_ymd_opt(base.year(), 1, 1),
            _ => None,
        };
        if let Some(nd) = tr_boundary {
            return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, "tr", false));
        }
    }

    // German boundaries
    if settings_allows_locale(settings, "de") {
        let de_boundary = match s {
            "ende des monats" | "monatsende" => {
                let d = days_in_month(base.year(), base.month());
                NaiveDate::from_ymd_opt(base.year(), base.month(), d)
            }
            "anfang des monats" | "monatsanfang" | "monatsbeginn" => {
                NaiveDate::from_ymd_opt(base.year(), base.month(), 1)
            }
            "ende des jahres" | "jahresende" => NaiveDate::from_ymd_opt(base.year(), 12, 31),
            "anfang des jahres" | "jahresanfang" | "jahresbeginn" => {
                NaiveDate::from_ymd_opt(base.year(), 1, 1)
            }
            _ => None,
        };
        if let Some(nd) = de_boundary {
            return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, "de", false));
        }
    }

    // French boundaries
    if settings_allows_locale(settings, "fr") {
        let fr_boundary = match s {
            "fin du mois" | "en fin de mois" => {
                let d = days_in_month(base.year(), base.month());
                NaiveDate::from_ymd_opt(base.year(), base.month(), d)
            }
            "debut du mois" | "en debut de mois" => NaiveDate::from_ymd_opt(base.year(), base.month(), 1),
            "fin d'annee" | "fin de l'annee" => NaiveDate::from_ymd_opt(base.year(), 12, 31),
            "debut d'annee" | "debut de l'annee" => NaiveDate::from_ymd_opt(base.year(), 1, 1),
            _ => None,
        };
        if let Some(nd) = fr_boundary {
            return Some(matched_day(nd.and_hms_opt(0, 0, 0)?, "fr", false));
        }
    }

    None
}

fn try_ordinal_weekday_of_month(
    s: &str,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<RelativeMatch> {
    if !settings_allows_locale(settings, "en") {
        return None;
    }
    let reg = LocaleRegistry::global();
    let en = &reg.locales[reg.resolve_lang_id("en")?];

    let toks: Vec<&str> = s.split_whitespace().collect();
    if toks.len() < 4 || toks.len() > 5 {
        return None;
    }

    let nth: Option<u32> = match toks[0] {
        "first" | "1st" => Some(1),
        "second" | "2nd" => Some(2),
        "third" | "3rd" => Some(3),
        "fourth" | "4th" => Some(4),
        "fifth" | "5th" => Some(5),
        "last" => None,
        _ => return None,
    };

    let target_wd = *en.weekdays.get(toks[1])? as u32;
    if !matches!(toks[2], "of" | "in") {
        return None;
    }
    let m = *en.months.get(toks[3])? as u32;
    let y = if toks.len() == 5 {
        let yt = toks[4];
        if yt.len() == 4 && yt.bytes().all(|b| b.is_ascii_digit()) {
            yt.parse::<i32>().ok().filter(|v| (1..=9999).contains(v))?
        } else {
            return None;
        }
    } else {
        base.year()
    };

    let nd = if let Some(k) = nth {
        let first = NaiveDate::from_ymd_opt(y, m, 1)?;
        let diff = (target_wd + 7 - first.weekday().num_days_from_monday()) % 7;
        let d = 1 + diff + (k - 1) * 7;
        if d > days_in_month(y, m) {
            return None;
        }
        NaiveDate::from_ymd_opt(y, m, d)?
    } else {
        let last_day = days_in_month(y, m);
        let last = NaiveDate::from_ymd_opt(y, m, last_day)?;
        let diff = (last.weekday().num_days_from_monday() + 7 - target_wd) % 7;
        NaiveDate::from_ymd_opt(y, m, last_day - diff)?
    };

    Some(matched_day(nd.and_hms_opt(0, 0, 0)?, "en", false))
}

fn add_business_days(base: NaiveDateTime, sign: i8, mut count: i64) -> Option<NaiveDateTime> {
    if !(0..=100_000).contains(&count) {
        return None;
    }
    if count == 0 {
        return Some(base);
    }
    let dir: i64 = if sign < 0 { -1 } else { 1 };
    let mut d = base.date();
    // If starting on a weekend, step onto the first weekday first (which counts as 1 business day).
    if d.weekday().num_days_from_monday() >= 5 {
        while d.weekday().num_days_from_monday() >= 5 {
            d = d.checked_add_signed(Duration::days(dir))?;
        }
        count -= 1;
    }
    let full_weeks = count / 5;
    let rem = count % 5;
    d = d.checked_add_signed(Duration::days(dir * full_weeks * 7))?;
    let mut left = rem;
    while left > 0 {
        d = d.checked_add_signed(Duration::days(dir))?;
        if d.weekday().num_days_from_monday() < 5 {
            left -= 1;
        }
    }
    if !(1..=9999).contains(&d.year()) {
        return None;
    }
    Some(d.and_time(base.time()))
}

fn parse_nlp_count(tok: &str, loc_code: &str, normalize: bool) -> Option<i64> {
    if let Ok(v) = tok.parse::<i64>() {
        return Some(v);
    }
    let reg = LocaleRegistry::global();
    let loc = &reg.locales[reg.resolve_lang_id(loc_code)?];
    crate::numerals::parse_single_word_cardinal(tok, loc, normalize).map(|v| v as i64)
}

fn try_business_days(
    s: &str,
    orig: &str,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<RelativeMatch> {
    // Japanese: "<N>営業日前" / "<N>営業日後"
    if orig.contains("営業日") && settings_allows_locale(settings, "ja") {
        let (sign, prefix) = if let Some(r) = orig.strip_suffix("営業日前") {
            (-1i8, r.trim())
        } else if let Some(r) = orig.strip_suffix("営業日後") {
            (1i8, r.trim())
        } else {
            (0, "")
        };
        if sign != 0 {
            if let Some(n) = parse_nlp_count(prefix, "ja", settings.normalize) {
                let dt = add_business_days(base, sign, n)?;
                return Some(matched_day(dt, "ja", true));
            }
        }
    }

    // English: "<N> business/working days ago", "in <N> business/working days"
    if settings_allows_locale(settings, "en") {
        let (sign, rest) = if let Some(r) = s.strip_prefix("in ") {
            (1i8, r.trim())
        } else if let Some(r) = s.strip_suffix(" ago") {
            (-1i8, r.trim())
        } else if let Some(r) = s.strip_suffix(" from now") {
            (1i8, r.trim())
        } else {
            (0, "")
        };
        if sign != 0 {
            for kw in [" business days", " business day", " working days", " working day"] {
                if let Some(num_s) = rest.strip_suffix(kw) {
                    if let Some(n) = parse_nlp_count(num_s.trim(), "en", settings.normalize) {
                        let dt = add_business_days(base, sign, n)?;
                        return Some(matched_day(dt, "en", true));
                    }
                }
            }
        }
    }

    // Turkish: "<N> iş günü önce/sonra"
    if settings_allows_locale(settings, "tr") {
        let (sign, rest) = if let Some(r) = s.strip_suffix(" once") {
            (-1i8, r.trim())
        } else if let Some(r) = s.strip_suffix(" sonra") {
            (1i8, r.trim())
        } else {
            (0, "")
        };
        if sign != 0 {
            if let Some(num_s) = rest.strip_suffix(" is gunu") {
                if let Some(n) = parse_nlp_count(num_s.trim(), "tr", settings.normalize) {
                    let dt = add_business_days(base, sign, n)?;
                    return Some(matched_day(dt, "tr", true));
                }
            }
        }
    }

    // German: "in <N> werktagen" / "vor <N> werktagen"
    if settings_allows_locale(settings, "de") {
        let (sign, rest) = if let Some(r) = s.strip_prefix("vor ") {
            (-1i8, r.trim())
        } else if let Some(r) = s.strip_prefix("in ") {
            (1i8, r.trim())
        } else {
            (0, "")
        };
        if sign != 0 {
            for kw in [" werktagen", " werktag", " arbeitstagen", " arbeitstag"] {
                if let Some(num_s) = rest.strip_suffix(kw) {
                    if let Some(n) = parse_nlp_count(num_s.trim(), "de", settings.normalize) {
                        let dt = add_business_days(base, sign, n)?;
                        return Some(matched_day(dt, "de", true));
                    }
                }
            }
        }
    }

    // French: "dans <N> jours ouvrés" / "il y a <N> jours ouvrés"
    if settings_allows_locale(settings, "fr") {
        let (sign, rest) = if let Some(r) = s.strip_prefix("il y a ") {
            (-1i8, r.trim())
        } else if let Some(r) = s.strip_prefix("dans ") {
            (1i8, r.trim())
        } else {
            (0, "")
        };
        if sign != 0 {
            for kw in [" jours ouvres", " jour ouvre", " jours ouvrables", " jour ouvrable"] {
                if let Some(num_s) = rest.strip_suffix(kw) {
                    if let Some(n) = parse_nlp_count(num_s.trim(), "fr", settings.normalize) {
                        let dt = add_business_days(base, sign, n)?;
                        return Some(matched_day(dt, "fr", true));
                    }
                }
            }
        }
    }

    None
}

fn try_chained_relatives(
    s: &str,
    _orig: &str,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<RelativeMatch> {
    let reg = LocaleRegistry::global();

    // English: "<N/a/an> <unit> from <anchor>"
    if settings_allows_locale(settings, "en") {
        if let Some((left, right)) = s.split_once(" from ") {
            let ltoks: Vec<&str> = left.split_whitespace().collect();
            let en = &reg.locales[reg.resolve_lang_id("en")?];
            if ltoks.len() >= 2 && ltoks.len() <= 3 {
                let unit_tok = ltoks[ltoks.len() - 1];
                if let Some(&unit) = en.units.get(unit_tok) {
                    let count_slice = &ltoks[..ltoks.len() - 1];
                    let amt: Option<f64> = if count_slice == ["a"] || count_slice == ["an"] {
                        Some(1.0)
                    } else if count_slice.len() == 1 {
                        if let Ok(v) = count_slice[0].parse::<f64>() {
                            Some(v)
                        } else {
                            crate::numerals::parse_cardinal_slice(count_slice, en, settings.normalize)
                                .map(|(v, _)| v as f64)
                        }
                    } else {
                        crate::numerals::parse_cardinal_slice(count_slice, en, settings.normalize)
                            .map(|(v, _)| v as f64)
                    };
                    if let Some(amount) = amt {
                        let anchor_s = right.trim();
                        let anchor_date: Option<NaiveDate> = match anchor_s {
                            "today" => Some(base.date()),
                            "tomorrow" => base.date().checked_add_signed(Duration::days(1)),
                            "yesterday" => base.date().checked_sub_signed(Duration::days(1)),
                            _ => {
                                if let Some(&wd) = en.weekdays.get(anchor_s) {
                                    let cur = base.weekday().num_days_from_monday() as i64;
                                    let target = wd as i64;
                                    let delta = match settings.prefer_dates_from {
                                        PreferDatesFrom::Future => {
                                            if target >= cur { target - cur } else { target - cur + 7 }
                                        }
                                        PreferDatesFrom::Past | PreferDatesFrom::CurrentPeriod => {
                                            if target <= cur { target - cur } else { target - cur - 7 }
                                        }
                                    };
                                    base.date().checked_add_signed(Duration::days(delta))
                                } else {
                                    None
                                }
                            }
                        };
                        if let Some(ad) = anchor_date {
                            let shifted = apply_relative_offset(ad.and_hms_opt(0, 0, 0)?, 1, amount, unit)?;
                            return Some(matched_day(shifted, "en", false));
                        }
                    }
                }
            }
        }
    }

    // Turkish "haftaya <weekday>" -> upcoming_strict + 7 days
    if settings_allows_locale(settings, "tr") {
        if let Some(wd_str) = s.strip_prefix("haftaya ") {
            let tr = &reg.locales[reg.resolve_lang_id("tr")?];
            let target_opt = tr
                .weekdays
                .get(wd_str.trim())
                .copied()
                .or_else(|| tr.folded_weekdays.get(wd_str.trim()).copied());
            if let Some(wd) = target_opt {
                let cur = base.weekday().num_days_from_monday() as i64;
                let target = wd as i64;
                let first_upcoming = if target > cur { target - cur } else { target + 7 - cur };
                let d = base.date().checked_add_signed(Duration::days(first_upcoming + 7))?;
                return Some(matched_day(d.and_hms_opt(0, 0, 0)?, "tr", false));
            }
        }
    }

    // German "übernächsten / übernächste / übernächster <weekday>"
    if settings_allows_locale(settings, "de") {
        for pref in ["ubernachsten ", "ubernachste ", "ubernachster ", "ubernachstes "] {
            if let Some(wd_str) = s.strip_prefix(pref) {
                let de = &reg.locales[reg.resolve_lang_id("de")?];
                if let Some(&wd) = de.weekdays.get(wd_str.trim()) {
                    let cur = base.weekday().num_days_from_monday() as i64;
                    let target = wd as i64;
                    let first_upcoming = if target > cur { target - cur } else { target + 7 - cur };
                    let d = base.date().checked_add_signed(Duration::days(first_upcoming + 7))?;
                    return Some(matched_day(d.and_hms_opt(0, 0, 0)?, "de", false));
                }
            }
        }
    }

    None
}

fn try_indefinite_and_cjk_fractionals(
    s: &str,
    orig: &str,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<RelativeMatch> {
    // Turkish "birkaç <unit> önce/sonra" -> 3 units
    if settings_allows_locale(settings, "tr") {
        let (sign, rest) = if let Some(r) = s.strip_suffix(" once") {
            (-1i8, r.trim())
        } else if let Some(r) = s.strip_suffix(" sonra") {
            (1i8, r.trim())
        } else {
            (0, "")
        };
        if sign != 0 {
            if let Some(u_str) = rest.strip_prefix("birkac ") {
                let reg = LocaleRegistry::global();
                let tr = &reg.locales[reg.resolve_lang_id("tr")?];
                if let Some(&unit) = tr
                    .units
                    .get(u_str.trim())
                    .or_else(|| tr.folded_units.get(u_str.trim()))
                {
                    let dt = apply_relative_offset(base, sign, 3.0, unit)?;
                    return Some(matched_day(dt, "tr", true));
                }
            }
        }
    }

    // Russian "полтора часа назад" / "полторы недели назад" / "через полтора часа"
    if settings_allows_locale(settings, "ru") {
        let (sign, rest) = if let Some(r) = orig.strip_prefix("через ") {
            (1i8, r.trim())
        } else if let Some(r) = orig.strip_suffix(" назад") {
            (-1i8, r.trim())
        } else {
            (0, "")
        };
        if sign != 0 {
            if let Some(u_str) = rest.strip_prefix("полтора ").or_else(|| rest.strip_prefix("полторы ")) {
                let direct: Option<(f64, TimeUnit)> = match u_str.trim() {
                    "минуты" | "минуту" => Some((90.0, TimeUnit::Second)),
                    "часа" | "час" => Some((90.0, TimeUnit::Minute)),
                    "дня" | "суток" => Some((36.0, TimeUnit::Hour)),
                    "недели" => Some((10.5 * 24.0, TimeUnit::Hour)),
                    "месяца" => Some((45.0, TimeUnit::Day)),
                    "года" => Some((18.0, TimeUnit::Month)),
                    _ => None,
                };
                if let Some((amt, unit)) = direct {
                    let dt = apply_relative_offset(base, sign, amt, unit)?;
                    return Some(matched_day(dt, "ru", true));
                }
            }
        }
    }

    // Japanese fractional: "1時間半前", "1時間半後", "<N>年半前"
    if settings_allows_locale(settings, "ja") && (orig.contains("半前") || orig.contains("半後")) {
        let (sign, core) = if let Some(r) = orig.strip_suffix("半前") {
            (-1i8, r.trim())
        } else if let Some(r) = orig.strip_suffix("半後") {
            (1i8, r.trim())
        } else {
            (0, "")
        };
        if sign != 0 {
            let specs: &[(&str, f64, TimeUnit)] = &[
                ("時間", 60.0, TimeUnit::Minute),
                ("週間", 7.0 * 24.0, TimeUnit::Hour),
                ("か月", 30.0, TimeUnit::Day),
                ("ヶ月", 30.0, TimeUnit::Day),
                ("分", 60.0, TimeUnit::Second),
                ("日", 24.0, TimeUnit::Hour),
                ("年", 12.0, TimeUnit::Month),
            ];
            for &(u_suf, half_mult, base_unit) in specs {
                if let Some(num_s) = core.strip_suffix(u_suf) {
                    if let Ok(n) = num_s.trim().parse::<f64>() {
                        let dt = apply_relative_offset(base, sign, (n + 0.5) * half_mult, base_unit)?;
                        return Some(matched_day(dt, "ja", true));
                    }
                }
            }
        }
    }

    // Chinese half-hour clock: "3点半" / "上午3点半" / "下午3点半"
    if settings_allows_locale(settings, "zh") {
        let mut t = orig;
        let mut is_pm = false;
        let mut is_am = false;
        for p in ["下午", "晚上"] {
            if let Some(r) = t.strip_prefix(p) {
                t = r.trim();
                is_pm = true;
                break;
            }
        }
        if !is_pm {
            for p in ["上午", "早上"] {
                if let Some(r) = t.strip_prefix(p) {
                    t = r.trim();
                    is_am = true;
                    break;
                }
            }
        }
        if let Some(h_str) = t.strip_suffix("点半").or_else(|| t.strip_suffix("點半")) {
            if let Ok(mut h) = h_str.trim().parse::<u32>() {
                if is_pm && h < 12 {
                    h += 12;
                } else if is_am && h == 12 {
                    h = 0;
                }
                if let Some(nt) = NaiveTime::from_hms_opt(h, 30, 0) {
                    let nd = resolve_time_only_date(nt, settings);
                    return Some(matched_day(nd.and_time(nt), "zh", true));
                }
            }
        }
    }

    // Korean half-hour clock: "3시 반" / "오전 3시 반" / "오후 3시 반"
    if settings_allows_locale(settings, "ko") {
        let mut t = orig;
        let mut is_pm = false;
        let mut is_am = false;
        if let Some(r) = t.strip_prefix("오후") {
            t = r.trim();
            is_pm = true;
        } else if let Some(r) = t.strip_prefix("오전") {
            t = r.trim();
            is_am = true;
        }
        if let Some(h_str) = t.strip_suffix("시 반").or_else(|| t.strip_suffix("시반")) {
            if let Ok(mut h) = h_str.trim().parse::<u32>() {
                if is_pm && h < 12 {
                    h += 12;
                } else if is_am && h == 12 {
                    h = 0;
                }
                if let Some(nt) = NaiveTime::from_hms_opt(h, 30, 0) {
                    let nd = resolve_time_only_date(nt, settings);
                    return Some(matched_day(nd.and_time(nt), "ko", true));
                }
            }
        }
    }

    None
}

fn try_conversational_clocks_ext(
    s: &str,
    _orig: &str,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<RelativeMatch> {
    let reg = LocaleRegistry::global();

    // Compound English "<today|yesterday|tomorrow> [morning|afternoon|evening|night] at <time>"
    if settings_allows_locale(settings, "en") {
        if let Some((day_part, time_part)) = s.split_once(" at ") {
            let day_offset: Option<i64> = match day_part.trim() {
                "yesterday" | "yesterday morning" | "yesterday afternoon" | "yesterday evening" => Some(-1),
                "today" | "this morning" | "this afternoon" | "this evening" | "tonight" => Some(0),
                "tomorrow" | "tomorrow morning" | "tomorrow afternoon" | "tomorrow evening" => Some(1),
                _ => None,
            };
            if let Some(off) = day_offset {
                if let Some(nt) = parse_time_str(time_part.trim()) {
                    let d = base.date().checked_add_signed(Duration::days(off))?;
                    return Some(matched_day(d.and_time(nt), "en", true));
                }
            }
        }

        // "<M> past <H>" and "<M> to <H>"
        let en = &reg.locales[reg.resolve_lang_id("en")?];
        for (sep, is_to) in [(" past ", false), (" to ", true)] {
            if let Some((m_raw, h_raw)) = s.split_once(sep) {
                let m_clean = m_raw
                    .trim()
                    .strip_suffix(" minutes")
                    .or_else(|| m_raw.trim().strip_suffix(" minute"))
                    .or_else(|| m_raw.trim().strip_suffix(" mins"))
                    .unwrap_or(m_raw.trim());
                let min_opt = m_clean
                    .parse::<u32>()
                    .ok()
                    .or_else(|| crate::numerals::parse_single_word_cardinal(m_clean, en, settings.normalize));
                let hr_opt = h_raw
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .or_else(|| crate::numerals::parse_single_word_cardinal(h_raw.trim(), en, settings.normalize));
                if let (Some(min), Some(hr)) = (min_opt, hr_opt) {
                    if (1..=59).contains(&min) && hr <= 23 {
                        let (hh, mm) = if is_to {
                            (if hr == 0 { 23 } else { hr - 1 }, 60 - min)
                        } else {
                            (hr, min)
                        };
                        if let Some(nt) = NaiveTime::from_hms_opt(hh, mm, 0) {
                            let nd = resolve_time_only_date(nt, settings);
                            return Some(matched_day(nd.and_time(nt), "en", true));
                        }
                    }
                }
            }
        }
    }

    // German: "halb <H>", "viertel vor <H>", "viertel nach <H>"
    if settings_allows_locale(settings, "de") {
        let de = &reg.locales[reg.resolve_lang_id("de")?];
        let parse_de_hr = |tok: &str| -> Option<u32> {
            let t = tok.trim();
            if t == "eins" || t == "ein" {
                return Some(1);
            }
            t.parse::<u32>()
                .ok()
                .or_else(|| crate::numerals::parse_single_word_cardinal(t, de, settings.normalize))
                .filter(|&h| (1..=24).contains(&h))
        };
        if let Some(rest) = s.strip_prefix("halb ") {
            if let Some(h) = parse_de_hr(rest) {
                let hh = if h == 24 { 23 } else { h - 1 };
                let nt = NaiveTime::from_hms_opt(hh, 30, 0)?;
                let nd = resolve_time_only_date(nt, settings);
                return Some(matched_day(nd.and_time(nt), "de", true));
            }
        } else if let Some(rest) = s.strip_prefix("viertel vor ") {
            if let Some(h) = parse_de_hr(rest) {
                let hh = if h == 24 { 23 } else { h - 1 };
                let nt = NaiveTime::from_hms_opt(hh, 45, 0)?;
                let nd = resolve_time_only_date(nt, settings);
                return Some(matched_day(nd.and_time(nt), "de", true));
            }
        } else if let Some(rest) = s.strip_prefix("viertel nach ") {
            if let Some(h) = parse_de_hr(rest) {
                let hh = if h == 24 { 0 } else { h };
                let nt = NaiveTime::from_hms_opt(hh, 15, 0)?;
                let nd = resolve_time_only_date(nt, settings);
                return Some(matched_day(nd.and_time(nt), "de", true));
            }
        }
    }

    // Turkish: "dörde çeyrek kala", "üçü yirmi geçe"
    if settings_allows_locale(settings, "tr") {
        let (is_kala, rest) = if let Some(r) = s.strip_suffix(" kala") {
            (true, r.trim())
        } else {
            let r = s.strip_suffix(" gece")?;
            (false, r.trim())
        };
        let toks: Vec<&str> = rest.split_whitespace().collect();
        if toks.len() >= 2 && toks.len() <= 3 {
            let parse_tr_hour = |tok: &str| -> Option<u32> {
                match tok {
                    "bire" | "biri" => Some(1),
                    "ikiye" | "ikiyi" => Some(2),
                    "uce" | "ucu" => Some(3),
                    "dorde" | "dordu" => Some(4),
                    "bese" | "besi" => Some(5),
                    "altiya" | "altiyi" => Some(6),
                    "yediye" | "yediyi" => Some(7),
                    "sekize" | "sekizi" => Some(8),
                    "dokuza" | "dokuzu" => Some(9),
                    "ona" | "onu" => Some(10),
                    "onbire" | "onbiri" => Some(11),
                    "onikiye" | "onikiyi" => Some(12),
                    other => {
                        let base_h = other.split(['\'', '’']).next().unwrap_or("");
                        base_h.parse::<u32>().ok().filter(|&h| h <= 23)
                    }
                }
            };
            let (hr_opt, min_toks): (Option<u32>, &[&str]) = if toks.len() == 3 && toks[0] == "on" {
                match toks[1] {
                    "bire" | "biri" => (Some(11), &toks[2..]),
                    "ikiye" | "ikiyi" => (Some(12), &toks[2..]),
                    _ => (parse_tr_hour(toks[0]), &toks[1..]),
                }
            } else {
                (parse_tr_hour(toks[0]), &toks[1..])
            };
            if let Some(hr) = hr_opt {
                let min_joined = min_toks.join(" ");
                let tr = &reg.locales[reg.resolve_lang_id("tr")?];
                let min_opt: Option<u32> = match min_joined.as_str() {
                    "ceyrek" => Some(15),
                    other => other
                        .parse::<u32>()
                        .ok()
                        .or_else(|| {
                            crate::numerals::parse_cardinal_slice(min_toks, tr, settings.normalize)
                                .map(|(v, _)| v)
                        }),
                };
                if let Some(min) = min_opt {
                    if (1..=59).contains(&min) && hr <= 23 {
                        let (hh, mm) = if is_kala {
                            (if hr == 0 { 23 } else { hr - 1 }, 60 - min)
                        } else {
                            (hr, min)
                        };
                        let nt = NaiveTime::from_hms_opt(hh, mm, 0)?;
                        let nd = resolve_time_only_date(nt, settings);
                        return Some(matched_day(nd.and_time(nt), "tr", true));
                    }
                }
            }
        }
    }

    None
}
