use crate::locale::{fold_diacritics, normalize_digits_and_lower, CompiledLocale, LocaleRegistry};
use crate::settings::{
    DateOrder, DatePart, ParserKind, Period, PreferDatesFrom, PreferMonthOfYear, Settings,
    StrictDateOrder,
};
use crate::timezone::{extract_trailing_timezone_with_abbrevs, TzSpec};
use chrono::format::{parse as chrono_parse, Parsed, StrftimeItems};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Weekday};

#[derive(Debug, Clone)]
pub struct FastPathMatch {
    pub naive: NaiveDateTime,
    pub tz: Option<TzSpec>,
    pub period: Period,
    pub has_time: bool,
    pub parts: Vec<&'static str>,
    pub locale: Option<String>,
}

const ENG_MONTHS_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const ENG_MONTHS_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

const ENG_WEEKDAYS_FULL: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

const ENG_WEEKDAYS_ABBR: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

fn try_strptime_any(
    s: &str,
    fmt: &str,
    locale: Option<String>,
    settings: &Settings,
) -> Option<FastPathMatch> {
    let eff_fmt_owned: String;
    let eff_fmt: &str = if fmt.contains(".%f") {
        if !s.contains('.') {
            return None;
        }
        eff_fmt_owned = fmt.replace(".%f", "%.f");
        &eff_fmt_owned
    } else {
        fmt
    };

    if let Ok(dt) = DateTime::parse_from_str(s, eff_fmt) {
        return Some(FastPathMatch {
            naive: dt.naive_local(),
            tz: Some(TzSpec::Fixed(*dt.offset())),
            period: Period::Day,
            has_time: true,
            parts: vec!["year", "month", "day", "time"],
            locale,
        });
    }
    if let Ok(ndt) = NaiveDateTime::parse_from_str(s, eff_fmt) {
        return Some(FastPathMatch {
            naive: ndt,
            tz: None,
            period: Period::Day,
            has_time: true,
            parts: vec!["year", "month", "day", "time"],
            locale,
        });
    }
    if let Ok(nd) = NaiveDate::parse_from_str(s, eff_fmt) {
        return Some(FastPathMatch {
            naive: nd.and_hms_opt(0, 0, 0)?,
            tz: None,
            period: Period::Day,
            has_time: false,
            parts: vec!["year", "month", "day"],
            locale,
        });
    }

    // Partial strptime fallback via chrono::format::Parsed (e.g. "%B %Y", "%Y-%m", "%d %B", "%H:%M", "%Y-%j")
    let mut parsed = Parsed::new();
    chrono_parse(&mut parsed, s, StrftimeItems::new(eff_fmt)).ok()?;

    let has_year = parsed.year.is_some();
    let has_month = parsed.month.is_some();
    let has_day = parsed.day.is_some();
    let has_ordinal = parsed.ordinal.is_some();
    let has_time = parsed.hour_div_12.is_some()
        || parsed.hour_mod_12.is_some()
        || parsed.minute.is_some()
        || parsed.second.is_some();

    if !has_year && !has_month && !has_day && !has_ordinal && !has_time {
        return None;
    }

    if settings.strict_parsing && (!has_year || (!(has_month && has_day) && !has_ordinal)) {
        return None;
    }
    if settings.require_parts.contains(&DatePart::Year) && !has_year {
        return None;
    }
    if settings.require_parts.contains(&DatePart::Month) && !has_month && !has_ordinal {
        return None;
    }
    if settings.require_parts.contains(&DatePart::Day) && !has_day && !has_ordinal {
        return None;
    }

    let time = if has_time {
        if parsed.minute.is_none() {
            parsed.minute = Some(0);
        }
        if parsed.second.is_none() {
            parsed.second = Some(0);
        }
        parsed.to_naive_time().ok()?
    } else {
        NaiveTime::from_hms_opt(0, 0, 0)?
    };

    let base = settings.effective_base();
    let nd = if has_ordinal {
        let y = parsed.year.unwrap_or(base.year());
        if !(1..=9999).contains(&y) {
            return None;
        }
        NaiveDate::from_yo_opt(y, parsed.ordinal?)?
    } else if !has_year && !has_month && !has_day && has_time {
        resolve_time_only_date(time, settings)
    } else {
        let mut y = parsed.year.unwrap_or(base.year());
        if !(1..=9999).contains(&y) {
            return None;
        }
        let m = parsed.month.unwrap_or_else(|| match settings.prefer_month_of_year {
            PreferMonthOfYear::First => 1,
            PreferMonthOfYear::Last => 12,
            PreferMonthOfYear::Current => base.month(),
        });
        if !(1..=12).contains(&m) {
            return None;
        }
        let d = parsed
            .day
            .unwrap_or_else(|| resolve_missing_day(y, m, base.day(), settings.prefer_day_of_month));
        if d < 1 || d > days_in_month(y, m) {
            return None;
        }
        if !has_year {
            if let Some(cand) = NaiveDate::from_ymd_opt(y, m, d) {
                match settings.prefer_dates_from {
                    PreferDatesFrom::Past if cand > base.date() => y -= 1,
                    PreferDatesFrom::Future if cand < base.date() => y += 1,
                    _ => {}
                }
            }
        }
        let final_d = d.min(days_in_month(y, m));
        NaiveDate::from_ymd_opt(y, m, final_d)?
    };

    let tz = parsed.offset.and_then(FixedOffset::east_opt).map(TzSpec::Fixed);
    let period = if has_day || has_ordinal || (!has_year && !has_month && has_time) {
        Period::Day
    } else if has_month {
        Period::Month
    } else {
        Period::Year
    };

    let mut parts = Vec::with_capacity(4);
    if has_year {
        parts.push("year");
    }
    if has_month || has_ordinal {
        parts.push("month");
    }
    if has_day || has_ordinal {
        parts.push("day");
    }
    if has_time {
        parts.push("time");
    }

    Some(FastPathMatch {
        naive: nd.and_time(time),
        tz,
        period,
        has_time,
        parts,
        locale,
    })
}

