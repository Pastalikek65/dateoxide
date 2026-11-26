pub mod schema;

use crate::settings::DateOrder;
use rustc_hash::{FxHashMap, FxHashSet};
use schema::RawLocale;
use std::sync::OnceLock;

static EMBEDDED_LOCALES: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/locales.bin"));
static REGISTRY: OnceLock<LocaleRegistry> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TimeUnit {
    Year = 0,
    Month = 1,
    Week = 2,
    Day = 3,
    Hour = 4,
    Minute = 5,
    Second = 6,
    Decade = 7,
    Century = 8,
    Fortnight = 9,
}

impl TimeUnit {
    #[inline]
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Year),
            1 => Some(Self::Month),
            2 => Some(Self::Week),
            3 => Some(Self::Day),
            4 => Some(Self::Hour),
            5 => Some(Self::Minute),
            6 => Some(Self::Second),
            7 => Some(Self::Decade),
            8 => Some(Self::Century),
            9 => Some(Self::Fortnight),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelativeSpec {
    /// -1 for past ("ago"), +1 for future ("in")
    pub sign: i8,
    pub amount: i32,
    pub unit: TimeUnit,
}

#[derive(Debug, Clone)]
pub struct RelativeAffix {
    pub prefix: String,
    pub suffix: String,
    pub sign: i8,
    pub unit: TimeUnit,
}

#[derive(Debug)]
pub struct CompiledLocale {
    pub id: u16,
    pub code: String,
    pub date_order: DateOrder,
    pub months: FxHashMap<String, u8>,
    pub weekdays: FxHashMap<String, u8>,
    pub am: FxHashSet<String>,
    pub pm: FxHashSet<String>,
    pub units: FxHashMap<String, TimeUnit>,
    pub ago: FxHashSet<String>,
    pub in_words: FxHashSet<String>,
    pub skip: FxHashSet<String>,
    pub relative_exact: FxHashMap<String, RelativeSpec>,
    pub relative_affix: Vec<RelativeAffix>,
    pub word_numbers: FxHashMap<String, u32>,
}

pub struct LocaleRegistry {
    pub locales: Vec<CompiledLocale>,
    pub code_to_id: FxHashMap<String, usize>,
    /// Global inverted index: lowercase token -> priority-ordered list of locale IDs.
    pub token_to_langs: FxHashMap<String, Vec<u16>>,
    /// Global exact relative phrase index: phrase -> list of (lang_id, RelativeSpec).
    pub exact_phrase_index: FxHashMap<String, Vec<(u16, RelativeSpec)>>,
    /// Tokens that can start or belong to a date expression (used for fast search_dates).
    pub known_date_tokens: FxHashSet<String>,
}

