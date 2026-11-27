use crate::settings::{ParsedDateTime, Settings};
use chrono::{FixedOffset, NaiveDateTime, TimeZone};
use chrono_tz::Tz;
use rustc_hash::FxHashMap;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TzSpec {
    Fixed(FixedOffset),
    Iana(Tz),
}

/// Resolve a timezone name, abbreviation, or numeric offset checking custom overrides first.
pub fn parse_tz_spec_with_abbrevs(
    s: &str,
    custom: &FxHashMap<String, String>,
) -> Option<TzSpec> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    if !custom.is_empty() {
        let upper = trimmed.to_ascii_uppercase();
        if let Some(mapped) = custom.get(&upper).or_else(|| custom.get(trimmed)) {
            return parse_tz_spec(mapped);
        }
    }
    parse_tz_spec(trimmed)
}

/// Resolve a timezone name, abbreviation, or numeric offset (e.g. "UTC", "Europe/Istanbul", "+03:00", "EST").
pub fn parse_tz_spec(s: &str) -> Option<TzSpec> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Fast path for Z / UTC / GMT
    if trimmed.eq_ignore_ascii_case("z")
        || trimmed.eq_ignore_ascii_case("utc")
        || trimmed.eq_ignore_ascii_case("gmt")
    {
        return Some(TzSpec::Fixed(FixedOffset::east_opt(0)?));
    }

    // Numeric offsets: +03:00, -0500, +3, UTC+3, GMT-04:00
    let offset_candidate = trimmed
        .strip_prefix("UTC")
        .or_else(|| trimmed.strip_prefix("utc"))
        .or_else(|| trimmed.strip_prefix("GMT"))
        .or_else(|| trimmed.strip_prefix("gmt"))
        .unwrap_or(trimmed);

    if let Some(off) = parse_numeric_offset(offset_candidate) {
        return Some(TzSpec::Fixed(off));
    }

    // IANA timezone name (e.g., "Europe/Istanbul", "America/New_York")
    if let Ok(tz) = Tz::from_str(trimmed) {
        return Some(TzSpec::Iana(tz));
    }

    // Common & world timezone abbreviations
    let secs = match trimmed.to_ascii_uppercase().as_str() {
        // UTC / Universal
        "UT" | "WT" | "AZOST" | "EGST" => 0,
        // Americas
        "SST" | "NUT" => -11 * 3600,
        "HST" | "HAST" | "CKT" | "TAHT" => -10 * 3600,
        "MART" | "MIT" => -9 * 3600 - 1800,
        "AKST" | "HADT" | "HDT" => -9 * 3600,
        "PST" | "PT" | "AKDT" => -8 * 3600,
        "MST" | "PDT" => -7 * 3600,
        "CST" | "MDT" => -6 * 3600,
        "EST" | "CDT" | "ACT" | "COT" | "ECT" | "ET" | "PET" => -5 * 3600,
        "AST" | "EDT" | "BOT" | "CLT" | "FKT" | "GYT" | "PYT" | "VET" => -4 * 3600,
        "NST" | "NT" => -3 * 3600 - 1800,
        "ADT" | "ARST" | "ART" | "BRT" | "CLST" | "FKST" | "GFT" | "PYST" | "SRT" | "UYT" => -3 * 3600,
        "NDT" => -2 * 3600 - 1800,
        "BRST" | "FNT" | "UYST" => -2 * 3600,
        "CVT" | "AZOT" | "EGT" => -3600,
        // Europe / Africa / Middle East
        "WET" => 0,
        "CET" | "WEST" | "BST" | "WAT" | "MET" | "MEZ" | "DFT" => 3600,
        "CEST" | "EET" | "CAT" | "SAST" | "MEST" | "MESZ" | "WAST" => 2 * 3600,
        "EEST" | "TRT" | "MSK" | "EAT" | "FET" | "IDT" => 3 * 3600,
        "IRST" => 3 * 3600 + 1800,
        "GST" | "AMT" | "AZT" | "GET" | "MSD" | "MUT" | "RET" | "SCT" | "SAMT" => 4 * 3600,
        "IRDT" | "AFT" => 4 * 3600 + 1800,
        // Asia / Pacific
        "PKT" | "UZT" | "TJT" | "TMT" | "MVT" | "YEKT" | "ORAT" | "AQTT" => 5 * 3600,
        "IST" | "SLT" => 5 * 3600 + 1800,
        "NPT" => 5 * 3600 + 2700,
        "ALMT" | "BTT" | "IOT" | "KGT" | "OMST" | "QYZT" | "VOST" | "PKST" | "XJT" | "BDT" => 6 * 3600,
        "MMT" | "CCT" => 6 * 3600 + 1800,
        "ICT" | "WIB" | "THA" | "KRAT" | "NOVT" | "CXT" | "HOVT" => 7 * 3600,
        "SGT" | "HKT" | "AWST" | "PHT" | "MYT" | "WITA" | "BNT" | "ULAT" | "IRKT" | "CHOT" | "WST" => 8 * 3600,
        "ACWST" => 8 * 3600 + 2700,
        "JST" | "KST" | "WIT" | "TLT" | "YAKT" => 9 * 3600,
        "ACST" => 9 * 3600 + 1800,
        "AEST" | "CHST" | "PGT" | "VLAT" => 10 * 3600,
        "ACDT" | "LHST" => 10 * 3600 + 1800,
        "AEDT" | "SBT" | "VUT" | "NCT" | "MAGT" | "KOST" | "PONT" | "LHDT" => 11 * 3600,
        "NFT" => 11 * 3600 + 1800,
        "NZST" | "FJT" | "GILT" | "MHT" | "TVT" | "ANAT" | "PETT" | "WFT" => 12 * 3600,
        "CHAST" => 12 * 3600 + 2700,
        "NZDT" | "FJST" | "TOT" | "PHOT" | "WSST" => 13 * 3600,
        "CHADT" => 13 * 3600 + 2700,
        "LINT" | "WSDT" => 14 * 3600,
        _ => return None,
    };

    FixedOffset::east_opt(secs).map(TzSpec::Fixed)
}