fn translate_input_for_strptime(
    norm: &str,
    fmt: &str,
    loc: &CompiledLocale,
    use_folded: bool,
) -> Option<String> {
    let want_full_month = fmt.contains("%B");
    let want_abbr_month = fmt.contains("%b") || fmt.contains("%h");
    let want_full_wd = fmt.contains("%A");
    let want_abbr_wd = fmt.contains("%a");
    let want_ampm = fmt.contains("%p") || fmt.contains("%P");

    if !want_full_month && !want_abbr_month && !want_full_wd && !want_abbr_wd && !want_ampm {
        return None;
    }

    let mut pairs: Vec<(&str, &'static str)> = Vec::new();
    if want_full_month || want_abbr_month {
        let map = if use_folded { &loc.folded_months } else { &loc.months };
        for (name, &m) in map {
            if (1..=12).contains(&m) {
                let idx = (m - 1) as usize;
                let rep = if want_full_month {
                    ENG_MONTHS_FULL[idx]
                } else {
                    ENG_MONTHS_ABBR[idx]
                };
                pairs.push((name.as_str(), rep));
            }
        }
    }
    if want_full_wd || want_abbr_wd {
        let map = if use_folded {
            &loc.folded_weekdays
        } else {
            &loc.weekdays
        };
        for (name, &w) in map {
            if (w as usize) < 7 {
                let rep = if want_full_wd {
                    ENG_WEEKDAYS_FULL[w as usize]
                } else {
                    ENG_WEEKDAYS_ABBR[w as usize]
                };
                pairs.push((name.as_str(), rep));
            }
        }
    }
    if want_ampm && !use_folded {
        for name in &loc.am {
            pairs.push((name.as_str(), "AM"));
        }
        for name in &loc.pm {
            pairs.push((name.as_str(), "PM"));
        }
    }

    if pairs.is_empty() {
        return None;
    }
    pairs.sort_unstable_by_key(|a| std::cmp::Reverse(a.0.len()));

    let mut out = String::with_capacity(norm.len() + 8);
    let mut pos = 0usize;
    let mut changed = false;

    while pos < norm.len() {
        let prev_is_alpha = norm[..pos]
            .chars()
            .next_back()
            .map(|c| c.is_alphabetic())
            .unwrap_or(false);

        let mut matched: Option<(usize, &str)> = None;
        if !prev_is_alpha {
            let rest = &norm[pos..];
            for &(pat, rep) in &pairs {
                if let Some(after) = rest.strip_prefix(pat) {
                    let next_is_alpha = after.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false);
                    if !next_is_alpha {
                        matched = Some((pat.len(), rep));
                        break;
                    }
                }
            }
        }

        if let Some((plen, rep)) = matched {
            out.push_str(rep);
            pos += plen;
            changed = true;
        } else {
            let ch = norm[pos..].chars().next().unwrap();
            out.push(ch);
            pos += ch.len_utf8();
        }
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

/// Try explicit user-provided strptime formats (`date_formats`).
pub fn try_custom_formats(input: &str, settings: &Settings) -> Option<FastPathMatch> {
    if !settings.allows_parser(ParserKind::CustomFormats) {
        return None;
    }
    let formats = settings.date_formats.as_deref()?;
    if formats.is_empty() {
        return None;
    }
    // Localized strptime fallback when formats use locale-dependent directives (%B/%b/%A/%a/%p)
    let needs_locale = formats.iter().any(|f| {
        f.contains("%B")
            || f.contains("%b")
            || f.contains("%h")
            || f.contains("%A")
            || f.contains("%a")
            || f.contains("%p")
            || f.contains("%P")
    });
    let try_english_pass = !needs_locale || crate::locale::settings_allows_locale(settings, "en");
    let s = input.trim();
    if try_english_pass {
        for fmt in formats {
            if let Some(m) = try_strptime_any(s, fmt, Some("en".to_string()), settings) {
                return Some(m);
            }
        }
    }

    if !needs_locale {
        return None;
    }

    let norm = normalize_digits_and_lower(s);
    if norm.is_empty() {
        return None;
    }

    // Fast retry for Unicode digits in case format had numeric-only mismatch? No, focus on localized names.
    let registry = LocaleRegistry::global();
    let tokens: Vec<&str> = norm
        .split(|c: char| {
            c.is_whitespace() || c == ',' || c == '/' || c == '-' || c == '.' || c == '[' || c == ']'
        })
        .map(|t| t.trim_matches(|c: char| c == '.' || c == '(' || c == ')'))
        .filter(|t| !t.is_empty())
        .collect();
    let candidates = registry.detect_candidates(
        &norm,
        &tokens,
        settings.languages.as_deref(),
        &settings.default_languages,
        settings.normalize,
        settings.use_given_language_order,
    );

    let folded_norm = if settings.normalize {
        Some(fold_diacritics(&norm))
    } else {
        None
    };

    for &lid in &candidates {
        let loc = &registry.locales[lid];
        for fmt in formats {
            if let Some(translated) = translate_input_for_strptime(&norm, fmt, loc, false) {
                if let Some(m) = try_strptime_any(&translated, fmt, Some(loc.code.clone()), settings) {
                    return Some(m);
                }
            }
            if let Some(ref fn_s) = folded_norm {
                if let Some(translated) = translate_input_for_strptime(fn_s, fmt, loc, true) {
                    if let Some(m) = try_strptime_any(&translated, fmt, Some(loc.code.clone()), settings) {
                        return Some(m);
                    }
                }
            }
        }
    }

    None
}

/// Zero-regex, zero-allocation fast path for ISO-8601 (extended & compact), RFC-2822/3339,
/// delimited numeric dates, standalone times, Unix timestamps, and compact YYYYMMDD dates.
pub fn try_fast_path(input: &str, settings: &Settings) -> Option<FastPathMatch> {
    let s = input.trim();
    if s.is_empty() || s.len() > 80 {
        return None;
    }

    let allow_base = settings.allows_parser(ParserKind::BaseFormats)
        || settings.allows_parser(ParserKind::AbsoluteTime);

    // 1. RFC 2822 (e.g., "Thu, 26 Nov 2026 15:30:00 +0300")
    if allow_base && s.len() >= 16 && s.contains(',') {
        if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
            return Some(FastPathMatch {
                naive: dt.naive_local(),
                tz: Some(TzSpec::Fixed(*dt.offset())),
                period: Period::Day,
                has_time: true,
                parts: vec!["year", "month", "day", "time"],
                locale: None,
            });
        }
    }

    // 2. RFC 3339 (e.g., "2026-11-26T15:30:00Z" or "2026-11-26T15:30:00.123+03:00")
    if allow_base
        && s.len() >= 19
        && s.as_bytes().get(4) == Some(&b'-')
        && s.as_bytes().get(7) == Some(&b'-')
    {
        if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
            return Some(FastPathMatch {
                naive: dt.naive_local(),
                tz: Some(TzSpec::Fixed(*dt.offset())),
                period: Period::Day,
                has_time: true,
                parts: vec!["year", "month", "day", "time"],
                locale: None,
            });
        }
    }

    // Peel off optional trailing timezone (including custom abbreviations) before numeric checks
    let (core, tz) = extract_trailing_timezone_with_abbrevs(s, &settings.timezone_abbreviations);
    if core.is_empty() {
        return None;
    }

    // 3. Compact ISO-8601 basic format: YYYYMMDDTHHMMSS[.frac] (e.g., "20261126T153000Z")
    if allow_base {
        if let Some(m) = try_compact_iso_datetime(core, tz) {
            return Some(m);
        }
        if let Some(m) = try_iso_week_date(core, tz, settings) {
            return Some(m);
        }
    }

    // 3b. Negative Unix timestamp (only when "negative-timestamp" is explicitly in PARSERS)
    if core.starts_with('-') && settings.allows_parser(ParserKind::NegativeTimestamp) {
        if let Some(m) = try_negative_timestamp(core, tz, settings) {
            return Some(m);
        }
    }

    // 4. Pure ASCII digits (Unix timestamp or compact YYYYMMDD)
    if core.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        if let Some(m) = try_timestamp_or_compact(core, tz, settings) {
            return Some(m);
        }
    }

    // 5. ISO-like & delimited numeric dates / standalone times
    if allow_base && core.as_bytes()[0].is_ascii_digit() {
        if let Some(m) = try_numeric_datetime(core, tz, settings) {
            return Some(m);
        }
    }

    // 6. Prefixed standalone times ("at 5:30 PM", "saat 14:30", "alle 18:00")
    if allow_base {
        if let Some(m) = try_prefixed_time_only(core, tz, settings) {
            return Some(m);
        }
    }

    None
}

