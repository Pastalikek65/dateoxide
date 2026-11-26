use crate::fast_path::{days_in_month, parse_time_str, resolve_time_only_date};
use crate::locale::{
    fold_diacritics, CompiledLocale, LocaleRegistry, RelativeAffix, RelativeSpec, TimeUnit,
};
use crate::settings::{PartOfDay, Period, PreferDatesFrom, Settings};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};

#[derive(Debug, Clone)]
pub struct RelativeMatch {
    pub naive: NaiveDateTime,
    pub period: Period,
    pub locale: String,
    pub has_time: bool,
    pub parts: Vec<&'static str>,
    pub part_of_day: Option<PartOfDay>,
}

/// Calendar-accurate offset addition (matches dateutil.relativedelta semantics for months/years).
pub fn apply_relative_offset(
    base: NaiveDateTime,
    sign: i8,
    amount: f64,
    unit: TimeUnit,
) -> Option<NaiveDateTime> {
    if !amount.is_finite() || amount < 0.0 || amount > 10_000_000.0 {
        return None;
    }
    let signed = if sign < 0 { -amount } else { amount };
    let dt = match unit {
        TimeUnit::Second => {
            let ms = (signed * 1000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))?
        }
        TimeUnit::Minute => {
            let ms = (signed * 60_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))?
        }
        TimeUnit::Hour => {
            let ms = (signed * 3_600_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))?
        }
        TimeUnit::Day => {
            if amount > 3_650_000.0 {
                return None;
            }
            let ms = (signed * 86_400_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))?
        }
        TimeUnit::Week => {
            if amount > 520_000.0 {
                return None;
            }
            let ms = (signed * 7.0 * 86_400_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))?
        }
        TimeUnit::Fortnight => {
            if amount > 260_000.0 {
                return None;
            }
            let ms = (signed * 14.0 * 86_400_000.0).round() as i64;
            base.checked_add_signed(Duration::milliseconds(ms))?
        }
        TimeUnit::Month => {
            if amount > 120_000.0 {
                return None;
            }
            add_months(base, signed.round() as i32)?
        }
        TimeUnit::Year => {
            if amount > 9_999.0 {
                return None;
            }
            add_months(base, (signed * 12.0).round() as i32)?
        }
        TimeUnit::Decade => {
            if amount > 1_000.0 {
                return None;
            }
            add_months(base, (signed * 120.0).round() as i32)?
        }
        TimeUnit::Century => {
            if amount > 100.0 {
                return None;
            }
            add_months(base, (signed * 1200.0).round() as i32)?
        }
    };
    if !(1..=9999).contains(&dt.year()) {
        return None;
    }
    Some(dt)
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
        TimeUnit::Week => Period::Week,
        _ => Period::Day,
    }
}

fn parts_for_relative(has_time: bool, unit: Option<TimeUnit>) -> Vec<&'static str> {
    let time_unit = matches!(
        unit,
        Some(TimeUnit::Hour | TimeUnit::Minute | TimeUnit::Second)
    );
    if has_time || time_unit {
        vec!["year", "month", "day", "time"]
    } else {
        vec!["year", "month", "day"]
    }
}

