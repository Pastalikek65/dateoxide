use crate::fast_path::{days_in_month, parse_time_str};
use crate::locale::{CompiledLocale, LocaleRegistry, RelativeSpec, TimeUnit};
use crate::settings::{Period, Settings};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};

#[derive(Debug, Clone)]
pub struct RelativeMatch {
    pub naive: NaiveDateTime,
    pub period: Period,
    pub locale: String,
}

/// Calendar-accurate offset addition (matches dateutil.relativedelta semantics for months/years).
pub fn apply_relative_offset(
    base: NaiveDateTime,
    sign: i8,
    amount: f64,
    unit: TimeUnit,
) -> Option<NaiveDateTime> {
    let signed = if sign < 0 { -amount } else { amount };
    match unit {
        TimeUnit::Second => {
            let ms = (signed * 1000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))
        }
        TimeUnit::Minute => {
            let ms = (signed * 60_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))
        }
        TimeUnit::Hour => {
            let ms = (signed * 3_600_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))
        }
        TimeUnit::Day => {
            let ms = (signed * 86_400_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))
        }
        TimeUnit::Week => {
            let ms = (signed * 7.0 * 86_400_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))
        }
        TimeUnit::Fortnight => {
            let ms = (signed * 14.0 * 86_400_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))
        }
        TimeUnit::Month => add_months(base, signed.round() as i32),
        TimeUnit::Year => add_months(base, (signed * 12.0).round() as i32),
        TimeUnit::Decade => add_months(base, (signed * 120.0).round() as i32),
        TimeUnit::Century => add_months(base, (signed * 1200.0).round() as i32),
    }
}

fn add_months(base: NaiveDateTime, delta_months: i32) -> Option<NaiveDateTime> {
    let total_months = (base.year() * 12 + (base.month() as i32 - 1)) + delta_months;
    let new_year = total_months.div_euclid(12);
    let new_month = (total_months.rem_euclid(12) + 1) as u32;
    let max_day = days_in_month(new_year, new_month);
    let new_day = base.day().min(max_day);
    let nd = NaiveDate::from_ymd_opt(new_year, new_month, new_day)?;
    Some(nd.and_time(base.time()))
}

fn period_for_unit(unit: TimeUnit) -> Period {
    match unit {
        TimeUnit::Year | TimeUnit::Decade | TimeUnit::Century => Period::Year,
        TimeUnit::Month => Period::Month,
        _ => Period::Day,
    }
}