impl LocaleRegistry {
    #[inline]
    pub fn global() -> &'static Self {
        REGISTRY.get_or_init(Self::load)
    }

    fn load() -> Self {
        let raw_locales: Vec<RawLocale> = postcard::from_bytes(EMBEDDED_LOCALES)
            .expect("embedded locales.bin must deserialize cleanly");

        let mut locales = Vec::with_capacity(raw_locales.len());
        let mut code_to_id = FxHashMap::default();
        let mut token_to_langs: FxHashMap<String, Vec<u16>> = FxHashMap::default();
        let mut exact_phrase_index: FxHashMap<String, Vec<(u16, RelativeSpec)>> =
            FxHashMap::default();
        let mut known_date_tokens: FxHashSet<String> = FxHashSet::default();

        let mut register_token = |tok: &str, lang_id: u16, is_search_token: bool| {
            if tok.is_empty() {
                return;
            }
            let entry = token_to_langs.entry(tok.to_string()).or_default();
            if entry.last().copied() != Some(lang_id) && !entry.contains(&lang_id) {
                entry.push(lang_id);
            }
            if is_search_token {
                known_date_tokens.insert(tok.to_string());
            }
        };

        for (idx, raw) in raw_locales.into_iter().enumerate() {
            let lang_id = idx as u16;
            code_to_id.insert(raw.code.clone(), idx);
            let date_order = DateOrder::from_str(&raw.date_order).unwrap_or(DateOrder::DMY);

            let mut months = FxHashMap::default();
            for (m_idx, names) in raw.months.into_iter().enumerate() {
                let month_num = (m_idx + 1) as u8;
                for name in names {
                    for part in name.split_whitespace() {
                        if part.chars().count() >= 2 {
                            register_token(part, lang_id, true);
                        }
                    }
                    register_token(&name, lang_id, true);
                    months.insert(name, month_num);
                }
            }

            let mut weekdays = FxHashMap::default();
            for (d_idx, names) in raw.weekdays.into_iter().enumerate() {
                let wd_num = d_idx as u8;
                for name in names {
                    register_token(&name, lang_id, true);
                    weekdays.insert(name, wd_num);
                }
            }

            let mut am = FxHashSet::default();
            for s in raw.am {
                register_token(&s, lang_id, true);
                am.insert(s);
            }
            let mut pm = FxHashSet::default();
            for s in raw.pm {
                register_token(&s, lang_id, true);
                pm.insert(s);
            }

            let mut units = FxHashMap::default();
            for (tok, u_id) in raw.units {
                if let Some(u) = TimeUnit::from_u8(u_id) {
                    register_token(&tok, lang_id, true);
                    units.insert(tok, u);
                }
            }

            let mut ago = FxHashSet::default();
            for s in raw.ago {
                for part in s.split_whitespace() {
                    register_token(part, lang_id, true);
                }
                register_token(&s, lang_id, true);
                ago.insert(s);
            }

            let mut in_words = FxHashSet::default();
            for s in raw.in_words {
                for part in s.split_whitespace() {
                    register_token(part, lang_id, true);
                }
                register_token(&s, lang_id, true);
                in_words.insert(s);
            }

            let mut skip = FxHashSet::default();
            for s in raw.skip {
                skip.insert(s);
            }

            let mut relative_exact = FxHashMap::default();
            for (phrase, sign, amount, u_id) in raw.relative_exact {
                if let Some(unit) = TimeUnit::from_u8(u_id) {
                    let spec = RelativeSpec { sign, amount, unit };
                    for part in phrase.split_whitespace() {
                        register_token(part, lang_id, true);
                    }
                    register_token(&phrase, lang_id, true);
                    exact_phrase_index
                        .entry(phrase.clone())
                        .or_default()
                        .push((lang_id, spec));
                    relative_exact.insert(phrase, spec);
                }
            }

            let mut relative_affix = Vec::with_capacity(raw.relative_affix.len());
            for (prefix, suffix, sign, u_id) in raw.relative_affix {
                if let Some(unit) = TimeUnit::from_u8(u_id) {
                    for part in prefix.split_whitespace().chain(suffix.split_whitespace()) {
                        if part.chars().count() >= 2 {
                            register_token(part, lang_id, true);
                        }
                    }
                    relative_affix.push(RelativeAffix {
                        prefix,
                        suffix,
                        sign,
                        unit,
                    });
                }
            }
            // Match longest affixes first for specificity
            relative_affix.sort_by(|a, b| {
                let la = a.prefix.len() + a.suffix.len();
                let lb = b.prefix.len() + b.suffix.len();
                lb.cmp(&la)
            });

            let mut word_numbers = FxHashMap::default();
            for (w, n) in raw.word_numbers {
                word_numbers.insert(w, n);
            }

            locales.push(CompiledLocale {
                id: lang_id,
                code: raw.code,
                date_order,
                months,
                weekdays,
                am,
                pm,
                units,
                ago,
                in_words,
                skip,
                relative_exact,
                relative_affix,
                word_numbers,
            });
        }

        Self {
            locales,
            code_to_id,
            token_to_langs,
            exact_phrase_index,
            known_date_tokens,
        }
    }

    pub fn resolve_lang_id(&self, code: &str) -> Option<usize> {
        let lower = code.trim().to_ascii_lowercase();
        if let Some(&id) = self.code_to_id.get(&lower) {
            return Some(id);
        }
        // Fallback: "en-US" or "tr_TR" -> "en" or "tr"
        if let Some(base) = lower.split(['-', '_']).next() {
            if let Some(&id) = self.code_to_id.get(base) {
                return Some(id);
            }
        }
        None
    }

    /// Detect top candidate language IDs in O(tokens) time without scanning all 205 locales.
    pub fn detect_candidates(
        &self,
        normalized: &str,
        tokens: &[&str],
        explicit_langs: Option<&[String]>,
        default_langs: &[String],
    ) -> Vec<usize> {
        if let Some(langs) = explicit_langs {
            if !langs.is_empty() {
                let mut out = Vec::with_capacity(langs.len());
                for code in langs {
                    if let Some(id) = self.resolve_lang_id(code) {
                        if !out.contains(&id) {
                            out.push(id);
                        }
                    }
                }
                return out;
            }
        }

        // Fast exact relative phrase lookup
        if let Some(matches) = self.exact_phrase_index.get(normalized) {
            let mut out: Vec<usize> = matches.iter().map(|(id, _)| *id as usize).collect();
            out.sort_unstable();
            out.dedup();
            return out;
        }

        // Script-based fast narrowing for non-Latin scripts
        let script_hint = detect_script_hint(normalized);

        let mut scores: FxHashMap<u16, u32> = FxHashMap::default();
        for &tok in tokens {
            if tok.len() <= 1 && tok.is_ascii() {
                continue;
            }
            if let Some(langs) = self.token_to_langs.get(tok) {
                // Rare/specific tokens carry higher weight than ultra-common 2-letter tokens
                let weight = if langs.len() <= 3 {
                    6
                } else if langs.len() <= 12 {
                    3
                } else {
                    1
                };
                for &lid in langs {
                    *scores.entry(lid).or_insert(0) += weight;
                }
            }
        }

        // Also check 2-word phrases (e.g. "il y a", "geçen hafta", "next week", "hace 2")
        for w in tokens.windows(2) {
            let bigram = format!("{} {}", w[0], w[1]);
            if let Some(langs) = self.token_to_langs.get(&bigram) {
                for &lid in langs {
                    *scores.entry(lid).or_insert(0) += 5;
                }
            }
        }

        // Boost script-matched languages (crucial for CJK/Arabic/Thai where no spaces exist)
        if let Some(hint_codes) = script_hint {
            for code in hint_codes {
                if let Some(id) = self.resolve_lang_id(code) {
                    *scores.entry(id as u16).or_insert(0) += 4;
                }
            }
        }

        // Slight tie-break boost for default_languages
        for code in default_langs {
            if let Some(id) = self.resolve_lang_id(code) {
                if let Some(sc) = scores.get_mut(&(id as u16)) {
                    *sc += 2;
                }
            }
        }

        if scores.is_empty() {
            return Vec::new();
        }

        let mut ranked: Vec<(u16, u32)> = scores.into_iter().collect();
        // Highest score first; ties broken by global language priority (lower id = higher priority)
        ranked.sort_unstable_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        ranked.into_iter().take(8).map(|(id, _)| id as usize).collect()
    }
}