pub fn parse_numeric_offset(s: &str) -> Option<FixedOffset> {
    let b = s.as_bytes();
    if b.len() < 2 {
        return None;
    }
    let sign = match b[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let rest = &b[1..];
    let (h, m): (i32, i32) = match rest.len() {
        1 if rest[0].is_ascii_digit() => ((rest[0] - b'0') as i32, 0),
        2 if rest[0].is_ascii_digit() && rest[1].is_ascii_digit() => {
            (((rest[0] - b'0') * 10 + (rest[1] - b'0')) as i32, 0)
        }
        4 if rest.iter().all(|c| c.is_ascii_digit()) => {
            let hh = ((rest[0] - b'0') * 10 + (rest[1] - b'0')) as i32;
            let mm = ((rest[2] - b'0') * 10 + (rest[3] - b'0')) as i32;
            (hh, mm)
        }
        5 if rest[2] == b':'
            && rest[0].is_ascii_digit()
            && rest[1].is_ascii_digit()
            && rest[3].is_ascii_digit()
            && rest[4].is_ascii_digit() =>
        {
            let hh = ((rest[0] - b'0') * 10 + (rest[1] - b'0')) as i32;
            let mm = ((rest[3] - b'0') * 10 + (rest[4] - b'0')) as i32;
            (hh, mm)
        }
        _ => return None,
    };

    if h > 23 || m > 59 {
        return None;
    }
    FixedOffset::east_opt(sign * (h * 3600 + m * 60))
}

pub fn extract_trailing_timezone(input: &str) -> (&str, Option<TzSpec>) {
    static EMPTY: std::sync::OnceLock<FxHashMap<String, String>> = std::sync::OnceLock::new();
    let empty = EMPTY.get_or_init(FxHashMap::default);
    extract_trailing_timezone_with_abbrevs(input, empty)
}

fn extract_trailing_timezone_non_paren<'a>(
    s: &'a str,
    custom: &FxHashMap<String, String>,
) -> (&'a str, Option<TzSpec>) {
    // Handle trailing 'Z' attached to a timestamp like "2026-11-26T15:30:00Z"
    if (s.ends_with('Z') || s.ends_with('z')) && s.len() >= 10 {
        let prev = s.as_bytes()[s.len() - 2];
        if prev.is_ascii_digit() {
            return (&s[..s.len() - 1], FixedOffset::east_opt(0).map(TzSpec::Fixed));
        }
    }

    // Handle trailing "+03:00" or "-0500" attached directly to time digits (e.g. 15:30:00+03:00)
    if let Some(pos) = s.rfind(['+', '-']) {
        if pos >= 8 {
            let cand = &s[pos..];
            if cand.len() >= 3 && cand.len() <= 6 {
                let before = s.as_bytes()[pos - 1];
                if (before.is_ascii_digit() && s[..pos].contains(':')) || before.is_ascii_whitespace()
                {
                    if let Some(off) = parse_numeric_offset(cand) {
                        let prefix = s[..pos].trim_end();
                        if !prefix.is_empty() {
                            return (prefix, Some(TzSpec::Fixed(off)));
                        }
                    }
                }
            }
        }
    }

    // Handle whitespace-separated trailing token: "EST", "UTC+3", "Europe/Istanbul", or custom abbrev
    if let Some((sp, sp_ch)) = s.char_indices().rev().find(|&(_, c)| c.is_whitespace()) {
        let last = &s[sp + sp_ch.len_utf8()..];
        let upper = last.to_ascii_uppercase();
        let is_custom = !custom.is_empty() && (custom.contains_key(&upper) || custom.contains_key(last));
        let is_two_letter_tz = last.len() == 2
            && matches!(last, "UT" | "ET" | "PT")
            && s[..sp]
                .split_whitespace()
                .next_back()
                .map(|prev| prev.contains(':'))
                .unwrap_or(false);
        let looks_like_tz = is_custom
            || is_two_letter_tz
            || last.contains('/')
            || last.contains('+')
            || last.contains('-')
            || last.eq_ignore_ascii_case("utc")
            || last.eq_ignore_ascii_case("gmt")
            || (last.len() >= 3 && last.len() <= 5 && last.chars().all(|c| c.is_ascii_uppercase()));
        if looks_like_tz {
            if let Some(tz) = parse_tz_spec_with_abbrevs(last, custom) {
                let prefix = s[..sp].trim_end();
                if !prefix.is_empty() {
                    return (prefix, Some(tz));
                }
            }
        }
    }

    (s, None)
}