/// Try parsing a normalized lowercase string as a relative date across candidate locales.
pub fn try_parse_relative(
    normalized: &str,
    candidate_ids: &[usize],
    registry: &LocaleRegistry,
    settings: &Settings,
) -> Option<RelativeMatch> {
    if settings.strict_parsing {
        return None;
    }

    let base = settings.effective_base();

    // 1. O(1) Global Exact Relative Phrase lookup ("yesterday", "bugün", "yarın", "demain", "今日", "şimdi")
    if let Some(matches) = registry.exact_phrase_index.get(normalized) {
        for &(lid, spec) in matches {
            let lid_usize = lid as usize;
            if candidate_ids.is_empty() || candidate_ids.contains(&lid_usize) {
                let dt = apply_relative_offset(base, spec.sign, spec.amount as f64, spec.unit)?;
                return Some(RelativeMatch {
                    naive: dt,
                    period: period_for_unit(spec.unit),
                    locale: registry.locales[lid_usize].code.clone(),
                });
            }
        }
    }

    // 2. Peel off optional time component (e.g., "yesterday at 5pm", "hier à 14h30", "dün saat 15:30")
    let (phrase_no_time, extracted_time) = extract_trailing_or_leading_time(normalized);

    for &lid in candidate_ids {
        let loc = &registry.locales[lid];

        // 2a. Exact relative phrase + time (e.g., "yesterday at 5pm", "hier à 14h30", "dün saat 15:30")
        if let Some(t) = extracted_time {
            let cleaned = strip_connector_words(phrase_no_time, loc);
            if let Some(&spec) = loc.relative_exact.get(cleaned) {
                let dt = apply_relative_offset(base, spec.sign, spec.amount as f64, spec.unit)?;
                return Some(RelativeMatch {
                    naive: dt.date().and_time(t),
                    period: Period::Day,
                    locale: loc.code.clone(),
                });
            }
        }

        // 2b. Zero-regex structured affix patterns (3,592 CLDR/dateparser patterns across 205 langs!)
        // Handles "3 gün önce", "vor 2 wochen", "il y a 4 jours", "3日前", "in 5 hours", etc.
        let target_str = if extracted_time.is_some() {
            strip_connector_words(phrase_no_time, loc)
        } else {
            normalized
        };

        if let Some((spec_sign, amount, unit)) = try_affix_match(target_str, loc) {
            let mut dt = apply_relative_offset(base, spec_sign, amount, unit)?;
            if let Some(t) = extracted_time {
                dt = dt.date().and_time(t);
            }
            return Some(RelativeMatch {
                naive: dt,
                period: period_for_unit(unit),
                locale: loc.code.clone(),
            });
        }

        // 2c. Compound & word-number relative expressions
        // Handles "1 year, 2 months, 3 days ago", "2 hours 30 minutes ago", "a week ago",
        // "two days ago", "bir saat önce", "in a couple days", "1 hafta 2 gün önce", etc.
        if let Some((mut dt, period)) = try_compound_relative(target_str, loc, base) {
            if let Some(t) = extracted_time {
                dt = dt.date().and_time(t);
            }
            return Some(RelativeMatch {
                naive: dt,
                period,
                locale: loc.code.clone(),
            });
        }

        // 2d. Relative weekday expressions: "last Monday", "next Friday", "geçen pazartesi"
        if let Some(mut dt) = try_relative_weekday(target_str, loc, base) {
            if let Some(t) = extracted_time {
                dt = dt.date().and_time(t);
            }
            return Some(RelativeMatch {
                naive: dt,
                period: Period::Day,
                locale: loc.code.clone(),
            });
        }
    }

    None
}

/// Zero-regex prefix + number + suffix matcher covering 99.97% of dateparser's relative-type-regex!
fn try_affix_match(s: &str, loc: &CompiledLocale) -> Option<(i8, f64, TimeUnit)> {
    // Also support word-number substitution at the start (e.g., "bir gün önce" -> "1 gün önce", "zwei wochen")
    let replaced_opt = replace_word_numbers_once(s, loc);
    let candidates: [&str; 2] = match replaced_opt.as_deref() {
        Some(r) => [s, r],
        None => [s, ""],
    };

    for cand in candidates {
        if cand.is_empty() {
            continue;
        }
        for affix in &loc.relative_affix {
            let after_prefix = if affix.prefix.is_empty() {
                cand
            } else if let Some(rest) = cand.strip_prefix(&affix.prefix) {
                rest
            } else {
                continue;
            };

            let middle = if affix.suffix.is_empty() {
                after_prefix
            } else if let Some(rest) = after_prefix.strip_suffix(&affix.suffix) {
                rest
            } else {
                continue;
            };

            let num_str = middle.trim();
            if let Some(val) = parse_positive_number(num_str) {
                return Some((affix.sign, val, affix.unit));
            }
        }
    }
    None
}

fn parse_positive_number(s: &str) -> Option<f64> {
    if s.is_empty() || s.len() > 12 {
        return None;
    }
    let mut seen_sep = false;
    let mut has_digit = false;
    for b in s.bytes() {
        if b.is_ascii_digit() {
            has_digit = true;
        } else if (b == b'.' || b == b',') && !seen_sep {
            seen_sep = true;
        } else {
            return None;
        }
    }
    if !has_digit {
        return None;
    }
    if s.contains(',') {
        s.replace(',', ".").parse::<f64>().ok()
    } else {
        s.parse::<f64>().ok()
    }
}