fn try_prefixed_time_only(
    core: &str,
    tz: Option<TzSpec>,
    settings: &Settings,
) -> Option<FastPathMatch> {
    if settings.strict_parsing
        || settings.require_parts.contains(&DatePart::Year)
        || settings.require_parts.contains(&DatePart::Month)
        || settings.require_parts.contains(&DatePart::Day)
    {
        return None;
    }

    let lower = core.to_lowercase();
    let prefixes: &[(&str, &[&str])] = &[
        ("a las ", &["es", "gl"]),
        ("alle ", &["it", "de"]),
        ("saat ", &["tr", "az", "id", "ms"]),
        ("klo ", &["fi"]),
        ("kl. ", &["sv", "da", "nb", "nn", "no", "is"]),
        ("at ", &["en"]),
        ("um ", &["de"]),
        ("om ", &["nl", "af"]),
        ("kl ", &["sv", "da", "nb", "nn", "no"]),
        ("às ", &["pt", "gl"]),
        ("à ", &["fr", "pt", "ca"]),
        ("в ", &["ru", "uk", "bg", "be", "sr", "mk"]),
        ("о ", &["uk", "pl"]),
    ];
    let mut rest: Option<&str> = None;
    for &(p, langs) in prefixes {
        if lower.starts_with(p) {
            if settings.languages.is_some()
                && !langs
                    .iter()
                    .any(|&l| crate::locale::settings_allows_locale(settings, l))
            {
                continue;
            }
            let prefix_chars = p.chars().count();
            let sliced = core
                .char_indices()
                .nth(prefix_chars)
                .map(|(idx, _)| core[idx..].trim())
                .unwrap_or("");
            if !sliced.is_empty() {
                rest = Some(sliced);
                break;
            }
        }
    }
    let time_s = rest?;
    let tl = time_s.to_ascii_lowercase();
    let looks_like_time = time_s.contains(':')
        || ((time_s.contains('h') || time_s.contains('H'))
            && time_s.as_bytes().first().map(|b| b.is_ascii_digit()).unwrap_or(false))
        || time_s.contains('시')
        || time_s.contains('時')
        || time_s.contains('时')
        || time_s.contains('点')
        || tl.ends_with("am")
        || tl.ends_with("pm")
        || tl.ends_with("a.m.")
        || tl.ends_with("p.m.")
        || tl.ends_with("a.m")
        || tl.ends_with("p.m");
    if !looks_like_time {
        return None;
    }
    let time = parse_time_str(time_s)?;
    let date = resolve_time_only_date(time, settings);
    Some(FastPathMatch {
        naive: date.and_time(time),
        tz,
        period: Period::Day,
        has_time: true,
        parts: vec!["time"],
        locale: None,
    })
}

