use chrono::{DateTime, FixedOffset, NaiveDateTime};
use chrono_tz::Tz;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    DMY,
    MDY,
    YMD,
    YDM,
    MYD,
    DYM,
}

impl DateOrder {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "DMY" => Some(Self::DMY),
            "MDY" => Some(Self::MDY),
            "YMD" => Some(Self::YMD),
            "YDM" => Some(Self::YDM),
            "MYD" => Some(Self::MYD),
            "DYM" => Some(Self::DYM),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferDatesFrom {
    #[default]
    CurrentPeriod,
    Past,
    Future,
}

impl PreferDatesFrom {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "current_period" => Some(Self::CurrentPeriod),
            "past" => Some(Self::Past),
            "future" => Some(Self::Future),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferDayOfMonth {
    #[default]
    Current,
    First,
    Last,
}

impl PreferDayOfMonth {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "current" => Some(Self::Current),
            "first" => Some(Self::First),
            "last" => Some(Self::Last),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PreferMonthOfYear {
    #[default]
    Current,
    First,
    Last,
}

impl PreferMonthOfYear {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "current" => Some(Self::Current),
            "first" => Some(Self::First),
            "last" => Some(Self::Last),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatePart {
    Year,
    Month,
    Day,
}

impl DatePart {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "year" => Some(Self::Year),
            "month" => Some(Self::Month),
            "day" => Some(Self::Day),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Period {
    #[default]
    Day,
    Month,
    Year,
    Time,
}

impl Period {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
            Self::Time => "time",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserKind {
    Timestamp,
    RelativeTime,
    CustomFormats,
    AbsoluteTime,
    BaseFormats,
    NoSpacesTime,
}

impl ParserKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "timestamp" => Some(Self::Timestamp),
            "relative-time" => Some(Self::RelativeTime),
            "custom-formats" => Some(Self::CustomFormats),
            "absolute-time" => Some(Self::AbsoluteTime),
            "base-formats" => Some(Self::BaseFormats),
            "no-spaces-time" => Some(Self::NoSpacesTime),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParsedDateTime {
    Naive(NaiveDateTime),
    FixedOffset(DateTime<FixedOffset>),
    Tz(DateTime<Tz>),
}

impl ParsedDateTime {
    pub fn naive_local(&self) -> NaiveDateTime {
        match self {
            Self::Naive(ndt) => *ndt,
            Self::FixedOffset(dt) => dt.naive_local(),
            Self::Tz(dt) => dt.naive_local(),
        }
    }

    pub fn is_aware(&self) -> bool {
        !matches!(self, Self::Naive(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DateData {
    pub date_obj: Option<ParsedDateTime>,
    pub period: Period,
    pub locale: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub date_order: Option<DateOrder>,
    pub prefer_dates_from: PreferDatesFrom,
    pub prefer_day_of_month: PreferDayOfMonth,
    pub prefer_month_of_year: PreferMonthOfYear,
    pub strict_parsing: bool,
    pub relative_base: Option<NaiveDateTime>,
    pub timezone: Option<String>,
    pub to_timezone: Option<String>,
    pub return_as_timezone_aware: Option<bool>,
    pub languages: Option<Vec<String>>,
    pub default_languages: Vec<String>,
    pub date_formats: Option<Vec<String>>,
    pub require_parts: Vec<DatePart>,
    pub normalize: bool,
    pub return_time_as_period: bool,
    pub skip_tokens: Vec<String>,
    pub parsers: Option<Vec<ParserKind>>,
    pub region: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            date_order: None,
            prefer_dates_from: PreferDatesFrom::default(),
            prefer_day_of_month: PreferDayOfMonth::default(),
            prefer_month_of_year: PreferMonthOfYear::default(),
            strict_parsing: false,
            relative_base: None,
            timezone: None,
            to_timezone: None,
            return_as_timezone_aware: None,
            languages: None,
            default_languages: Vec::new(),
            date_formats: None,
            require_parts: Vec::new(),
            normalize: true,
            return_time_as_period: false,
            skip_tokens: Vec::new(),
            parsers: None,
            region: None,
        }
    }
}

impl Settings {
    pub fn effective_base(&self) -> NaiveDateTime {
        self.relative_base
            .unwrap_or_else(|| chrono::Local::now().naive_local())
    }

    #[inline]
    pub fn allows_parser(&self, kind: ParserKind) -> bool {
        match &self.parsers {
            None => true,
            Some(list) => list.contains(&kind),
        }
    }

    pub fn effective_date_order(&self, fallback: DateOrder) -> DateOrder {
        if let Some(order) = self.date_order {
            return order;
        }
        if let Some(ref reg) = self.region {
            let code = reg.trim().to_ascii_uppercase();
            return match code.as_str() {
                "US" | "PH" | "FM" | "AS" | "UM" => DateOrder::MDY,
                "CN" | "JP" | "KR" | "TW" | "HU" | "LT" | "SE" | "ZA" | "CA" => DateOrder::YMD,
                r if r.len() == 2 && r.bytes().all(|b| b.is_ascii_alphabetic()) => DateOrder::DMY,
                _ => fallback,
            };
        }
        fallback
    }
}
