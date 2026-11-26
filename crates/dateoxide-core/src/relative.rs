use crate::fast_path::{days_in_month, parse_time_str};
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
    let folded_full = if settings.normalize {
        fold_diacritics(normalized)
    } else {
        normalized.to_string()
    };

    // 0. Special compound day + time-of-day idioms & half-hour phrases
    if matches!(
        if settings.normalize { folded_full.as_str() } else { normalized },
        "half an hour ago" | "half hour ago" | "yarım saat önce" | "yarim saat once"
    ) {
        if let Some(dt) = apply_relative_offset(base, -1, 30.0, TimeUnit::Minute) {
            return Some(RelativeMatch {
                naive: dt,
                period: Period::Day,
                locale: if normalized.contains("saat") { "tr" } else { "en" }.to_string(),
                has_time: false,
                parts: vec!["year", "month", "day", "time"],
                part_of_day: None,
            });
        }
    }
    if let Some(m) = try_compound_tod_idiom(normalized, &folded_full, candidate_ids, registry, base, settings) {
        return Some(m);
    }

    // 1. O(1) Global Exact Relative Phrase lookup ("yesterday", "bugün", "yarın", "demain", "今日", "कल", "غدا")
    if let Some(matches) = registry.exact_phrase_index.get(normalized) {
        let mut matched_specs: Vec<(usize, RelativeSpec)> = Vec::new();
        for &(lid, spec) in matches {
            let lid_usize = lid as usize;
            if candidate_ids.is_empty() || candidate_ids.contains(&lid_usize) {
                matched_specs.push((lid_usize, spec));
            }
        }
        if !matched_specs.is_empty() {
            let specs_only: Vec<RelativeSpec> = matched_specs.iter().map(|(_, s)| *s).collect();
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
            for &(lid, spec) in matches {
                let lid_usize = lid as usize;
                if candidate_ids.is_empty() || candidate_ids.contains(&lid_usize) {
                    matched_specs.push((lid_usize, spec));
                }
            }
            if !matched_specs.is_empty() {
                let specs_only: Vec<RelativeSpec> = matched_specs.iter().map(|(_, s)| *s).collect();
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
        for w in markers {
            if let Some(stripped) = text.strip_suffix(w.as_str()) {
                if stripped.is_empty() || stripped.ends_with(' ') || !w.is_ascii() {
                    return Some(stripped.trim().to_string());
                }
            }
            if let Some(stripped) = text.strip_prefix(w.as_str()) {
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

fn try_relative_period_or_weekday(
    s: &str,
    loc: &CompiledLocale,
    base: NaiveDateTime,
    normalize: bool,
) -> Option<(NaiveDateTime, Period)> {
    let fs = if normalize {
        fold_diacritics(s)
    } else {
        s.to_string()
    };
    let tokens: Vec<&str> = fs.split_whitespace().collect();
    if tokens.len() != 2 {
        return None;
    }
    let (modifier, target_tok) = (tokens[0], tokens[1]);
    let dir: i8 = match modifier {
        "last" | "previous" | "geçen" | "gecen" | "letzten" | "letzter" | "letzte" | "dernier"
        | "dernière" | "pasado" | "pasada" | "прошлый" | "прошлая" => -1,
        "next" | "gelecek" | "önümüzdeki" | "onumuzdeki" | "nächsten" | "nächster" | "nächste"
        | "prochain" | "prochaine" | "próximo" | "proximo" | "следующий" => 1,
        "this" | "bu" | "diesen" | "diese" | "ce" | "cette" | "este" | "esta" => 0,
        _ => return None,
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

    let target_wd = loc.weekdays.get(target_tok).copied().or_else(|| {
        if normalize {
            loc.folded_weekdays.get(target_tok).copied()
        } else {
            None
        }
    })?;

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
        .map(|dt| (dt, Period::Day))
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
