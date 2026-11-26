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
    pub relative_exact: FxHashMap<String, Vec<RelativeSpec>>,
    pub relative_affix: Vec<RelativeAffix>,
    pub word_numbers: FxHashMap<String, u32>,
    pub time_of_day: FxHashMap<String, u8>,
    // Diacritic-folded secondary tables (populated only where folded != exact)
    pub folded_months: FxHashMap<String, u8>,
    pub folded_weekdays: FxHashMap<String, u8>,
    pub folded_units: FxHashMap<String, TimeUnit>,
    pub folded_ago: FxHashSet<String>,
    pub folded_in_words: FxHashSet<String>,
    pub folded_relative_exact: FxHashMap<String, Vec<RelativeSpec>>,
    pub folded_relative_affix: Vec<RelativeAffix>,
    pub folded_time_of_day: FxHashMap<String, u8>,
}

pub struct LocaleRegistry {
    pub locales: Vec<CompiledLocale>,
    pub code_to_id: FxHashMap<String, usize>,
    /// Global inverted index: lowercase token -> priority-ordered list of locale IDs.
    pub token_to_langs: FxHashMap<String, Vec<u16>>,
    /// Folded inverted index: diacritic-folded token -> priority-ordered list of locale IDs.
    pub folded_token_to_langs: FxHashMap<String, Vec<u16>>,
    /// Global exact relative phrase index: phrase -> list of (lang_id, RelativeSpec).
    pub exact_phrase_index: FxHashMap<String, Vec<(u16, RelativeSpec)>>,
    /// Folded exact relative phrase index: folded phrase -> list of (lang_id, RelativeSpec).
    pub folded_exact_phrase_index: FxHashMap<String, Vec<(u16, RelativeSpec)>>,
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
        let mut folded_token_to_langs: FxHashMap<String, Vec<u16>> = FxHashMap::default();
        let mut exact_phrase_index: FxHashMap<String, Vec<(u16, RelativeSpec)>> =
            FxHashMap::default();
        let mut folded_exact_phrase_index: FxHashMap<String, Vec<(u16, RelativeSpec)>> =
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
            let folded = fold_diacritics(tok);
            if folded != tok && !folded.is_empty() {
                let fentry = folded_token_to_langs.entry(folded.clone()).or_default();
                if fentry.last().copied() != Some(lang_id) && !fentry.contains(&lang_id) {
                    fentry.push(lang_id);
                }
                if is_search_token {
                    known_date_tokens.insert(folded);
                }
            }
        };

        for (idx, raw) in raw_locales.into_iter().enumerate() {
            let lang_id = idx as u16;
            code_to_id.insert(raw.code.to_ascii_lowercase(), idx);
            let date_order = DateOrder::from_str(&raw.date_order).unwrap_or(DateOrder::DMY);

            let mut months = FxHashMap::default();
            let mut folded_months = FxHashMap::default();
            for (m_idx, names) in raw.months.into_iter().enumerate() {
                let month_num = (m_idx + 1) as u8;
                for name in names {
                    for part in name.split_whitespace() {
                        if part.chars().count() >= 2 {
                            register_token(part, lang_id, true);
                        }
                    }
                    register_token(&name, lang_id, true);
                    let norm_name = normalize_digits_and_lower(&name);
                    if norm_name != name {
                        register_token(&norm_name, lang_id, true);
                        let nf = fold_diacritics(&norm_name);
                        if nf != norm_name {
                            folded_months.entry(nf).or_insert(month_num);
                        }
                        months.insert(norm_name, month_num);
                    }
                    let f = fold_diacritics(&name);
                    if f != name {
                        folded_months.entry(f).or_insert(month_num);
                    }
                    months.insert(name, month_num);
                }
            }

            let mut weekdays = FxHashMap::default();
            let mut folded_weekdays = FxHashMap::default();
            for (d_idx, names) in raw.weekdays.into_iter().enumerate() {
                let wd_num = d_idx as u8;
                for name in names {
                    for part in name.split_whitespace() {
                        if part.chars().count() >= 2 {
                            register_token(part, lang_id, true);
                        }
                    }
                    register_token(&name, lang_id, true);
                    let norm_name = normalize_digits_and_lower(&name);
                    if norm_name != name {
                        register_token(&norm_name, lang_id, true);
                        let nf = fold_diacritics(&norm_name);
                        if nf != norm_name {
                            folded_weekdays.entry(nf).or_insert(wd_num);
                        }
                        weekdays.insert(norm_name, wd_num);
                    }
                    let f = fold_diacritics(&name);
                    if f != name {
                        folded_weekdays.entry(f).or_insert(wd_num);
                    }
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
            let mut folded_units = FxHashMap::default();
            for (tok, u_id) in raw.units {
                if let Some(u) = TimeUnit::from_u8(u_id) {
                    register_token(&tok, lang_id, true);
                    let f = fold_diacritics(&tok);
                    if f != tok {
                        folded_units.entry(f).or_insert(u);
                    }
                    units.insert(tok, u);
                }
            }

            let mut ago = FxHashSet::default();
            let mut folded_ago = FxHashSet::default();
            for s in raw.ago {
                for part in s.split_whitespace() {
                    register_token(part, lang_id, true);
                }
                register_token(&s, lang_id, true);
                let f = fold_diacritics(&s);
                if f != s {
                    folded_ago.insert(f);
                }
                ago.insert(s);
            }

            let mut in_words = FxHashSet::default();
            let mut folded_in_words = FxHashSet::default();
            for s in raw.in_words {
                for part in s.split_whitespace() {
                    register_token(part, lang_id, true);
                }
                register_token(&s, lang_id, true);
                let f = fold_diacritics(&s);
                if f != s {
                    folded_in_words.insert(f);
                }
                in_words.insert(s);
            }

            let mut skip = FxHashSet::default();
            for s in raw.skip {
                skip.insert(s);
            }

            let mut relative_exact: FxHashMap<String, Vec<RelativeSpec>> = FxHashMap::default();
            let mut folded_relative_exact: FxHashMap<String, Vec<RelativeSpec>> =
                FxHashMap::default();
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
                    let f = fold_diacritics(&phrase);
                    if f != phrase {
                        folded_exact_phrase_index
                            .entry(f.clone())
                            .or_default()
                            .push((lang_id, spec));
                        folded_relative_exact.entry(f).or_default().push(spec);
                    }
                    relative_exact.entry(phrase).or_default().push(spec);
                }
            }

            let mut relative_affix = Vec::with_capacity(raw.relative_affix.len());
            let mut folded_relative_affix = Vec::new();
            for (prefix, suffix, sign, u_id) in raw.relative_affix {
                if let Some(unit) = TimeUnit::from_u8(u_id) {
                    for part in prefix.split_whitespace().chain(suffix.split_whitespace()) {
                        if part.chars().count() >= 2 {
                            register_token(part, lang_id, true);
                        }
                    }
                    let fp = fold_diacritics(&prefix);
                    let fs = fold_diacritics(&suffix);
                    if fp != prefix || fs != suffix {
                        folded_relative_affix.push(RelativeAffix {
                            prefix: fp,
                            suffix: fs,
                            sign,
                            unit,
                        });
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
            let sort_affix = |v: &mut Vec<RelativeAffix>| {
                v.sort_by(|a, b| {
                    let la = a.prefix.len() + a.suffix.len();
                    let lb = b.prefix.len() + b.suffix.len();
                    lb.cmp(&la)
                });
            };
            sort_affix(&mut relative_affix);
            sort_affix(&mut folded_relative_affix);

            let mut word_numbers = FxHashMap::default();
            for (w, n) in raw.word_numbers {
                let f = fold_diacritics(&w);
                if f != w {
                    word_numbers.entry(f).or_insert(n);
                }
                word_numbers.insert(w, n);
            }

            let mut time_of_day = FxHashMap::default();
            let mut folded_time_of_day = FxHashMap::default();
            for (phrase, hour) in raw.time_of_day {
                for part in phrase.split_whitespace() {
                    register_token(part, lang_id, true);
                }
                register_token(&phrase, lang_id, true);
                let f = fold_diacritics(&phrase);
                if f != phrase {
                    folded_time_of_day.entry(f).or_insert(hour);
                }
                time_of_day.insert(phrase, hour);
            }

            // Ensure key compound time-of-day trigger words are indexed for search_dates / lang detect
            if raw.code == "en" {
                register_token("tonight", lang_id, true);
            } else if raw.code == "es" {
                register_token("anoche", lang_id, true);
            } else if raw.code == "tr" {
                register_token("saat", lang_id, true);
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
                time_of_day,
                folded_months,
                folded_weekdays,
                folded_units,
                folded_ago,
                folded_in_words,
                folded_relative_exact,
                folded_relative_affix,
                folded_time_of_day,
            });
        }

        Self {
            locales,
            code_to_id,
            token_to_langs,
            folded_token_to_langs,
            exact_phrase_index,
            folded_exact_phrase_index,
            known_date_tokens,
        }
    }

    pub fn resolve_lang_id(&self, code: &str) -> Option<usize> {
        let lower = code.trim().to_ascii_lowercase();
        if let Some(&id) = self.code_to_id.get(&lower) {
            return Some(id);
        }
        // Handle underscore/hyphen variants (e.g. "uz_cyrl" -> "uz-cyrl")
        let dash = lower.replace('_', "-");
        if let Some(&id) = self.code_to_id.get(&dash) {
            return Some(id);
        }
        // Fallback: "en-US" or "tr_TR" -> "en" or "tr"
        if let Some(base) = dash.split('-').next() {
            if let Some(&id) = self.code_to_id.get(base) {
                return Some(id);
            }
        }
        // Legacy / ISO 639-1 macro aliases when exact code isn't present
        let aliased = match lower.as_str() {
            "no" => Some("nb"),
            "iw" => Some("he"),
            "in" => Some("id"),
            "ji" => Some("yi"),
            "fil" => Some("tl"),
            _ => None,
        };
        if let Some(target) = aliased {
            if let Some(&id) = self.code_to_id.get(target) {
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
        normalize: bool,
        use_given_order: bool,
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

        let order_with_defaults = |mut ids: Vec<usize>| -> Vec<usize> {
            if use_given_order && !default_langs.is_empty() {
                let mut ordered = Vec::with_capacity(ids.len());
                for code in default_langs {
                    if let Some(did) = self.resolve_lang_id(code) {
                        if ids.contains(&did) && !ordered.contains(&did) {
                            ordered.push(did);
                        }
                    }
                }
                for id in ids {
                    if !ordered.contains(&id) {
                        ordered.push(id);
                    }
                }
                ids = ordered;
            }
            ids
        };

        // Fast exact relative phrase lookup
        if let Some(matches) = self.exact_phrase_index.get(normalized) {
            let mut out: Vec<usize> = matches.iter().map(|(id, _)| *id as usize).collect();
            out.sort_unstable();
            out.dedup();
            return order_with_defaults(out);
        }

        if normalize {
            let f = fold_diacritics(normalized);
            if let Some(matches) = self.folded_exact_phrase_index.get(&f) {
                let mut out: Vec<usize> = matches.iter().map(|(id, _)| *id as usize).collect();
                out.sort_unstable();
                out.dedup();
                return order_with_defaults(out);
            }
        }

        // Script-based fast narrowing for non-Latin scripts
        let script_hint = detect_script_hint(normalized);

        let mut scores: FxHashMap<u16, u32> = FxHashMap::default();
        for &tok in tokens {
            if tok.len() <= 1 && tok.is_ascii() {
                continue;
            }
            if let Some(langs) = self.token_to_langs.get(tok) {
                let weight = if langs.len() <= 3 {
                    8
                } else if langs.len() <= 12 {
                    4
                } else {
                    2
                };
                for &lid in langs {
                    *scores.entry(lid).or_insert(0) += weight;
                }
            }
            if normalize {
                let ft = fold_diacritics(tok);
                if let Some(langs) = self.folded_token_to_langs.get(&ft) {
                    let weight = if langs.len() <= 3 {
                        5
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
        }

        // Also check 2-word phrases (e.g. "il y a", "geçen hafta", "next week", "hace 2")
        for w in tokens.windows(2) {
            let bigram = format!("{} {}", w[0], w[1]);
            if let Some(langs) = self.token_to_langs.get(&bigram) {
                for &lid in langs {
                    *scores.entry(lid).or_insert(0) += 6;
                }
            }
            if normalize {
                let fb = fold_diacritics(&bigram);
                if fb != bigram {
                    if let Some(langs) = self.folded_token_to_langs.get(&fb) {
                        for &lid in langs {
                            *scores.entry(lid).or_insert(0) += 4;
                        }
                    }
                }
            }
        }

        // Boost script-matched languages (crucial for CJK/Arabic/Thai where no spaces exist)
        if let Some(hint_codes) = script_hint {
            for code in hint_codes {
                if let Some(id) = self.resolve_lang_id(code) {
                    *scores.entry(id as u16).or_insert(0) += 5;
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

        let top: Vec<usize> = ranked.into_iter().take(8).map(|(id, _)| id as usize).collect();
        order_with_defaults(top)
    }
}

/// Fold common Latin and Arabic diacritics for accent-tolerant matching (`NORMALIZE=True`).
pub fn fold_diacritics(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let mapped = match ch {
            'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ø' | 'ō' | 'ŏ' | 'ő' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
            'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
            'ğ' | 'ĝ' | 'ġ' | 'ģ' => 'g',
            'ş' | 'ś' | 'ŝ' | 'š' => 's',
            'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
            'ý' | 'ÿ' | 'ŷ' => 'y',
            'ž' | 'ź' | 'ż' => 'z',
            'ł' | 'ĺ' | 'ļ' | 'ľ' => 'l',
            'ř' | 'ŕ' | 'ŗ' => 'r',
            'ď' | 'đ' => 'd',
            'ť' | 'ţ' => 't',
            // Arabic alef normalization & harakat/tatweel/ZWNJ stripping
            'أ' | 'إ' | 'آ' | 'ٱ' => 'ا',
            'ى' => 'ي',
            '\u{200C}' => ' ',
            '\u{064B}'..='\u{065F}' | '\u{0670}' | '\u{0640}' | '\u{0300}'..='\u{036F}' => {
                continue;
            }
            other => other,
        };
        out.push(mapped);
    }
    out
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
    let mut has_bengali = false;
    let mut has_gurmukhi = false;
    let mut has_gujarati = false;
    let mut has_tamil = false;
    let mut has_telugu = false;
    let mut has_kannada = false;
    let mut has_malayalam = false;
    let mut has_sinhala = false;
    let mut has_lao = false;
    let mut has_myanmar = false;
    let mut has_georgian = false;
    let mut has_armenian = false;
    let mut has_ethiopic = false;
    let mut has_khmer = false;

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
            0x0980..=0x09FF => has_bengali = true,
            0x0A00..=0x0A7F => has_gurmukhi = true,
            0x0A80..=0x0AFF => has_gujarati = true,
            0x0B80..=0x0BFF => has_tamil = true,
            0x0C00..=0x0C7F => has_telugu = true,
            0x0C80..=0x0CFF => has_kannada = true,
            0x0D00..=0x0D7F => has_malayalam = true,
            0x0D80..=0x0DFF => has_sinhala = true,
            0x0E80..=0x0EFF => has_lao = true,
            0x1000..=0x109F => has_myanmar = true,
            0x10A0..=0x10FF => has_georgian = true,
            0x0530..=0x058F => has_armenian = true,
            0x1200..=0x137F => has_ethiopic = true,
            0x1780..=0x17FF => has_khmer = true,
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
        Some(&["ru", "uk", "bg", "sr", "be", "kk", "mk", "ky", "mn"])
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
    } else if has_bengali {
        Some(&["bn", "as"])
    } else if has_gurmukhi {
        Some(&["pa"])
    } else if has_gujarati {
        Some(&["gu"])
    } else if has_tamil {
        Some(&["ta"])
    } else if has_telugu {
        Some(&["te"])
    } else if has_kannada {
        Some(&["kn"])
    } else if has_malayalam {
        Some(&["ml"])
    } else if has_sinhala {
        Some(&["si"])
    } else if has_lao {
        Some(&["lo"])
    } else if has_myanmar {
        Some(&["my"])
    } else if has_georgian {
        Some(&["ka"])
    } else if has_armenian {
        Some(&["hy"])
    } else if has_ethiopic {
        Some(&["am", "ti"])
    } else if has_khmer {
        Some(&["km"])
    } else {
        None
    }
}

/// Normalize all 18 Unicode decimal numeral systems to ASCII 0-9
/// and collapse whitespace/punctuation for fast matching.
pub fn normalize_digits_and_lower(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = true;
    for ch in input.chars() {
        let mapped = match ch {
            '\u{0660}'..='\u{0669}' => char::from_u32('0' as u32 + (ch as u32 - 0x0660)).unwrap(),
            '\u{06F0}'..='\u{06F9}' => char::from_u32('0' as u32 + (ch as u32 - 0x06F0)).unwrap(),
            '\u{0966}'..='\u{096F}' => char::from_u32('0' as u32 + (ch as u32 - 0x0966)).unwrap(),
            '\u{09E6}'..='\u{09EF}' => char::from_u32('0' as u32 + (ch as u32 - 0x09E6)).unwrap(),
            '\u{0A66}'..='\u{0A6F}' => char::from_u32('0' as u32 + (ch as u32 - 0x0A66)).unwrap(),
            '\u{0AE6}'..='\u{0AEF}' => char::from_u32('0' as u32 + (ch as u32 - 0x0AE6)).unwrap(),
            '\u{0B66}'..='\u{0B6F}' => char::from_u32('0' as u32 + (ch as u32 - 0x0B66)).unwrap(),
            '\u{0BE6}'..='\u{0BEF}' => char::from_u32('0' as u32 + (ch as u32 - 0x0BE6)).unwrap(),
            '\u{0C66}'..='\u{0C6F}' => char::from_u32('0' as u32 + (ch as u32 - 0x0C66)).unwrap(),
            '\u{0CE6}'..='\u{0CEF}' => char::from_u32('0' as u32 + (ch as u32 - 0x0CE6)).unwrap(),
            '\u{0D66}'..='\u{0D6F}' => char::from_u32('0' as u32 + (ch as u32 - 0x0D66)).unwrap(),
            '\u{0DE6}'..='\u{0DEF}' => char::from_u32('0' as u32 + (ch as u32 - 0x0DE6)).unwrap(),
            '\u{0E50}'..='\u{0E59}' => char::from_u32('0' as u32 + (ch as u32 - 0x0E50)).unwrap(),
            '\u{0ED0}'..='\u{0EDF}' => char::from_u32('0' as u32 + (ch as u32 - 0x0ED0)).unwrap(),
            '\u{0F20}'..='\u{0F29}' => char::from_u32('0' as u32 + (ch as u32 - 0x0F20)).unwrap(),
            '\u{1040}'..='\u{1049}' => char::from_u32('0' as u32 + (ch as u32 - 0x1040)).unwrap(),
            '\u{17E0}'..='\u{17E9}' => char::from_u32('0' as u32 + (ch as u32 - 0x17E0)).unwrap(),
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
