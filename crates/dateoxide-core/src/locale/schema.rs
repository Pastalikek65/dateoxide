use serde::{Deserialize, Serialize};

/// Compact serializable locale schema shared between build.rs and runtime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawLocale {
    pub code: String,
    pub date_order: String,
    pub months: Vec<Vec<String>>,
    pub weekdays: Vec<Vec<String>>,
    pub am: Vec<String>,
    pub pm: Vec<String>,
    pub units: Vec<(String, u8)>,
    pub ago: Vec<String>,
    pub in_words: Vec<String>,
    pub skip: Vec<String>,
    /// (phrase, sign: -1 past / +1 future, amount, unit_id)
    pub relative_exact: Vec<(String, i8, i32, u8)>,
    /// (prefix, suffix, sign: -1 past / +1 future, unit_id)
    pub relative_affix: Vec<(String, String, i8, u8)>,
    pub word_numbers: Vec<(String, u32)>,
}