fn try_negative_timestamp(
    core: &str,
    tz: Option<TzSpec>,
    settings: &Settings,
) -> Option<FastPathMatch> {
    if settings.strict_parsing {
        return None;
    }
    let abs_str = core.strip_prefix('-')?;
    if abs_str.is_empty() || !abs_str.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
        return None;
    }
    let dt = if let Some((int_part, frac_part)) = abs_str.split_once('.') {
        if !(5..=10).contains(&int_part.len())
            || frac_part.is_empty()
            || frac_part.len() > 9
            || !int_part.bytes().all(|b| b.is_ascii_digit())
            || !frac_part.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let secs: i64 = int_part.parse().ok()?;
        let mut nanos_str = frac_part.to_string();
        while nanos_str.len() < 9 {
            nanos_str.push('0');
        }
        let nanos: u32 = nanos_str.parse().ok()?;
        if nanos == 0 {
            DateTime::from_timestamp(-secs, 0)?
        } else {
            DateTime::from_timestamp(-(secs + 1), 1_000_000_000 - nanos)?
        }
    } else {
        match abs_str.len() {
            5..=10 => {
                let secs: i64 = abs_str.parse().ok()?;
                DateTime::from_timestamp(-secs, 0)?
            }
            13 => {
                let ms: i64 = abs_str.parse().ok()?;
                DateTime::from_timestamp_millis(-ms)?
            }
            16 => {
                let us: i64 = abs_str.parse().ok()?;
                DateTime::from_timestamp_micros(-us)?
            }
            _ => return None,
        }
    };

    if !(1..=9999).contains(&dt.year()) {
        return None;
    }

    Some(FastPathMatch {
        naive: dt.naive_utc(),
        tz: tz.or_else(|| FixedOffset::east_opt(0).map(TzSpec::Fixed)),
        period: Period::Day,
        has_time: true,
        parts: vec!["year", "month", "day", "time"],
        locale: None,
    })
}

fn try_compact_iso_datetime(core: &str, tz: Option<TzSpec>) -> Option<FastPathMatch> {
    let b = core.as_bytes();
    if b.len() < 15 || (b[8] != b'T' && b[8] != b't') {
        return None;
    }
    if !b[0..8].iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let y: i32 = core[0..4].parse().ok()?;
    if !(1..=9999).contains(&y) {
        return None;
    }
    let m: u32 = core[4..6].parse().ok()?;
    let d: u32 = core[6..8].parse().ok()?;
    let nd = NaiveDate::from_ymd_opt(y, m, d)?;

    let time_raw = &core[9..];
    let (hms, nano) = if let Some((main_t, frac)) = time_raw.split_once(['.', ',']) {
        if main_t.len() != 6 || frac.is_empty() || !frac.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let mut padded = [b'0'; 9];
        for (i, ch) in frac.bytes().take(9).enumerate() {
            padded[i] = ch;
        }
        let nn: u32 = std::str::from_utf8(&padded).ok()?.parse().ok()?;
        (main_t, nn)
    } else {
        if time_raw.len() != 6 {
            return None;
        }
        (time_raw, 0u32)
    };

    if !hms.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let hh: u32 = hms[0..2].parse().ok()?;
    let mm: u32 = hms[2..4].parse().ok()?;
    let ss: u32 = hms[4..6].parse().ok()?;
    let nt = NaiveTime::from_hms_nano_opt(hh, mm, ss, nano)?;

    Some(FastPathMatch {
        naive: nd.and_time(nt),
        tz,
        period: Period::Day,
        has_time: true,
        parts: vec!["year", "month", "day", "time"],
        locale: None,
    })
}

pub(crate) fn resolve_time_only_date(time: NaiveTime, settings: &Settings) -> NaiveDate {
    let base = settings.effective_base();
    let mut date = base.date();
    match settings.prefer_dates_from {
        PreferDatesFrom::Future if time < base.time() => {
            if let Some(next) = date.succ_opt() {
                date = next;
            }
        }
        PreferDatesFrom::Past if time > base.time() => {
            if let Some(prev) = date.pred_opt() {
                date = prev;
            }
        }
        _ => {}
    }
    date
}