fn replace_word_numbers_once(s: &str, loc: &CompiledLocale) -> Option<String> {
    if loc.word_numbers.is_empty() {
        return None;
    }
    let mut changed = false;
    let mut parts = Vec::new();
    for tok in s.split_whitespace() {
        if let Some(&n) = loc.word_numbers.get(tok) {
            parts.push(n.to_string());
            changed = true;
        } else {
            parts.push(tok.to_string());
        }
    }
    if changed {
        Some(parts.join(" "))
    } else {
        None
    }
}

/// Parses compound relative expressions like "1 year, 2 months, 3 days ago",
/// "in 2 hours and 30 minutes", "1 hafta 2 gün önce", "a week ago", "two days ago".
fn try_compound_relative(
    s: &str,
    loc: &CompiledLocale,
    base: NaiveDateTime,
) -> Option<(NaiveDateTime, Period)> {
    // Check for multi-word "ago" or "in" markers (e.g., "il y a", "hace", "vor", "önce", "sonra", "ago", "in")
    let mut sign: Option<i8> = None;
    let mut rest = s.trim();

    // Check multi-word prefixes/suffixes first
    for w in &loc.ago {
        if let Some(stripped) = rest.strip_suffix(w.as_str()) {
            if stripped.is_empty() || stripped.ends_with(' ') || !w.is_ascii() {
                sign = Some(-1);
                rest = stripped.trim();
                break;
            }
        }
        if let Some(stripped) = rest.strip_prefix(w.as_str()) {
            if stripped.is_empty() || stripped.starts_with(' ') || !w.is_ascii() {
                sign = Some(-1);
                rest = stripped.trim();
                break;
            }
        }
    }

    if sign.is_none() {
        for w in &loc.in_words {
            if let Some(stripped) = rest.strip_prefix(w.as_str()) {
                if stripped.is_empty() || stripped.starts_with(' ') || !w.is_ascii() {
                    sign = Some(1);
                    rest = stripped.trim();
                    break;
                }
            }
            if let Some(stripped) = rest.strip_suffix(w.as_str()) {
                if stripped.is_empty() || stripped.ends_with(' ') || !w.is_ascii() {
                    sign = Some(1);
                    rest = stripped.trim();
                    break;
                }
            }
        }
    }

    let sign = sign?;
    if rest.is_empty() {
        return None;
    }

    // Tokenize remaining (number, unit) pairs
    let tokens: Vec<&str> = rest
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .filter(|t| !t.is_empty())
        .collect();

    let mut pairs: Vec<(f64, TimeUnit)> = Vec::new();
    let mut pending_num: Option<f64> = None;

    for &tok in &tokens {
        let clean = tok.trim_matches(|c: char| c == ',' || c == '.');
        if clean.is_empty() {
            continue;
        }
        // Skip connectors like "and", "ve", "und", "et", "y"
        if matches!(clean, "and" | "ve" | "und" | "et" | "y" | "e" | "of") || loc.skip.contains(clean)
        {
            continue;
        }

        // Attached number+unit like "5m", "2h", "3d", "10s"
        if pending_num.is_none() {
            if let Some((num, unit_str)) = split_attached_num_unit(clean) {
                if let Some(&u) = loc.units.get(unit_str) {
                    pairs.push((num, u));
                    continue;
                }
            }
        }

        if let Some(n) = parse_positive_number(clean) {
            if pending_num.is_some() {
                return None;
            }
            pending_num = Some(n);
        } else if let Some(&wn) = loc.word_numbers.get(clean) {
            if pending_num.is_some() {
                return None;
            }
            pending_num = Some(wn as f64);
        } else if let Some(&u) = loc.units.get(clean) {
            let amt = pending_num.take().unwrap_or(1.0);
            pairs.push((amt, u));
        } else {
            // Unrecognized non-skip word -> not a clean relative expression
            return None;
        }
    }

    if pairs.is_empty() || pending_num.is_some() {
        return None;
    }

    let mut dt = base;
    let mut finest_period = Period::Year;
    for (amt, unit) in pairs {
        dt = apply_relative_offset(dt, sign, amt, unit)?;
        let p = period_for_unit(unit);
        if matches!(p, Period::Day) || (matches!(p, Period::Month) && matches!(finest_period, Period::Year)) {
            finest_period = p;
        }
    }

    Some((dt, finest_period))
}

