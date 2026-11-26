use crate::relative::apply_relative_offset;
use crate::locale::TimeUnit;
use crate::settings::{Settings, StartOfWeek};
use chrono::{Datelike, Duration, NaiveDateTime};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct TimeSpanMatch {
    pub span_type: &'static str,
    pub direction: &'static str,
    pub number: Option<u32>,
    pub matched_text: String,
    pub start_pos: usize,
    pub end_pos: usize,
}

struct CompiledSpanPattern {
    regex: Regex,
    span_type: &'static str,
    direction: &'static str,
    has_num: bool,
}

static SPAN_PATTERNS: OnceLock<Vec<CompiledSpanPattern>> = OnceLock::new();

fn patterns() -> &'static [CompiledSpanPattern] {
    SPAN_PATTERNS.get_or_init(|| {
        let specs = [
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+month\b", "month", "past", false),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+week\b", "week", "past", false),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+(\d+)\s+days?\b", "days", "past", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+(\d+)\s+weeks?\b", "weeks", "past", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+(\d+)\s+months?\b", "months", "past", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+month\b", "month", "future", false),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+week\b", "week", "future", false),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+(\d+)\s+days?\b", "days", "future", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+(\d+)\s+weeks?\b", "weeks", "future", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+(\d+)\s+months?\b", "months", "future", true),
        ];
        specs
            .into_iter()
            .map(|(pat, span_type, direction, has_num)| CompiledSpanPattern {
                regex: Regex::new(pat).expect("valid static span regex"),
                span_type,
                direction,
                has_num,
            })
            .collect()
    })
}

/// Detect time span expressions such as `"last week"`, `"past 3 days"`, `"next 2 months"`.
pub fn detect_time_span(text: &str) -> Option<TimeSpanMatch> {
    for pat in patterns() {
        if let Some(caps) = pat.regex.captures(text) {
            let full = caps.get(0)?;
            let number = if pat.has_num {
                let num_str = caps.get(1)?.as_str();
                match num_str.parse::<u32>() {
                    Ok(n) => Some(n),
                    Err(_) => continue,
                }
            } else {
                None
            };
            return Some(TimeSpanMatch {
                span_type: pat.span_type,
                direction: pat.direction,
                number,
                matched_text: full.as_str().to_string(),
                start_pos: full.start(),
                end_pos: full.end(),
            });
        }
    }
    None
}

fn week_start(date: NaiveDateTime, start_of_week: StartOfWeek) -> NaiveDateTime {
    let mut days_back = date.weekday().num_days_from_monday() as i64;
    if start_of_week != StartOfWeek::Monday {
        days_back = (days_back + 1) % 7;
    }
    date - Duration::days(days_back)
}

/// Return `(start, end)` datetimes for a detected time span relative to `base_date`.
pub fn generate_time_span(
    span_type: &str,
    direction: &str,
    number: u32,
    base_date: NaiveDateTime,
    settings: &Settings,
) -> Option<(NaiveDateTime, NaiveDateTime)> {
    let past = direction == "past";
    if span_type == "week" {
        let ws = week_start(base_date, settings.default_start_of_week);
        return if past {
            Some((ws - Duration::days(7), ws - Duration::days(1)))
        } else {
            let start = ws + Duration::days(7);
            Some((start, start + Duration::days(6)))
        };
    }

    let sign: i8 = if past { -1 } else { 1 };
    let other = match span_type {
        "month" => apply_relative_offset(
            base_date,
            sign,
            settings.default_days_in_month as f64,
            TimeUnit::Day,
        )?,
        "days" => apply_relative_offset(base_date, sign, number as f64, TimeUnit::Day)?,
        "weeks" => apply_relative_offset(base_date, sign, number as f64, TimeUnit::Week)?,
        _ => apply_relative_offset(base_date, sign, number as f64, TimeUnit::Month)?,
    };

    if past {
        Some((other, base_date))
    } else {
        Some((base_date, other))
    }
}