fn try_iso_week_date(
    core: &str,
    tz: Option<TzSpec>,
    settings: &Settings,
) -> Option<FastPathMatch> {
    let (date_part, time_part) = if let Some(idx) = core.find(['T', 't', ' ']) {
        let (d, rest) = core.split_at(idx);
        let t = rest[1..].trim();
        (d, if t.is_empty() { None } else { Some(t) })
    } else {
        (core, None)
    };

    let bytes = date_part.as_bytes();
    if bytes.len() < 7 || !bytes[0..4].iter().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (year, week, weekday_opt) = if bytes.len() >= 8 && bytes[4] == b'-' && (bytes[5] == b'W' || bytes[5] == b'w') {
        // YYYY-Www or YYYY-Www-D
        let y: i32 = date_part[0..4].parse().ok()?;
        if date_part.len() == 8 && bytes[6..8].iter().all(|b| b.is_ascii_digit()) {
            let w: u32 = date_part[6..8].parse().ok()?;
            (y, w, None)
        } else if date_part.len() == 10
            && bytes[8] == b'-'
            && bytes[6..8].iter().all(|b| b.is_ascii_digit())
            && bytes[9].is_ascii_digit()
        {
            let w: u32 = date_part[6..8].parse().ok()?;
            let d: u32 = date_part[9..10].parse().ok()?;
            (y, w, Some(d))
        } else {
            return None;
        }
    } else if (bytes[4] == b'W' || bytes[4] == b'w') && (date_part.len() == 7 || date_part.len() == 8) {
        // YYYYWww or YYYYWwwD
        let y: i32 = date_part[0..4].parse().ok()?;
        if !bytes[5..].iter().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let w: u32 = date_part[5..7].parse().ok()?;
        let d_opt = if date_part.len() == 8 {
            Some(date_part[7..8].parse::<u32>().ok()?)
        } else {
            None
        };
        (y, w, d_opt)
    } else {
        return None;
    };

    if weekday_opt.is_none() && (settings.strict_parsing || settings.require_parts.contains(&DatePart::Day)) {
        return None;
    }

    let wd = match weekday_opt.unwrap_or(1) {
        1 => Weekday::Mon,
        2 => Weekday::Tue,
        3 => Weekday::Wed,
        4 => Weekday::Thu,
        5 => Weekday::Fri,
        6 => Weekday::Sat,
        7 => Weekday::Sun,
        _ => return None,
    };
    if !(1..=9999).contains(&year) {
        return None;
    }
    let nd = NaiveDate::from_isoywd_opt(year, week, wd)?;
    let has_time = time_part.is_some();
    let time = match time_part {
        Some(tp) => {
            let clean_tp = tp
                .strip_prefix("t ")
                .or_else(|| tp.strip_prefix("T "))
                .or_else(|| tp.strip_prefix("at "))
                .or_else(|| tp.strip_prefix("AT "))
                .or_else(|| tp.strip_prefix("@ "))
                .map(|r| r.trim())
                .unwrap_or(tp);
            parse_time_str(clean_tp)?
        }
        None => NaiveTime::from_hms_opt(0, 0, 0)?,
    };
    let mut parts = vec!["year", "month", "day"];
    if has_time {
        parts.push("time");
    }
    Some(FastPathMatch {
        naive: nd.and_time(time),
        tz,
        period: if weekday_opt.is_some() { Period::Day } else { Period::Week },
        has_time,
        parts,
        locale: None,
    })
}

fn try_timestamp_or_compact(
    core: &str,
    tz: Option<TzSpec>,
    settings: &Settings,
) -> Option<FastPathMatch> {
    if !core.contains('.') {
        match core.len() {
            4 | 6 if settings.parsers.is_some()
                && settings.allows_parser(ParserKind::NoSpacesTime)
                && !settings.allows_parser(ParserKind::AbsoluteTime) =>
            {
                if settings.strict_parsing
                    || settings.require_parts.contains(&DatePart::Year)
                    || settings.require_parts.contains(&DatePart::Month)
                    || settings.require_parts.contains(&DatePart::Day)
                {
                    return None;
                }
                let hh: u32 = core[0..2].parse().ok()?;
                let mm: u32 = core[2..4].parse().ok()?;
                let ss: u32 = if core.len() == 6 { core[4..6].parse().ok()? } else { 0 };
                let time = NaiveTime::from_hms_opt(hh, mm, ss)?;
                let date = resolve_time_only_date(time, settings);
                Some(FastPathMatch {
                    naive: date.and_time(time),
                    tz,
                    period: Period::Day,
                    has_time: true,
                    parts: vec!["time"],
                    locale: None,
                })
            }
            8 => {
                if !settings.allows_parser(ParserKind::BaseFormats)
                    && !settings.allows_parser(ParserKind::NoSpacesTime)
                    && !settings.allows_parser(ParserKind::AbsoluteTime)
                {
                    return None;
                }
                let nd = parse_compact_8digit_date(core, settings)?;
                Some(FastPathMatch {
                    naive: nd.and_hms_opt(0, 0, 0)?,
                    tz,
                    period: Period::Day,
                    has_time: false,
                    parts: vec!["year", "month", "day"],
                    locale: None,
                })
            }
            12 | 14 if settings.parsers.is_some()
                && settings.allows_parser(ParserKind::NoSpacesTime) =>
            {
                let date_part = &core[0..8];
                let time_part = &core[8..];
                let nd = parse_compact_8digit_date(date_part, settings)?;
                let hh: u32 = time_part[0..2].parse().ok()?;
                let mm: u32 = time_part[2..4].parse().ok()?;
                let ss: u32 = if time_part.len() == 6 {
                    time_part[4..6].parse().ok()?
                } else {
                    0
                };
                let time = NaiveTime::from_hms_opt(hh, mm, ss)?;
                Some(FastPathMatch {
                    naive: nd.and_time(time),
                    tz,
                    period: Period::Day,
                    has_time: true,
                    parts: vec!["year", "month", "day", "time"],
                    locale: None,
                })
            }
            9..=10 => {
                if settings.strict_parsing || !settings.allows_parser(ParserKind::Timestamp) {
                    return None;
                }
                let secs: i64 = core.parse().ok()?;
                let dt = DateTime::from_timestamp(secs, 0)?;
                Some(FastPathMatch {
                    naive: dt.naive_utc(),
                    tz: tz.or_else(|| FixedOffset::east_opt(0).map(TzSpec::Fixed)),
                    period: Period::Day,
                    has_time: true,
                    parts: vec!["year", "month", "day", "time"],
                    locale: None,
                })
            }
            13 => {
                if settings.strict_parsing || !settings.allows_parser(ParserKind::Timestamp) {
                    return None;
                }
                let ms: i64 = core.parse().ok()?;
                let dt = DateTime::from_timestamp_millis(ms)?;
                Some(FastPathMatch {
                    naive: dt.naive_utc(),
                    tz: tz.or_else(|| FixedOffset::east_opt(0).map(TzSpec::Fixed)),
                    period: Period::Day,
                    has_time: true,
                    parts: vec!["year", "month", "day", "time"],
                    locale: None,
                })
            }
            16 => {
                if settings.strict_parsing || !settings.allows_parser(ParserKind::Timestamp) {
                    return None;
                }
                let us: i64 = core.parse().ok()?;
                let dt = DateTime::from_timestamp_micros(us)?;
                if !(1..=9999).contains(&dt.year()) {
                    return None;
                }
                Some(FastPathMatch {
                    naive: dt.naive_utc(),
                    tz: tz.or_else(|| FixedOffset::east_opt(0).map(TzSpec::Fixed)),
                    period: Period::Day,
                    has_time: true,
                    parts: vec!["year", "month", "day", "time"],
                    locale: None,
                })
            }
            _ => None,
        }
    } else {
        if settings.strict_parsing || !settings.allows_parser(ParserKind::Timestamp) {
            return None;
        }
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
                has_time: true,
                parts: vec!["year", "month", "day", "time"],
                locale: None,
            });
        }
        None
    }
}