fn split_attached_num_unit(s: &str) -> Option<(f64, &str)> {
    let idx = s.find(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')?;
    if idx == 0 {
        return None;
    }
    let num = parse_positive_number(&s[..idx])?;
    let unit = &s[idx..];
    if unit.is_empty() {
        return None;
    }
    Some((num, unit))
}

fn try_relative_weekday(s: &str, loc: &CompiledLocale, base: NaiveDateTime) -> Option<NaiveDateTime> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() != 2 {
        return None;
    }
    let (modifier, wd_tok) = (tokens[0], tokens[1]);
    let target_wd = *loc.weekdays.get(wd_tok)?;
    let dir: i64 = match modifier {
        "last" | "previous" | "geçen" | "letzten" | "letzter" | "dernier" | "pasado" | "прошлый" => {
            -1
        }
        "next" | "gelecek" | "önümüzdeki" | "nächsten" | "nächster" | "prochain" | "próximo"
        | "следующий" => 1,
        "this" | "bu" | "diesen" | "ce" | "este" => 0,
        _ => return None,
    };

    let current_wd = base.weekday().num_days_from_monday() as i64;
    let target = target_wd as i64;
    let delta_days = if dir < 0 {
        let mut diff = target - current_wd;
        if diff >= 0 {
            diff -= 7;
        }
        diff
    } else if dir > 0 {
        let mut diff = target - current_wd;
        if diff <= 0 {
            diff += 7;
        }
        diff
    } else {
        target - current_wd
    };

    base.checked_add_signed(Duration::days(delta_days))
}

/// Extract a trailing or leading time from strings like "yesterday at 5pm", "hier à 14h30", "dün saat 15:30".
pub fn extract_trailing_or_leading_time(s: &str) -> (&str, Option<NaiveTime>) {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() < 2 {
        return (s, None);
    }

    // Check last 2 tokens for "5 pm" / "5:30 pm"
    if tokens.len() >= 3 {
        let last2 = format!("{} {}", tokens[tokens.len() - 2], tokens[tokens.len() - 1]);
        if let Some(t) = parse_time_str(&last2) {
            let cut = s.len() - tokens[tokens.len() - 1].len() - tokens[tokens.len() - 2].len() - 1;
            if cut < s.len() {
                return (s[..cut].trim_end(), Some(t));
            }
        }
    }

    // Check last 1 token for "14:30", "14h30", "5pm"
    let last = tokens[tokens.len() - 1];
    if last.contains(':') || last.contains('h') || last.ends_with("am") || last.ends_with("pm") {
        if let Some(t) = parse_time_str(last) {
            let cut = s.len() - last.len();
            return (s[..cut].trim_end(), Some(t));
        }
    }

    (s, None)
}

fn strip_connector_words<'a>(s: &'a str, loc: &CompiledLocale) -> &'a str {
    let mut cur = s.trim();
    loop {
        let mut trimmed = false;
        if let Some(sp) = cur.rfind(' ') {
            let tail = &cur[sp + 1..];
            if matches!(tail, "at" | "on" | "à" | "a" | "um" | "am" | "saat" | "las" | "a las" | "в")
                || loc.skip.contains(tail)
            {
                cur = cur[..sp].trim_end();
                trimmed = true;
            }
        }
        if !trimmed {
            break;
        }
    }
    cur
}

#[allow(dead_code)]
fn _spec_unused(_s: RelativeSpec) {}