/// Peel off a trailing timezone token from a date string, honoring custom abbreviations
/// and JavaScript / RFC-2822 parenthesized timezone comments such as
/// `"Thu Nov 26 2026 14:30:00 GMT+0300 (Turkey Time)"`.
pub fn extract_trailing_timezone_with_abbrevs<'a>(
    input: &'a str,
    custom: &FxHashMap<String, String>,
) -> (&'a str, Option<TzSpec>) {
    let s = input.trim();
    if s.is_empty() {
        return (s, None);
    }

    if s.ends_with(')') {
        if let Some(open) = s.rfind('(') {
            if open > 0 {
                let inside = s[open + 1..s.len() - 1].trim();
                let prefix = s[..open].trim_end();
                if !prefix.is_empty() {
                    let (prefix2, prefix_tz) = extract_trailing_timezone_non_paren(prefix, custom);
                    if prefix_tz.is_some() && !prefix2.is_empty() {
                        return (prefix2, prefix_tz);
                    }
                    if let Some(paren_tz) = parse_tz_spec_with_abbrevs(inside, custom) {
                        return (prefix, Some(paren_tz));
                    }
                }
            }
        }
    }

    extract_trailing_timezone_non_paren(s, custom)
}

/// Extract a Unix `ctime` / `date` mid-string timezone token immediately preceding a trailing
/// 4-digit year (e.g. `"Thu Nov 26 14:30:00 UTC 2026"`, `"Mon Jan 15 09:00:00 CEST 2026"`).
pub fn extract_embedded_timezone_before_year(
    input: &str,
    custom: &FxHashMap<String, String>,
) -> Option<(String, TzSpec)> {
    let tokens: Vec<&str> = input.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }
    let len = tokens.len();
    let year_tok = tokens[len - 1];
    if year_tok.len() != 4 || !year_tok.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: u32 = year_tok.parse().ok()?;
    if !(1000..=2999).contains(&year) {
        return None;
    }

    let tz_tok = tokens[len - 2];
    let time_tok = tokens[len - 3];
    if !time_tok.contains(':') {
        return None;
    }

    let upper = tz_tok.to_ascii_uppercase();
    let is_custom = !custom.is_empty() && (custom.contains_key(&upper) || custom.contains_key(tz_tok));
    let looks_like_tz = is_custom
        || tz_tok.contains('/')
        || tz_tok.contains('+')
        || tz_tok.contains('-')
        || tz_tok.eq_ignore_ascii_case("utc")
        || tz_tok.eq_ignore_ascii_case("gmt")
        || matches!(tz_tok, "UT" | "ET" | "PT")
        || (tz_tok.len() >= 3
            && tz_tok.len() <= 5
            && tz_tok.chars().all(|c| c.is_ascii_uppercase()));
    if !looks_like_tz {
        return None;
    }

    let tz = parse_tz_spec_with_abbrevs(tz_tok, custom)?;
    let mut cleaned_tokens = Vec::with_capacity(len - 1);
    cleaned_tokens.extend_from_slice(&tokens[..len - 2]);
    cleaned_tokens.push(year_tok);
    Some((cleaned_tokens.join(" "), tz))
}