fn parse_compact_8digit_date(s: &str, settings: &Settings) -> Option<NaiveDate> {
    if s.len() != 8 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let order = settings.effective_date_order(DateOrder::YMD);
    let try_ymd = || -> Option<NaiveDate> {
        let y: i32 = s[0..4].parse().ok()?;
        let m: u32 = s[4..6].parse().ok()?;
        let d: u32 = s[6..8].parse().ok()?;
        if (1900..=2199).contains(&y) {
            NaiveDate::from_ymd_opt(y, m, d)
        } else {
            None
        }
    };
    let try_dmy = || -> Option<NaiveDate> {
        let d: u32 = s[0..2].parse().ok()?;
        let m: u32 = s[2..4].parse().ok()?;
        let y: i32 = s[4..8].parse().ok()?;
        if (1900..=2199).contains(&y) {
            NaiveDate::from_ymd_opt(y, m, d)
        } else {
            None
        }
    };
    let try_mdy = || -> Option<NaiveDate> {
        let m: u32 = s[0..2].parse().ok()?;
        let d: u32 = s[2..4].parse().ok()?;
        let y: i32 = s[4..8].parse().ok()?;
        if (1900..=2199).contains(&y) {
            NaiveDate::from_ymd_opt(y, m, d)
        } else {
            None
        }
    };
    match order {
        DateOrder::DMY => try_dmy().or_else(|| {
            if settings.strict_date_order == StrictDateOrder::All {
                None
            } else {
                try_ymd().or_else(try_mdy)
            }
        }),
        DateOrder::MDY => try_mdy().or_else(|| {
            if settings.strict_date_order == StrictDateOrder::All {
                None
            } else {
                try_ymd().or_else(try_dmy)
            }
        }),
        _ => try_ymd().or_else(|| {
            if settings.strict_date_order == StrictDateOrder::All {
                None
            } else if settings.allows_parser(ParserKind::NoSpacesTime) || settings.date_order.is_some()
            {
                try_dmy().or_else(try_mdy)
            } else {
                None
            }
        }),
    }
}