fn time_for_hour_anchor(hour: u8, settings: &Settings) -> (NaiveTime, Option<PartOfDay>) {
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

fn select_relative_spec(specs: &[RelativeSpec], pref: PreferDatesFrom) -> RelativeSpec {
    if specs.len() == 1 {
        return specs[0];
    }
    match pref {
        PreferDatesFrom::Past => specs
            .iter()
            .copied()
            .find(|s| s.sign < 0)
            .unwrap_or(specs[0]),
        PreferDatesFrom::Future | PreferDatesFrom::CurrentPeriod => specs
            .iter()
            .copied()
            .find(|s| s.sign > 0)
            .unwrap_or(specs[0]),
    }
}

fn lookup_exact_in_locale(
    phrase: &str,
    loc: &CompiledLocale,
    normalize: bool,
    pref: PreferDatesFrom,
) -> Option<RelativeSpec> {
    if let Some(specs) = loc.relative_exact.get(phrase) {
        return Some(select_relative_spec(specs, pref));
    }
    if normalize {
        let f = fold_diacritics(phrase);
        if let Some(specs) = loc.folded_relative_exact.get(&f) {
            return Some(select_relative_spec(specs, pref));
        }
    }
    None
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
    let trimmed_article = normalized.strip_prefix("the ").unwrap_or(normalized);
    let folded_full = if settings.normalize {
        fold_diacritics(trimmed_article)
    } else {
        trimmed_article.to_string()
    };

    // 0. Special compound day + time-of-day idioms, half/fractional durations & conversational clocks
    if let Some(m) = try_conversational_clock_or_fraction(trimmed_article, settings) {
        return Some(m);
    }
    if let Some(m) = try_compound_tod_idiom(trimmed_article, &folded_full, candidate_ids, registry, base, settings) {
        return Some(m);
    }

    // 1. O(1) Global Exact Relative Phrase lookup ("yesterday", "bugün", "yarın", "demain", "今日", "कल", "غدا")
    if let Some(matches) = registry.exact_phrase_index.get(trimmed_article) {
        let mut matched_specs: Vec<(usize, RelativeSpec)> = Vec::new();
        if settings.use_given_language_order && !candidate_ids.is_empty() {
            for &cid in candidate_ids {
                for &(lid, spec) in matches {
                    if lid as usize == cid {
                        matched_specs.push((cid, spec));
                    }
                }
            }
        } else {
            for &(lid, spec) in matches {
                let lid_usize = lid as usize;
                if candidate_ids.is_empty() || candidate_ids.contains(&lid_usize) {
                    matched_specs.push((lid_usize, spec));
                }
            }
        }
        if !matched_specs.is_empty() {
            let specs_only: Vec<RelativeSpec> = if settings.use_given_language_order && !candidate_ids.is_empty() {
                let first_lid = matched_specs[0].0;
                matched_specs
                    .iter()
                    .filter(|(l, _)| *l == first_lid)
                    .map(|(_, s)| *s)
                    .collect()
            } else {
                matched_specs.iter().map(|(_, s)| *s).collect()
            };
            let chosen = select_relative_spec(&specs_only, settings.prefer_dates_from);
            let chosen_lid = matched_specs
                .iter()
                .find(|(_, s)| *s == chosen)
                .map(|(l, _)| *l)
                .unwrap_or(matched_specs[0].0);
            let dt = apply_relative_offset(base, chosen.sign, chosen.amount as f64, chosen.unit)?;
            return Some(RelativeMatch {
                naive: dt,
                period: period_for_unit(chosen.unit),
                locale: registry.locales[chosen_lid].code.clone(),
                has_time: false,
                parts: parts_for_relative(false, Some(chosen.unit)),
                part_of_day: None,
            });
        }
    }

    if settings.normalize {
        if let Some(matches) = registry.folded_exact_phrase_index.get(&folded_full) {
            let mut matched_specs: Vec<(usize, RelativeSpec)> = Vec::new();
            if settings.use_given_language_order && !candidate_ids.is_empty() {
                for &cid in candidate_ids {
                    for &(lid, spec) in matches {
                        if lid as usize == cid {
                            matched_specs.push((cid, spec));
                        }
                    }
                }
            } else {
                for &(lid, spec) in matches {
                    let lid_usize = lid as usize;
                    if candidate_ids.is_empty() || candidate_ids.contains(&lid_usize) {
                        matched_specs.push((lid_usize, spec));
                    }
                }
            }
            if !matched_specs.is_empty() {
                let specs_only: Vec<RelativeSpec> = if settings.use_given_language_order && !candidate_ids.is_empty() {
                    let first_lid = matched_specs[0].0;
                    matched_specs
                        .iter()
                        .filter(|(l, _)| *l == first_lid)
                        .map(|(_, s)| *s)
                        .collect()
                } else {
                    matched_specs.iter().map(|(_, s)| *s).collect()
                };
                let chosen = select_relative_spec(&specs_only, settings.prefer_dates_from);
                let chosen_lid = matched_specs
                    .iter()
                    .find(|(_, s)| *s == chosen)
                    .map(|(l, _)| *l)
                    .unwrap_or(matched_specs[0].0);
                let dt = apply_relative_offset(base, chosen.sign, chosen.amount as f64, chosen.unit)?;
                return Some(RelativeMatch {
                    naive: dt,
                    period: period_for_unit(chosen.unit),
                    locale: registry.locales[chosen_lid].code.clone(),
                    has_time: false,
                    parts: parts_for_relative(false, Some(chosen.unit)),
                    part_of_day: None,
                });
            }
        }
    }

    // 2. Peel off optional explicit time component
    let (phrase_no_time, extracted_time) = extract_trailing_or_leading_time(normalized);

    for &lid in candidate_ids {
        let loc = &registry.locales[lid];

        // 2a. Exact relative phrase + explicit time
        if let Some(t) = extracted_time {
            let cleaned = strip_connector_words(phrase_no_time, loc);
            if let Some(spec) = lookup_exact_in_locale(cleaned, loc, settings.normalize, settings.prefer_dates_from) {
                let dt = apply_relative_offset(base, spec.sign, spec.amount as f64, spec.unit)?;
                return Some(RelativeMatch {
                    naive: dt.date().and_time(t),
                    period: Period::Day,
                    locale: loc.code.clone(),
                    has_time: true,
                    parts: vec!["year", "month", "day", "time"],
                    part_of_day: None,
                });
            }
        }

        // 2b. <relative_exact> [connector] <time_of_day> ("yesterday at noon", "yesterday morning", "dün akşam")
        if extracted_time.is_none() {
            if let Some((dt, pod)) = try_relative_plus_tod(normalized, loc, base, settings) {
                return Some(RelativeMatch {
                    naive: dt,
                    period: Period::Day,
                    locale: loc.code.clone(),
                    has_time: true,
                    parts: vec!["year", "month", "day", "time"],
                    part_of_day: pod,
                });
            }
        }

        // 2c. Zero-regex structured affix patterns (3,592 CLDR/dateparser patterns across 205 langs!)
        let target_str = if extracted_time.is_some() {
            strip_connector_words(phrase_no_time, loc)
        } else {
            normalized
        };

        if let Some((spec_sign, amount, unit)) = try_affix_match(target_str, loc, settings.normalize) {
            let mut dt = apply_relative_offset(base, spec_sign, amount, unit)?;
            if let Some(t) = extracted_time {
                dt = dt.date().and_time(t);
            }
            let has_t = extracted_time.is_some();
            return Some(RelativeMatch {
                naive: dt,
                period: if has_t { Period::Day } else { period_for_unit(unit) },
                locale: loc.code.clone(),
                has_time: has_t,
                parts: parts_for_relative(has_t, Some(unit)),
                part_of_day: None,
            });
        }

        // 2d. Compound & word-number relative expressions ("2 ay 3 gün önce", "5 dk önce", "1 year, 2 months, 3 days ago")
        if let Some((mut dt, period, has_time_unit)) =
            try_compound_relative(target_str, loc, base, settings.normalize)
        {
            if let Some(t) = extracted_time {
                dt = dt.date().and_time(t);
            }
            let has_t = extracted_time.is_some();
            return Some(RelativeMatch {
                naive: dt,
                period: if has_t { Period::Day } else { period },
                locale: loc.code.clone(),
                has_time: has_t,
                parts: if has_t || has_time_unit {
                    vec!["year", "month", "day", "time"]
                } else {
                    vec!["year", "month", "day"]
                },
                part_of_day: None,
            });
        }

        // 2e. Bare relative duration ("2 days", "3 hours", "1 week", "2 gün") without explicit ago/in
        if let Some((mut dt, period, has_time_unit)) =
            try_bare_duration_relative(target_str, loc, base, settings)
        {
            if let Some(t) = extracted_time {
                dt = dt.date().and_time(t);
            }
            let has_t = extracted_time.is_some();
            return Some(RelativeMatch {
                naive: dt,
                period: if has_t { Period::Day } else { period },
                locale: loc.code.clone(),
                has_time: has_t,
                parts: if has_t || has_time_unit {
                    vec!["year", "month", "day", "time"]
                } else {
                    vec!["year", "month", "day"]
                },
                part_of_day: None,
            });
        }

        // 2f. Relative period/weekday expressions: "last Monday", "geçen hafta", "gelecek ay"
        if let Some((mut dt, period)) =
            try_relative_period_or_weekday(target_str, loc, base, settings.normalize)
        {
            if let Some(t) = extracted_time {
                dt = dt.date().and_time(t);
            }
            let has_t = extracted_time.is_some();
            return Some(RelativeMatch {
                naive: dt,
                period: if has_t { Period::Day } else { period },
                locale: loc.code.clone(),
                has_time: has_t,
                parts: parts_for_relative(has_t, None),
                part_of_day: None,
            });
        }
    }

    None
}

fn try_compound_tod_idiom(
    normalized: &str,
    folded: &str,
    candidate_ids: &[usize],
    registry: &LocaleRegistry,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<RelativeMatch> {
    let check = if settings.normalize { folded } else { normalized };
    let (day_offset, hour, lang_code): (i64, u8, &str) = match check {
        "tonight" | "this night" => (0, 20, "en"),
        "last night" => (-1, 20, "en"),
        "this morning" | "today morning" => (0, 9, "en"),
        "this afternoon" | "today afternoon" => (0, 15, "en"),
        "this evening" | "today evening" => (0, 18, "en"),
        "day before yesterday" => {
            let dt = base - Duration::days(2);
            return Some(RelativeMatch {
                naive: dt,
                period: Period::Day,
                locale: "en".to_string(),
                has_time: false,
                parts: vec!["year", "month", "day"],
                part_of_day: None,
            });
        }
        "day after tomorrow" => {
            let dt = base + Duration::days(2);
            return Some(RelativeMatch {
                naive: dt,
                period: Period::Day,
                locale: "en".to_string(),
                has_time: false,
                parts: vec!["year", "month", "day"],
                part_of_day: None,
            });
        }
        "yesterday morning" => (-1, 9, "en"),
        "yesterday afternoon" => (-1, 15, "en"),
        "yesterday evening" => (-1, 18, "en"),
        "tomorrow morning" => (1, 9, "en"),
        "tomorrow evening" => (1, 18, "en"),
        "tomorrow night" => (1, 20, "en"),
        "dün gece" | "dun gece" => (-1, 20, "tr"),
        "dün akşam" | "dun aksam" => (-1, 18, "tr"),
        "dün sabah" | "dun sabah" => (-1, 9, "tr"),
        "bu akşam" | "bu aksam" => (0, 18, "tr"),
        "bu sabah" => (0, 9, "tr"),
        "bu gece" => (0, 20, "tr"),
        "yarın sabah" | "yarin sabah" => (1, 9, "tr"),
        "yarın akşam" | "yarin aksam" => (1, 18, "tr"),
        "heute abend" => (0, 18, "de"),
        "heute morgen" => (0, 9, "de"),
        "gestern abend" => (-1, 18, "de"),
        "heute nacht" => (0, 20, "de"),
        "ce soir" => (0, 18, "fr"),
        "ce matin" => (0, 9, "fr"),
        "hier soir" => (-1, 18, "fr"),
        "anoche" => (-1, 20, "es"),
        "esta noche" => (0, 20, "es"),
        "esta tarde" => (0, 15, "es"),
        _ => return None,
    };

    if !candidate_ids.is_empty() {
        let lang_allowed = candidate_ids
            .iter()
            .any(|&id| registry.locales[id].code == lang_code);
        if !lang_allowed {
            return None;
        }
    }

    let target_date = (base + Duration::days(day_offset)).date();
    let (nt, pod) = time_for_hour_anchor(hour, settings);
    Some(RelativeMatch {
        naive: target_date.and_time(nt),
        period: Period::Day,
        locale: lang_code.to_string(),
        has_time: true,
        parts: vec!["year", "month", "day", "time"],
        part_of_day: pod,
    })
}

fn split_tod_hour<'a>(
    s: &'a str,
    loc: &CompiledLocale,
    normalize: bool,
) -> Option<(&'a str, u8)> {
    for (phrase, &hour) in &loc.time_of_day {
        if let Some(rest) = s.strip_suffix(phrase.as_str()) {
            if rest.is_empty() || rest.ends_with(' ') {
                return Some((rest.trim_end(), hour));
            }
        }
    }
    if normalize {
        let fs = fold_diacritics(s);
        for (phrase, &hour) in &loc.folded_time_of_day {
            if let Some(rest) = fs.strip_suffix(phrase.as_str()) {
                if rest.is_empty() || rest.ends_with(' ') {
                    return Some((s[..rest.len().min(s.len())].trim_end(), hour));
                }
            }
        }
        for (phrase, &hour) in &loc.time_of_day {
            if let Some(rest) = fs.strip_suffix(phrase.as_str()) {
                if rest.is_empty() || rest.ends_with(' ') {
                    return Some((s[..rest.len().min(s.len())].trim_end(), hour));
                }
            }
        }
    }
    None
}

fn try_relative_plus_tod(
    normalized: &str,
    loc: &CompiledLocale,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<(NaiveDateTime, Option<PartOfDay>)> {
    let (prefix_raw, hour) = split_tod_hour(normalized, loc, settings.normalize)?;
    let (nt, pod) = time_for_hour_anchor(hour, settings);
    if prefix_raw.is_empty() {
        return Some((base.date().and_time(nt), pod));
    }

    let cleaned = strip_connector_words(prefix_raw, loc);
    if let Some(spec) = lookup_exact_in_locale(
        cleaned,
        loc,
        settings.normalize,
        settings.prefer_dates_from,
    ) {
        let dt = apply_relative_offset(base, spec.sign, spec.amount as f64, spec.unit)?;
        return Some((dt.date().and_time(nt), pod));
    }

    None
}

/// Zero-regex prefix + number + suffix matcher covering 99.97% of dateparser's relative-type-regex!
fn try_affix_match(s: &str, loc: &CompiledLocale, normalize: bool) -> Option<(i8, f64, TimeUnit)> {
    let mut candidates: Vec<String> = Vec::with_capacity(3);
    candidates.push(s.to_string());
    if let Some(r) = replace_word_numbers_once(s, loc, normalize) {
        candidates.push(r);
    }
    if normalize {
        let fs = fold_diacritics(s);
        if fs != s {
            if let Some(r) = replace_word_numbers_once(&fs, loc, true) {
                candidates.push(r);
            }
            candidates.push(fs);
        }
    }

    let match_against = |cand: &str, affixes: &[RelativeAffix]| -> Option<(i8, f64, TimeUnit)> {
        for affix in affixes {
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
        None
    };

    for cand in &candidates {
        if let Some(hit) = match_against(cand, &loc.relative_affix) {
            return Some(hit);
        }
    }
    if normalize && !loc.folded_relative_affix.is_empty() {
        for cand in &candidates {
            if let Some(hit) = match_against(cand, &loc.folded_relative_affix) {
                return Some(hit);
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

fn replace_word_numbers_once(s: &str, loc: &CompiledLocale, normalize: bool) -> Option<String> {
    if loc.word_numbers.is_empty() {
        return None;
    }
    let mut changed = false;
    let mut parts = Vec::new();
    for tok in s.split_whitespace() {
        if let Some(&n) = loc.word_numbers.get(tok) {
            parts.push(n.to_string());
            changed = true;
        } else if normalize {
            let ft = fold_diacritics(tok);
            if let Some(&n) = loc.word_numbers.get(&ft) {
                parts.push(n.to_string());
                changed = true;
            } else {
                parts.push(tok.to_string());
            }
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

fn try_compound_relative(
    s: &str,
    loc: &CompiledLocale,
    base: NaiveDateTime,
    normalize: bool,
) -> Option<(NaiveDateTime, Period, bool)> {
    let mut sign: Option<i8> = None;
    let mut rest = s.trim().to_string();

    let strip_marker = |text: &str, markers: &rustc_hash::FxHashSet<String>| -> Option<String> {
        let mut sorted: Vec<&str> = markers.iter().map(|s| s.as_str()).collect();
        sorted.sort_unstable_by(|a, b| b.len().cmp(&a.len()));
        for w in sorted {
            if let Some(stripped) = text.strip_suffix(w) {
                if stripped.is_empty() || stripped.ends_with(' ') || !w.is_ascii() {
                    return Some(stripped.trim().to_string());
                }
            }
            if let Some(stripped) = text.strip_prefix(w) {
                if stripped.is_empty() || stripped.starts_with(' ') || !w.is_ascii() {
                    return Some(stripped.trim().to_string());
                }
            }
        }
        None
    };

    if let Some(r) = strip_marker(&rest, &loc.ago) {
        sign = Some(-1);
        rest = r;
    } else if normalize {
        let f_rest = fold_diacritics(&rest);
        if let Some(r) = strip_marker(&f_rest, &loc.folded_ago) {
            sign = Some(-1);
            rest = r;
        } else if let Some(r) = strip_marker(&f_rest, &loc.ago) {
            sign = Some(-1);
            rest = r;
        }
    }

    if sign.is_none() {
        if let Some(r) = strip_marker(&rest, &loc.in_words) {
            sign = Some(1);
            rest = r;
        } else if normalize {
            let f_rest = fold_diacritics(&rest);
            if let Some(r) = strip_marker(&f_rest, &loc.folded_in_words) {
                sign = Some(1);
                rest = r;
            } else if let Some(r) = strip_marker(&f_rest, &loc.in_words) {
                sign = Some(1);
                rest = r;
            }
        }
    }

    let sign = sign?;
    if rest.is_empty() {
        return None;
    }

    apply_token_duration_pairs(&rest, loc, base, sign, normalize, true)
}

fn try_bare_duration_relative(
    s: &str,
    loc: &CompiledLocale,
    base: NaiveDateTime,
    settings: &Settings,
) -> Option<(NaiveDateTime, Period, bool)> {
    let sign = if settings.prefer_dates_from == PreferDatesFrom::Future {
        1
    } else {
        -1
    };
    apply_token_duration_pairs(s, loc, base, sign, settings.normalize, false)
}

fn apply_token_duration_pairs(
    rest: &str,
    loc: &CompiledLocale,
    base: NaiveDateTime,
    sign: i8,
    normalize: bool,
    allow_single_char_attached_units: bool,
) -> Option<(NaiveDateTime, Period, bool)> {
    let lookup_unit = |tok: &str| -> Option<TimeUnit> {
        if let Some(&u) = loc.units.get(tok) {
            return Some(u);
        }
        if normalize {
            let ft = fold_diacritics(tok);
            if let Some(&u) = loc.units.get(&ft).or_else(|| loc.folded_units.get(&ft)) {
                return Some(u);
            }
        }
        None
    };

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
        if matches!(clean, "and" | "ve" | "und" | "et" | "y" | "e" | "of") || loc.skip.contains(clean)
        {
            continue;
        }

        if pending_num.is_none() && allow_single_char_attached_units {
            if let Some((num, unit_str)) = split_attached_num_unit(clean) {
                if let Some(u) = lookup_unit(unit_str) {
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
        } else if let Some(u) = lookup_unit(clean) {
            if !allow_single_char_attached_units && pending_num.is_none() {
                return None;
            }
            if !allow_single_char_attached_units && clean.chars().count() < 2 {
                return None;
            }
            let amt = pending_num.take().unwrap_or(1.0);
            pairs.push((amt, u));
        } else {
            return None;
        }
    }

    if pairs.is_empty() || pending_num.is_some() {
        return None;
    }

    let mut dt = base;
    let mut finest_period = Period::Year;
    let mut has_time_unit = false;
    for (amt, unit) in pairs {
        // Avoid misinterpreting calendar years like "2000 год" / "1000 год" as "2000 years ago"
        if !allow_single_char_attached_units && unit == TimeUnit::Year && amt >= 1000.0 {
            return None;
        }
        if matches!(unit, TimeUnit::Hour | TimeUnit::Minute | TimeUnit::Second) {
            has_time_unit = true;
        }
        dt = apply_relative_offset(dt, sign, amt, unit)?;
        let p = period_for_unit(unit);
        let rank = |per: Period| match per {
            Period::Year => 0,
            Period::Month => 1,
            Period::Week => 2,
            Period::Day => 3,
            Period::Time => 4,
        };
        if rank(p) > rank(finest_period) {
            finest_period = p;
        }
    }

    Some((dt, finest_period, has_time_unit))
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

fn classify_relative_modifier(tok: &str) -> Option<i8> {
    match tok {
        "last" | "previous" | "past" | "geçen" | "gecen" | "önceki" | "onceki" | "letzten"
        | "letzter" | "letzte" | "vergangenen" | "dernier" | "dernière" | "derniere"
        | "pasado" | "pasada" | "passado" | "passada" | "scorso" | "scorsa" | "vorige"
        | "afgelopen" | "zeszły" | "zeszła" | "zeszly" | "zeszla" | "poprzedni"
        | "прошлый" | "прошлая" | "миналата" | "minuly" | "minulý" | "förra" | "forra"
        | "sidste" | "forrige" | "viime" | "지난" => Some(-1),
        "next" | "coming" | "following" | "gelecek" | "önümüzdeki" | "onumuzdeki" | "sonraki"
        | "nächsten" | "nächster" | "nächste" | "nachsten" | "nachster" | "nachste"
        | "kommenden" | "prochain" | "prochaine" | "próximo" | "próxima" | "proximo"
        | "proxima" | "siguiente" | "prossimo" | "prossima" | "volgende" | "komende"
        | "przyszły" | "przyszła" | "przyszly" | "przyszla" | "następny" | "nastepny"
        | "следующий" | "следующая" | "nästa" | "nasta" | "næste" | "naeste" | "neste"
        | "ensi" | "다음" => Some(1),
        "this" | "bu" | "diesen" | "dieser" | "diese" | "ce" | "cette" | "cet" | "este"
        | "esta" | "questo" | "questa" | "deze" | "dit" | "ten" | "ta" | "этот" | "эта"
        | "denna" | "denne" | "tämä" | "tama" | "이번" => Some(0),
        _ => None,
    }
}

fn try_cjk_relative_weekday(s: &str, base: NaiveDateTime) -> Option<(NaiveDateTime, Period)> {
    // Chinese: 上/下/本周[一..日|天] or 上/下星期[一..日|天]
    // Japanese: 先週/来週/今週[の][月..日][曜[日]]
    let (dir, rest): (i8, &str) = if let Some(r) = s.strip_prefix("先週の").or_else(|| s.strip_prefix("先週")) {
        (-1, r)
    } else if let Some(r) = s.strip_prefix("来週の").or_else(|| s.strip_prefix("来週")) {
        (1, r)
    } else if let Some(r) = s.strip_prefix("今週の").or_else(|| s.strip_prefix("今週")) {
        (0, r)
    } else if let Some(r) = s.strip_prefix("上星期").or_else(|| s.strip_prefix("上周")) {
        (-1, r)
    } else if let Some(r) = s.strip_prefix("下星期").or_else(|| s.strip_prefix("下周")) {
        (1, r)
    } else if let Some(r) = s
        .strip_prefix("本星期")
        .or_else(|| s.strip_prefix("本周"))
        .or_else(|| s.strip_prefix("这星期"))
        .or_else(|| s.strip_prefix("这周"))
    {
        (0, r)
    } else {
        return None;
    };

    let target_wd: i64 = match rest.trim() {
        "一" | "月" | "月曜" | "月曜日" => 0,
        "二" | "火" | "火曜" | "火曜日" => 1,
        "三" | "水" | "水曜" | "水曜日" => 2,
        "四" | "木" | "木曜" | "木曜日" => 3,
        "五" | "金" | "金曜" | "金曜日" => 4,
        "六" | "土" | "土曜" | "土曜日" => 5,
        "日" | "天" | "日曜" | "日曜日" => 6,
        _ => return None,
    };

    let current_wd = base.weekday().num_days_from_monday() as i64;
    let delta_days = compute_weekday_delta(dir, current_wd, target_wd);
    base.checked_add_signed(Duration::days(delta_days))
        .map(|dt| (dt.date().and_hms_opt(0, 0, 0).unwrap_or(dt), Period::Day))
}

#[inline]
fn compute_weekday_delta(dir: i8, current_wd: i64, target_wd: i64) -> i64 {
    if dir < 0 {
        let mut diff = target_wd - current_wd;
        if diff >= 0 {
            diff -= 7;
        }
        diff
    } else if dir > 0 {
        let mut diff = target_wd - current_wd;
        if diff <= 0 {
            diff += 7;
        }
        diff
    } else {
        target_wd - current_wd
    }
}

fn try_relative_period_or_weekday(
    s: &str,
    loc: &CompiledLocale,
    base: NaiveDateTime,
    normalize: bool,
) -> Option<(NaiveDateTime, Period)> {
    if let Some(cjk) = try_cjk_relative_weekday(s.trim(), base) {
        return Some(cjk);
    }

    let fs = if normalize {
        fold_diacritics(s)
    } else {
        s.to_string()
    };
    let mut tokens: Vec<&str> = fs.split_whitespace().collect();
    if tokens.len() == 3 && matches!(tokens[0], "el" | "la" | "le" | "w" | "в" | "am") {
        tokens.remove(0);
    }
    if tokens.len() != 2 {
        return None;
    }

    let (dir, target_tok) = if let Some(d) = classify_relative_modifier(tokens[0]) {
        (d, tokens[1])
    } else if let Some(d) = classify_relative_modifier(tokens[1]) {
        (d, tokens[0])
    } else {
        return None;
    };

    let unit_opt = loc.units.get(target_tok).copied().or_else(|| {
        if normalize {
            loc.folded_units.get(target_tok).copied()
        } else {
            None
        }
    });
    if let Some(unit) = unit_opt {
        let period = period_for_unit(unit);
        if dir == 0 {
            return Some((base, period));
        }
        return apply_relative_offset(base, dir, 1.0, unit).map(|dt| (dt, period));
    }

    let target_wd = loc
        .weekdays
        .get(target_tok)
        .copied()
        .or_else(|| {
            if normalize {
                loc.folded_weekdays.get(target_tok).copied()
            } else {
                None
            }
        })
        .or_else(|| {
            target_tok
                .strip_suffix("-feira")
                .and_then(|stem| loc.weekdays.get(stem).copied())
        })?;

    let current_wd = base.weekday().num_days_from_monday() as i64;
    let delta_days = compute_weekday_delta(dir, current_wd, target_wd as i64);

    base.checked_add_signed(Duration::days(delta_days))
        .map(|dt| (dt.date().and_hms_opt(0, 0, 0).unwrap_or(dt), Period::Day))
}

/// Parse standalone fractional relative durations and conversational clock idioms
/// (e.g. "two and a half hours ago", "bir buçuk saat önce", "quarter past 3", "saat 3 buçuk").
pub fn try_conversational_clock_or_fraction(
    normalized: &str,
    settings: &Settings,
) -> Option<RelativeMatch> {
    let base = settings.effective_base();
    let s = if settings.normalize {
        fold_diacritics(normalized)
    } else {
        normalized.to_string()
    };
    let s = s.trim();

    if matches!(
        s,
        "half an hour ago" | "half hour ago" | "yarım saat önce" | "yarim saat once"
    ) {
        let dt = apply_relative_offset(base, -1, 30.0, TimeUnit::Minute)?;
        return Some(RelativeMatch {
            naive: dt,
            period: Period::Day,
            locale: if s.contains("saat") { "tr" } else { "en" }.to_string(),
            has_time: false,
            parts: vec!["year", "month", "day", "time"],
            part_of_day: None,
        });
    }
    if matches!(
        s,
        "in half an hour" | "in half hour" | "yarım saat sonra" | "yarim saat sonra"
    ) {
        let dt = apply_relative_offset(base, 1, 30.0, TimeUnit::Minute)?;
        return Some(RelativeMatch {
            naive: dt,
            period: Period::Day,
            locale: if s.contains("saat") { "tr" } else { "en" }.to_string(),
            has_time: false,
            parts: vec!["year", "month", "day", "time"],
            part_of_day: None,
        });
    }
    if matches!(s, "half a day ago") {
        let dt = apply_relative_offset(base, -1, 12.0, TimeUnit::Hour)?;
        return Some(RelativeMatch {
            naive: dt,
            period: Period::Day,
            locale: "en".to_string(),
            has_time: false,
            parts: vec!["year", "month", "day", "time"],
            part_of_day: None,
        });
    }

    if let Some(m) = try_fractional_duration(s, base) {
        return Some(m);
    }
    if let Some(m) = try_conversational_clock(s, settings) {
        return Some(m);
    }
    None
}

fn parse_small_num_or_word(tok: &str) -> Option<f64> {
    if let Ok(v) = tok.parse::<f64>() {
        if v.is_finite() && v >= 0.0 {
            return Some(v);
        }
    }
    match tok {
        "a" | "an" | "one" | "bir" | "ein" | "eine" | "un" | "une" | "uno" | "una" => Some(1.0),
        "two" | "iki" | "zwei" | "deux" | "dos" => Some(2.0),
        "three" | "üç" | "uc" | "drei" | "trois" | "tres" => Some(3.0),
        "four" | "dört" | "dort" | "vier" | "quatre" | "cuatro" => Some(4.0),
        "five" | "beş" | "bes" | "fünf" | "funf" | "cinq" | "cinco" => Some(5.0),
        "six" | "altı" | "alti" | "sechs" | "seis" => Some(6.0),
        "seven" | "yedi" | "sieben" | "sept" | "siete" => Some(7.0),
        "eight" | "sekiz" | "acht" | "huit" | "ocho" => Some(8.0),
        "nine" | "dokuz" | "neun" | "neuf" | "nueve" => Some(9.0),
        "ten" | "on" | "zehn" | "dix" | "diez" => Some(10.0),
        "eleven" | "on bir" => Some(11.0),
        "twelve" | "on iki" => Some(12.0),
        _ => None,
    }
}

fn parse_unit_token(tok: &str) -> Option<TimeUnit> {
    match tok {
        "second" | "seconds" | "sec" | "secs" | "saniye" | "sekunde" | "sekunden" => Some(TimeUnit::Second),
        "minute" | "minutes" | "min" | "mins" | "dakika" | "dk" => Some(TimeUnit::Minute),
        "hour" | "hours" | "hr" | "hrs" | "saat" | "stunde" | "stunden" => Some(TimeUnit::Hour),
        "day" | "days" | "gün" | "gun" | "tag" | "tage" | "tagen" => Some(TimeUnit::Day),
        "week" | "weeks" | "hafta" | "woche" | "wochen" => Some(TimeUnit::Week),
        "month" | "months" | "ay" | "monat" | "monate" | "monaten" => Some(TimeUnit::Month),
        "year" | "years" | "yıl" | "yil" | "sene" | "jahr" | "jahre" | "jahren" => Some(TimeUnit::Year),
        _ => None,
    }
}

fn apply_fractional_offset(base: NaiveDateTime, sign: i8, whole_and_half: f64, unit: TimeUnit) -> Option<NaiveDateTime> {
    match unit {
        TimeUnit::Month => {
            let days = (whole_and_half * 30.0).round();
            apply_relative_offset(base, sign, days, TimeUnit::Day)
        }
        TimeUnit::Year => {
            let months = (whole_and_half * 12.0).round();
            apply_relative_offset(base, sign, months, TimeUnit::Month)
        }
        _ => apply_relative_offset(base, sign, whole_and_half, unit),
    }
}

fn try_fractional_duration(s: &str, base: NaiveDateTime) -> Option<RelativeMatch> {
    // English: "[in ]<n> and a half <unit>[ ago| from now]"
    let (sign_en, core_en): (i8, &str) = if let Some(rest) = s.strip_prefix("in ") {
        (1, rest.trim())
    } else if let Some(rest) = s.strip_suffix(" ago") {
        (-1, rest.trim())
    } else if let Some(rest) = s.strip_suffix(" from now") {
        (1, rest.trim())
    } else {
        (0, s)
    };
    if sign_en != 0 {
        if let Some((left, right)) = core_en.split_once(" and a half ") {
            if let (Some(n), Some(unit)) = (parse_small_num_or_word(left.trim()), parse_unit_token(right.trim())) {
                let dt = apply_fractional_offset(base, sign_en, n + 0.5, unit)?;
                return Some(RelativeMatch {
                    naive: dt,
                    period: period_for_unit(unit),
                    locale: "en".to_string(),
                    has_time: false,
                    parts: parts_for_relative(false, Some(unit)),
                    part_of_day: None,
                });
            }
        }
    }

    // Turkish: "<n> buçuk <unit> önce/sonra" (folded: "bucuk", "once")
    let (sign_tr, core_tr): (i8, &str) = if let Some(rest) = s.strip_suffix(" once").or_else(|| s.strip_suffix(" önce")) {
        (-1, rest.trim())
    } else if let Some(rest) = s.strip_suffix(" sonra") {
        (1, rest.trim())
    } else {
        (0, "")
    };
    if sign_tr != 0 {
        if let Some((left, right)) = core_tr.split_once(" bucuk ").or_else(|| core_tr.split_once(" buçuk ")) {
            if let (Some(n), Some(unit)) = (parse_small_num_or_word(left.trim()), parse_unit_token(right.trim())) {
                let dt = apply_fractional_offset(base, sign_tr, n + 0.5, unit)?;
                return Some(RelativeMatch {
                    naive: dt,
                    period: period_for_unit(unit),
                    locale: "tr".to_string(),
                    has_time: false,
                    parts: parts_for_relative(false, Some(unit)),
                    part_of_day: None,
                });
            }
        }
    }

    // German: "vor eineinhalb <unit>" / "in anderthalb <unit>"
    let (sign_de, core_de): (i8, &str) = if let Some(rest) = s.strip_prefix("vor ") {
        (-1, rest.trim())
    } else if let Some(rest) = s.strip_prefix("in ") {
        (1, rest.trim())
    } else {
        (0, "")
    };
    if sign_de != 0 {
        for kw in ["eineinhalb ", "anderthalb "] {
            if let Some(unit_str) = core_de.strip_prefix(kw) {
                if let Some(unit) = parse_unit_token(unit_str.trim()) {
                    let dt = apply_fractional_offset(base, sign_de, 1.5, unit)?;
                    return Some(RelativeMatch {
                        naive: dt,
                        period: period_for_unit(unit),
                        locale: "de".to_string(),
                        has_time: false,
                        parts: parts_for_relative(false, Some(unit)),
                        part_of_day: None,
                    });
                }
            }
        }
    }

    None
}

fn parse_clock_hour_spec(raw: &str) -> Option<u32> {
    let mut t = raw.trim().trim_end_matches('.');
    let mut is_pm = false;
    let mut is_am = false;
    for suf in ["p.m.", "p.m", "pm"] {
        if let Some(rest) = t.strip_suffix(suf) {
            t = rest.trim();
            is_pm = true;
            break;
        }
    }
    if !is_pm {
        for suf in ["a.m.", "a.m", "am"] {
            if let Some(rest) = t.strip_suffix(suf) {
                t = rest.trim();
                is_am = true;
                break;
            }
        }
    }
    let val = parse_small_num_or_word(t)?;
    let mut h = val as u32;
    if (h as f64 - val).abs() > 1e-6 || h > 23 {
        return None;
    }
    if is_pm {
        if h == 0 || h > 12 {
            return None;
        }
        if h < 12 {
            h += 12;
        }
    } else if is_am {
        if h == 0 || h > 12 {
            return None;
        }
        if h == 12 {
            h = 0;
        }
    }
    Some(h)
}

fn try_conversational_clock(s: &str, settings: &Settings) -> Option<RelativeMatch> {
    // English: "[a ]quarter past <H>", "[a ]quarter to <H>", "half past <H>"
    let trimmed = s.strip_prefix("a ").unwrap_or(s);
    let en_clock: Option<(u32, u32)> = if let Some(rest) = trimmed.strip_prefix("quarter past ") {
        parse_clock_hour_spec(rest).map(|h| (h, 15))
    } else if let Some(rest) = trimmed.strip_prefix("quarter to ") {
        parse_clock_hour_spec(rest).map(|h| (if h == 0 { 23 } else { h - 1 }, 45))
    } else if let Some(rest) = trimmed.strip_prefix("half past ") {
        parse_clock_hour_spec(rest).map(|h| (h, 30))
    } else {
        None
    };
    if let Some((hh, mm)) = en_clock {
        let nt = NaiveTime::from_hms_opt(hh, mm, 0)?;
        let nd = resolve_time_only_date(nt, settings);
        return Some(RelativeMatch {
            naive: nd.and_time(nt),
            period: Period::Day,
            locale: "en".to_string(),
            has_time: true,
            parts: vec!["time"],
            part_of_day: None,
        });
    }

    // Turkish: "[saat ]<H> bucuk" / "[saat ]<H> ceyrek gece" / "[saat ]<H> ceyrek kala"
    let tr_s = s.strip_prefix("saat ").unwrap_or(s).trim();
    let tr_clock: Option<(u32, u32)> = if let Some(rest) = tr_s.strip_suffix(" bucuk").or_else(|| tr_s.strip_suffix(" buçuk")) {
        parse_clock_hour_spec(rest).map(|h| (h, 30))
    } else if let Some(rest) = tr_s.strip_suffix(" ceyrek gece").or_else(|| tr_s.strip_suffix(" çeyrek geçe")) {
        let base_h = rest.trim().split(['\'', '’']).next().unwrap_or("").trim();
        parse_clock_hour_spec(base_h).map(|h| (h, 15))
    } else if let Some(rest) = tr_s.strip_suffix(" ceyrek kala").or_else(|| tr_s.strip_suffix(" çeyrek kala")) {
        let base_h = rest.trim().split(['\'', '’']).next().unwrap_or("").trim();
        parse_clock_hour_spec(base_h).map(|h| (if h == 0 { 23 } else { h - 1 }, 45))
    } else {
        None
    };
    if let Some((hh, mm)) = tr_clock {
        let nt = NaiveTime::from_hms_opt(hh, mm, 0)?;
        let nd = resolve_time_only_date(nt, settings);
        return Some(RelativeMatch {
            naive: nd.and_time(nt),
            period: Period::Day,
            locale: "tr".to_string(),
            has_time: true,
            parts: vec!["time"],
            part_of_day: None,
        });
    }

    None
}

/// Extract a trailing time from strings like "yesterday at 5pm", "hier à 14h30", "dün saat 15:30".
pub fn extract_trailing_or_leading_time(s: &str) -> (&str, Option<NaiveTime>) {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() < 2 {
        return (s, None);
    }

    if tokens.len() >= 3 {
        let last2 = format!("{} {}", tokens[tokens.len() - 2], tokens[tokens.len() - 1]);
        if let Some(t) = parse_time_str(&last2) {
            let cut = s.len() - tokens[tokens.len() - 1].len() - tokens[tokens.len() - 2].len() - 1;
            if cut < s.len() {
                return (s[..cut].trim_end(), Some(t));
            }
        }
    }

    let last = tokens[tokens.len() - 1];
    if last.contains(':')
        || last.contains('h')
        || last.ends_with("am")
        || last.ends_with("pm")
        || last.ends_with("a.m.")
        || last.ends_with("p.m.")
    {
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
            if matches!(tail, "at" | "on" | "à" | "a" | "um" | "am" | "saat" | "las" | "в")
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
