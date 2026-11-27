use chrono::{DateTime, FixedOffset, NaiveDateTime, NaiveTime};
use chrono_tz::Tz;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOrder {
    DMY,
    MDY,
    YMD,
    YDM,
    MYD,
    DYM,
}

#[allow(clippy::should_implement_trait)]
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
pub enum StrictDateOrder {
    #[default]
    None,
    Year,
    All,
}

#[allow(clippy::should_implement_trait)]
impl StrictDateOrder {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "year" => Some(Self::Year),
            "all" => Some(Self::All),
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

#[allow(clippy::should_implement_trait)]
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

#[allow(clippy::should_implement_trait)]
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

#[allow(clippy::should_implement_trait)]
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

#[allow(clippy::should_implement_trait)]
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
    Week,
    Month,
    Year,
    Time,
}

impl Period {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
            Self::Year => "year",
            Self::Time => "time",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartOfDay {
    EarlyMorning,
    Morning,
    Afternoon,
    Evening,
    Night,
}

impl PartOfDay {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::EarlyMorning => "early_morning",
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
            Self::Night => "night",
        }
    }

    pub fn from_hour(hour: u8) -> Option<Self> {
        match hour {
            0..=6 => Some(Self::EarlyMorning),
            7..=11 => Some(Self::Morning),
            12..=16 => Some(Self::Afternoon),
            17..=19 => Some(Self::Evening),
            20..=23 => Some(Self::Night),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartsOfDay {
    pub early_morning: NaiveTime,
    pub morning: NaiveTime,
    pub afternoon: NaiveTime,
    pub evening: NaiveTime,
    pub night: NaiveTime,
}

impl Default for PartsOfDay {
    fn default() -> Self {
        Self {
            early_morning: NaiveTime::from_hms_opt(3, 0, 0).unwrap(),
            morning: NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
            afternoon: NaiveTime::from_hms_opt(15, 0, 0).unwrap(),
            evening: NaiveTime::from_hms_opt(18, 0, 0).unwrap(),
            night: NaiveTime::from_hms_opt(20, 0, 0).unwrap(),
        }
    }
}

impl PartsOfDay {
    #[inline]
    pub fn time_for(&self, p: PartOfDay) -> NaiveTime {
        match p {
            PartOfDay::EarlyMorning => self.early_morning,
            PartOfDay::Morning => self.morning,
            PartOfDay::Afternoon => self.afternoon,
            PartOfDay::Evening => self.evening,
            PartOfDay::Night => self.night,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StartOfWeek {
    #[default]
    Monday,
    Sunday,
}

#[allow(clippy::should_implement_trait)]
impl StartOfWeek {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "monday" => Some(Self::Monday),
            "sunday" => Some(Self::Sunday),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParserKind {
    Timestamp,
    NegativeTimestamp,
    RelativeTime,
    CustomFormats,
    AbsoluteTime,
    BaseFormats,
    NoSpacesTime,
}

#[allow(clippy::should_implement_trait)]
impl ParserKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "timestamp" => Some(Self::Timestamp),
            "negative-timestamp" => Some(Self::NegativeTimestamp),
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
    pub parts: Vec<&'static str>,
    pub part_of_day: Option<PartOfDay>,
}

impl DateData {
    pub fn empty() -> Self {
        Self {
            date_obj: None,
            period: Period::Day,
            locale: None,
            parts: Vec::new(),
            part_of_day: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub date_order: Option<DateOrder>,
    pub strict_date_order: StrictDateOrder,
    pub prefer_locale_date_order: bool,
    pub prefer_dates_from: PreferDatesFrom,
    pub prefer_day_of_month: PreferDayOfMonth,
    pub prefer_month_of_year: PreferMonthOfYear,
    pub strict_parsing: bool,
    pub relative_base: Option<NaiveDateTime>,
    pub timezone: Option<String>,
    pub to_timezone: Option<String>,
    pub return_as_timezone_aware: Option<bool>,
    pub timezone_abbreviations: FxHashMap<String, String>,
    pub languages: Option<Vec<String>>,
    pub default_languages: Vec<String>,
    pub use_given_language_order: bool,
    pub language_detection_confidence_threshold: f64,
    pub date_formats: Option<Vec<String>>,
    pub require_parts: Vec<DatePart>,
    pub normalize: bool,
    pub return_time_as_period: bool,
    pub return_time_span: bool,
    pub default_start_of_week: StartOfWeek,
    pub default_days_in_month: u32,
    pub ignore_surrounding_text: bool,
    pub skip_tokens: Vec<String>,
    pub parsers: Option<Vec<ParserKind>>,
    pub region: Option<String>,
    pub parts_of_day: PartsOfDay,
    pub cache_size_limit: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            date_order: None,
            strict_date_order: StrictDateOrder::None,
            prefer_locale_date_order: true,
            prefer_dates_from: PreferDatesFrom::default(),
            prefer_day_of_month: PreferDayOfMonth::default(),
            prefer_month_of_year: PreferMonthOfYear::default(),
            strict_parsing: false,
            relative_base: None,
            timezone: None,
            to_timezone: None,
            return_as_timezone_aware: None,
            timezone_abbreviations: FxHashMap::default(),
            languages: None,
            default_languages: Vec::new(),
            use_given_language_order: false,
            language_detection_confidence_threshold: 0.5,
            date_formats: None,
            require_parts: Vec::new(),
            normalize: true,
            return_time_as_period: false,
            return_time_span: false,
            default_start_of_week: StartOfWeek::Monday,
            default_days_in_month: 30,
            ignore_surrounding_text: false,
            skip_tokens: Vec::new(),
            parsers: None,
            region: None,
            parts_of_day: PartsOfDay::default(),
            cache_size_limit: 1000,
        }
    }
}

/// Extract an ISO 3166-1 alpha-2 region subtag from a BCP-47 locale tag (e.g. `"en-GB"` -> `"GB"`,
/// `"en_CA"` -> `"CA"`, `"zh-Hans-TW"` -> `"TW"`), ignoring 4-letter script subtags like `"uz-Cyrl"`.
pub fn extract_region_from_locale(code: &str) -> Option<&str> {
    let mut parts = code.trim().split(['-', '_']);
    let _lang = parts.next()?;
    parts.find(|&sub| sub.len() == 2 && sub.bytes().all(|b| b.is_ascii_alphabetic()))
}

/// Map a 2-letter region/country code to its conventional numeric date order.
pub fn region_to_date_order(reg: &str) -> Option<DateOrder> {
    let code = reg.trim().to_ascii_uppercase();
    match code.as_str() {
        "US" | "PH" | "FM" | "AS" | "UM" => Some(DateOrder::MDY),
        "CN" | "JP" | "KR" | "TW" | "HU" | "LT" | "SE" | "ZA" | "CA" => Some(DateOrder::YMD),
        r if r.len() == 2 && r.bytes().all(|b| b.is_ascii_alphabetic()) => Some(DateOrder::DMY),
        _ => None,
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
            None => !matches!(kind, ParserKind::NegativeTimestamp),
            Some(list) => list.contains(&kind),
        }
    }

    pub fn effective_date_order(&self, fallback: DateOrder) -> DateOrder {
        if let Some(order) = self.date_order {
            return order;
        }
        if let Some(ref reg) = self.region {
            return region_to_date_order(reg).unwrap_or(fallback);
        }
        if self.prefer_locale_date_order {
            if let Some(first) = self.languages.as_deref().and_then(|ls| ls.first()) {
                if let Some(reg) = extract_region_from_locale(first) {
                    if let Some(order) = region_to_date_order(reg) {
                        return order;
                    }
                }
                let reg = crate::locale::LocaleRegistry::global();
                if let Some(id) = reg.resolve_lang_id(first) {
                    return reg.locales[id].date_order;
                }
            }
        }
        fallback
    }
}