fn try_numeric_datetime(
    core: &str,
    tz: Option<TzSpec>,
    settings: &Settings,
) -> Option<FastPathMatch> {
    if !core.contains(['-', '/']) {
        let lower = core.to_ascii_lowercase();
        let looks_like_time = core.contains(':')
            || core.contains('h')
            || core.contains('H')
            || lower.ends_with("am")
            || lower.ends_with("pm")
            || lower.ends_with("a.m.")
            || lower.ends_with("p.m.")
            || lower.ends_with("a.m")
            || lower.ends_with("p.m");

        if looks_like_time {
            if let Some(time) = parse_time_str(core) {
                if settings.strict_parsing
                    || settings.require_parts.contains(&DatePart::Year)
                    || settings.require_parts.contains(&DatePart::Month)
                    || settings.require_parts.contains(&DatePart::Day)
                {
                    return None;
                }
                let date = resolve_time_only_date(time, settings);
                return Some(FastPathMatch {
                    naive: date.and_time(time),
                    tz,
                    period: Period::Day,
                    has_time: true,
                    parts: vec!["time"],
                    locale: None,
                });
            }
        }
    }

    // Strip spaces around dots in numeric dates like "26. 10.21 14.12" or "30 . 09 . 22 12.12"
    let owned_compact_dots;
    let effective_core: &str = if core.contains(" .") || core.contains(". ") {
        owned_compact_dots = collapse_dot_spaces(core);
        &owned_compact_dots
    } else {
        core
    };

    let (date_part, time_part) = if let Some(idx) = effective_core.find(['T', 't', ' ']) {
        let (d, rest) = effective_core.split_at(idx);
        let t = rest[1..].trim();
        (d, if t.is_empty() { None } else { Some(t) })
    } else {
        (effective_core, None)
    };

    let sep = if date_part.contains('-') {
        '-'
    } else if date_part.contains('/') {
        '/'
    } else if date_part.contains('.') {
        '.'
    } else {
        return None;
    };

    let mut parts = [0u32; 3];
    let mut lens = [0usize; 3];
    let mut count = 0usize;
    let date_clean = date_part.trim_end_matches('.');
    for seg in date_clean.split(sep) {
        let seg_t = seg.trim();
        if seg_t.is_empty()
            || seg_t.len() > 4
            || count >= 3
            || !seg_t.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        parts[count] = seg_t.parse().ok()?;
        lens[count] = seg_t.len();
        count += 1;
    }

    let has_time = time_part.is_some();
    let time = match time_part {
        Some(tp) => {
            let clean_tp = tp
                .strip_prefix("t ")
                .or_else(|| tp.strip_prefix("T "))
                .or_else(|| tp.strip_prefix("at "))
                .or_else(|| tp.strip_prefix("AT "))
                .or_else(|| tp.strip_prefix("@ "))
                .map(|r| r.trim())
                .unwrap_or(tp);
            parse_time_str(clean_tp)?
        }
        None => NaiveTime::from_hms_opt(0, 0, 0)?,
    };

    if count == 2 {
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
        if !(1..=9999).contains(&y) {
            return None;
        }
        let base = settings.effective_base();
        let day = resolve_missing_day(y, m, base.day(), settings.prefer_day_of_month);
        let nd = NaiveDate::from_ymd_opt(y, m, day)?;
        let mut p = vec!["year", "month"];
        if has_time {
            p.push("time");
        }
        return Some(FastPathMatch {
            naive: nd.and_time(time),
            tz,
            period: Period::Month,
            has_time,
            parts: p,
            locale: None,
        });
    }

    if count != 3 {
        return None;
    }

    let fallback_order = if lens[0] == 4 {
        DateOrder::YMD
    } else {
        DateOrder::MDY
    };
    let order = settings.effective_date_order(fallback_order);

    let nd = resolve_ymd_triplet(parts, lens, order, settings.strict_date_order)?;
    let mut p = vec!["year", "month", "day"];
    if has_time {
        p.push("time");
    }
    Some(FastPathMatch {
        naive: nd.and_time(time),
        tz,
        period: Period::Day,
        has_time,
        parts: p,
        locale: None,
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

fn resolve_ymd_triplet(
    parts: [u32; 3],
    lens: [usize; 3],
    order: DateOrder,
    strict: StrictDateOrder,
) -> Option<NaiveDate> {
    if lens[0] == 4 {
        let y = parts[0] as i32;
        if !(1..=9999).contains(&y) {
            return None;
        }
        if strict == StrictDateOrder::All && matches!(order, DateOrder::MYD | DateOrder::DYM) {
            return None;
        }
        if order == DateOrder::YDM {
            if let Some(d) = NaiveDate::from_ymd_opt(y, parts[2], parts[1]) {
                return Some(d);
            }
            if strict == StrictDateOrder::All {
                return None;
            }
            return NaiveDate::from_ymd_opt(y, parts[1], parts[2]);
        }
        if let Some(d) = NaiveDate::from_ymd_opt(y, parts[1], parts[2]) {
            return Some(d);
        }
        if strict == StrictDateOrder::None {
            return NaiveDate::from_ymd_opt(y, parts[2], parts[1]);
        }
        return None;
    }

    if lens[1] == 4 {
        let y = parts[1] as i32;
        if !(1..=9999).contains(&y) {
            return None;
        }
        if strict == StrictDateOrder::All && !matches!(order, DateOrder::MYD | DateOrder::DYM) {
            return None;
        }
        return match order {
            DateOrder::MYD => NaiveDate::from_ymd_opt(y, parts[0], parts[2]),
            _ => NaiveDate::from_ymd_opt(y, parts[2], parts[0]),
        };
    }

    if lens[2] == 4 {
        let y = parts[2] as i32;
        if !(1..=9999).contains(&y) {
            return None;
        }
        if strict == StrictDateOrder::All && !matches!(order, DateOrder::MDY | DateOrder::DMY) {
            return None;
        }
        return match order {
            DateOrder::MDY => {
                if let Some(d) = NaiveDate::from_ymd_opt(y, parts[0], parts[1]) {
                    Some(d)
                } else if strict == StrictDateOrder::All {
                    None
                } else {
                    NaiveDate::from_ymd_opt(y, parts[1], parts[0])
                }
            }
            _ => {
                if let Some(d) = NaiveDate::from_ymd_opt(y, parts[1], parts[0]) {
                    Some(d)
                } else if strict == StrictDateOrder::All {
                    None
                } else {
                    NaiveDate::from_ymd_opt(y, parts[0], parts[1])
                }
            }
        };
    }

    let (y, m, d) = match order {
        DateOrder::YMD => (expand_two_digit_year(parts[0]), parts[1], parts[2]),
        DateOrder::YDM => (expand_two_digit_year(parts[0]), parts[2], parts[1]),
        DateOrder::MDY => (expand_two_digit_year(parts[2]), parts[0], parts[1]),
        DateOrder::DMY => (expand_two_digit_year(parts[2]), parts[1], parts[0]),
        DateOrder::MYD => (expand_two_digit_year(parts[1]), parts[0], parts[2]),
        DateOrder::DYM => (expand_two_digit_year(parts[1]), parts[2], parts[0]),
    };

    if let Some(nd) = NaiveDate::from_ymd_opt(y, m, d) {
        return Some(nd);
    }
    if strict != StrictDateOrder::None {
        return None;
    }
    NaiveDate::from_ymd_opt(y, d, m)
}

fn collapse_dot_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for i in 0..chars.len() {
        let c = chars[i];
        if c == ' ' {
            let prev_dot = i > 0 && chars[i - 1] == '.';
            let next_dot = i + 1 < chars.len() && chars[i + 1] == '.';
            if prev_dot || next_dot {
                continue;
            }
        }
        out.push(c);
    }
    out
}

/// Parse time strings like "15:30", "15:30:45,123", "5:30p.m.", "PM 2:26", "下午 02:26:26", "14.12".
pub fn parse_time_str(input: &str) -> Option<NaiveTime> {
    let mut s = input.trim().trim_end_matches('.');
    if s.is_empty() {
        return None;
    }

    let mut is_pm = false;
    let mut is_am = false;

    // Leading AM/PM/CJK/Korean markers ("PM 2:26", "下午 02:26:26", "午後2時30分", "오후 2시 30분")
    for prefix in ["下午", "午後", "오후", "p.m.", "p.m", "pm", "PM"] {
        if let Some(rest) = s.strip_prefix(prefix) {
            if rest.starts_with(char::is_whitespace) || rest.starts_with(|c: char| c.is_ascii_digit()) {
                s = rest.trim_start();
                is_pm = true;
                break;
            }
        }
    }
    if !is_pm {
        for prefix in ["上午", "午前", "오전", "a.m.", "a.m", "am", "AM"] {
            if let Some(rest) = s.strip_prefix(prefix) {
                if rest.starts_with(char::is_whitespace) || rest.starts_with(|c: char| c.is_ascii_digit()) {
                    s = rest.trim_start();
                    is_am = true;
                    break;
                }
            }
        }
    }

    let lower = s.to_ascii_lowercase();
    if !is_am && !is_pm {
        for suffix in ["p.m.", "p.m", "pm", "ip.", "ip"] {
            if let Some(rest) = lower.strip_suffix(suffix) {
                if !rest.is_empty() {
                    s = s[..rest.len()].trim_end();
                    is_pm = true;
                    break;
                }
            }
        }
        if !is_pm {
            for suffix in ["a.m.", "a.m", "am", "ap.", "ap"] {
                if let Some(rest) = lower.strip_suffix(suffix) {
                    if !rest.is_empty() {
                        s = s[..rest.len()].trim_end();
                        is_am = true;
                        break;
                    }
                }
            }
        }
    }

    // Trailing "Uhr" (German)
    if let Some(rest) = s.strip_suffix("Uhr").or_else(|| s.strip_suffix("uhr")) {
        s = rest.trim_end();
    }

    // CJK / Korean clock times ("2시 30분", "2時30分", "2时30分15秒", "14点30分")
    if s.contains('시') || s.contains('時') || s.contains('时') || s.contains('点') {
        if let Some(t) = parse_cjk_korean_clock(s, is_am, is_pm) {
            return Some(t);
        }
    }

    // French / European "14h30" or "09h15" or "14h"
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

    // Allow dot-separated HH.MM when no ':' is present (e.g. "13.12", "18.37", "5.45 ip.")
    let sep = if s.contains(':') {
        ':'
    } else if s.contains('.') && s.matches('.').count() == 1 {
        '.'
    } else {
        return None;
    };

    let mut colon_parts = s.split(sep);
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

fn parse_cjk_korean_clock(s: &str, is_am: bool, is_pm: bool) -> Option<NaiveTime> {
    let mut rest = s.trim();
    let hour_idx = rest.find(['시', '時', '时', '点'])?;
    let h_str = rest[..hour_idx].trim();
    if h_str.is_empty() || h_str.len() > 2 || !h_str.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut h: u32 = h_str.parse().ok()?;
    let sep_len = rest[hour_idx..].chars().next()?.len_utf8();
    rest = rest[hour_idx + sep_len..].trim();

    let mut m: u32 = 0;
    let mut sec: u32 = 0;
    if !rest.is_empty() {
        if let Some(min_idx) = rest.find(['분', '分']) {
            let m_str = rest[..min_idx].trim();
            if m_str.is_empty() || m_str.len() > 2 || !m_str.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            m = m_str.parse().ok()?;
            let min_len = rest[min_idx..].chars().next()?.len_utf8();
            rest = rest[min_idx + min_len..].trim();
        }
        if !rest.is_empty() {
            if let Some(sec_idx) = rest.find(['초', '秒']) {
                let sec_str = rest[..sec_idx].trim();
                if sec_str.is_empty()
                    || sec_str.len() > 2
                    || !sec_str.bytes().all(|b| b.is_ascii_digit())
                {
                    return None;
                }
                sec = sec_str.parse().ok()?;
                let sec_len = rest[sec_idx..].chars().next()?.len_utf8();
                rest = rest[sec_idx + sec_len..].trim();
            }
        }
        if !rest.is_empty() {
            return None;
        }
    }

    apply_ampm(&mut h, is_am, is_pm)?;
    NaiveTime::from_hms_opt(h, m, sec)
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