/// Equivalent of `dateparser.timezone_parser.pop_tz_offset_from_string`.
pub fn pop_tz_offset_from_string(
    input: &str,
    custom: Option<&FxHashMap<String, String>>,
) -> (String, Option<TzSpec>) {
    let (cleaned, tz) = match custom {
        Some(map) => extract_trailing_timezone_with_abbrevs(input, map),
        None => extract_trailing_timezone(input),
    };
    (cleaned.to_string(), tz)
}

/// Resolve a local `NaiveDateTime` inside an IANA timezone, shifting past DST spring-forward gaps.
#[inline]
fn resolve_iana_local(tz: Tz, naive: NaiveDateTime) -> chrono::DateTime<Tz> {
    if let Some(dt) = tz.from_local_datetime(&naive).earliest() {
        return dt;
    }
    let shifted = naive + chrono::Duration::hours(1);
    if let Some(dt) = tz.from_local_datetime(&shifted).earliest() {
        return dt;
    }
    naive.and_utc().with_timezone(&tz)
}

/// Apply parsed timezone + Settings (`TIMEZONE`, `TO_TIMEZONE`, `RETURN_AS_TIMEZONE_AWARE`)
/// to produce the final `ParsedDateTime`.
pub fn finalize_datetime(
    naive: NaiveDateTime,
    extracted_tz: Option<TzSpec>,
    settings: &Settings,
) -> ParsedDateTime {
    let source_tz = extracted_tz.or_else(|| {
        settings
            .timezone
            .as_deref()
            .and_then(|t| parse_tz_spec_with_abbrevs(t, &settings.timezone_abbreviations))
    });

    let target_tz = settings
        .to_timezone
        .as_deref()
        .and_then(|t| parse_tz_spec_with_abbrevs(t, &settings.timezone_abbreviations));

    let want_aware = match settings.return_as_timezone_aware {
        Some(flag) => flag,
        None => extracted_tz.is_some(),
    };

    if let Some(tgt) = target_tz {
        let utc_dt = match source_tz {
            Some(TzSpec::Fixed(off)) => off
                .from_local_datetime(&naive)
                .single()
                .map(|d| d.to_utc())
                .unwrap_or_else(|| naive.and_utc()),
            Some(TzSpec::Iana(tz)) => resolve_iana_local(tz, naive).to_utc(),
            None => chrono::Local
                .from_local_datetime(&naive)
                .earliest()
                .map(|d| d.to_utc())
                .unwrap_or_else(|| naive.and_utc()),
        };

        return match tgt {
            TzSpec::Fixed(off) => {
                let converted = utc_dt.with_timezone(&off);
                if want_aware {
                    ParsedDateTime::FixedOffset(converted)
                } else {
                    ParsedDateTime::Naive(converted.naive_local())
                }
            }
            TzSpec::Iana(tz) => {
                let converted = utc_dt.with_timezone(&tz);
                if want_aware {
                    ParsedDateTime::Tz(converted)
                } else {
                    ParsedDateTime::Naive(converted.naive_local())
                }
            }
        };
    }

    if want_aware {
        match source_tz {
            Some(TzSpec::Fixed(off)) => {
                if let Some(dt) = off.from_local_datetime(&naive).single() {
                    return ParsedDateTime::FixedOffset(dt);
                }
            }
            Some(TzSpec::Iana(tz)) => {
                return ParsedDateTime::Tz(resolve_iana_local(tz, naive));
            }
            None => {
                if let Some(dt) = chrono::Local.from_local_datetime(&naive).earliest() {
                    return ParsedDateTime::FixedOffset(dt.fixed_offset());
                }
            }
        }
    }

    ParsedDateTime::Naive(naive)
}

#[inline]
pub fn is_valid_year_range(dt: &ParsedDateTime) -> bool {
    use chrono::Datelike;
    let y = match dt {
        ParsedDateTime::Naive(n) => n.year(),
        ParsedDateTime::FixedOffset(f) => f.year(),
        ParsedDateTime::Tz(t) => t.year(),
    };
    (1..=9999).contains(&y)
}
