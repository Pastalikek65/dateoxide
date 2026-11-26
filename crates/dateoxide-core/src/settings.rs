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
}

impl Period {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
            Self::Year => "year",
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

#[derive(Debug, Clone, Default)]
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
}

impl Settings {
    pub fn effective_base(&self) -> NaiveDateTime {
        self.relative_base
            .unwrap_or_else(|| chrono::Local::now().naive_local())
    }
}