fn detect_script_hint(s: &str) -> Option<&'static [&'static str]> {
    let mut has_hiragana_katakana = false;
    let mut has_han = false;
    let mut has_hangul = false;
    let mut has_cyrillic = false;
    let mut has_arabic = false;
    let mut has_hebrew = false;
    let mut has_greek = false;
    let mut has_thai = false;
    let mut has_devanagari = false;

    for ch in s.chars() {
        let c = ch as u32;
        match c {
            0x3040..=0x30FF => has_hiragana_katakana = true,
            0x4E00..=0x9FFF => has_han = true,
            0xAC00..=0xD7AF => has_hangul = true,
            0x0400..=0x04FF => has_cyrillic = true,
            0x0600..=0x06FF => has_arabic = true,
            0x0590..=0x05FF => has_hebrew = true,
            0x0370..=0x03FF => has_greek = true,
            0x0E00..=0x0E7F => has_thai = true,
            0x0900..=0x097F => has_devanagari = true,
            _ => {}
        }
    }

    if has_hiragana_katakana {
        Some(&["ja"])
    } else if has_hangul {
        Some(&["ko"])
    } else if has_han {
        Some(&["zh", "ja"])
    } else if has_cyrillic {
        Some(&["ru", "uk", "bg", "sr", "be", "kk"])
    } else if has_arabic {
        Some(&["ar", "fa", "ur"])
    } else if has_hebrew {
        Some(&["he"])
    } else if has_greek {
        Some(&["el"])
    } else if has_thai {
        Some(&["th"])
    } else if has_devanagari {
        Some(&["hi", "mr", "ne"])
    } else {
        None
    }
}

/// Normalize Unicode digits (Arabic-Indic, Eastern Arabic, Devanagari, Fullwidth) to ASCII 0-9
/// and collapse whitespace/punctuation for fast matching.
pub fn normalize_digits_and_lower(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = true;
    for ch in input.chars() {
        let mapped = match ch {
            '\u{0660}'..='\u{0669}' => char::from_u32('0' as u32 + (ch as u32 - 0x0660)).unwrap(),
            '\u{06F0}'..='\u{06F9}' => char::from_u32('0' as u32 + (ch as u32 - 0x06F0)).unwrap(),
            '\u{0966}'..='\u{096F}' => char::from_u32('0' as u32 + (ch as u32 - 0x0966)).unwrap(),
            '\u{FF10}'..='\u{FF19}' => char::from_u32('0' as u32 + (ch as u32 - 0xFF10)).unwrap(),
            // Turkish dotted/dotless I normalization to match lowercase tables cleanly
            '\u{0130}' => 'i',
            '\u{00A0}' | '\u{2009}' | '\u{202F}' => ' ',
            other => other,
        };
        if mapped.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else if mapped == '\u{0307}' {
            // Skip stray combining dot above (from lowercasing Turkish İ)
            continue;
        } else {
            for lc in mapped.to_lowercase() {
                if lc != '\u{0307}' {
                    out.push(lc);
                }
            }
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}
