pub mod schema;

use crate::settings::{DateOrder, Settings};
use rustc_hash::{FxHashMap, FxHashSet};
use schema::RawLocale;
use std::borrow::Cow;
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
    pub folded_am: FxHashSet<String>,
    pub folded_pm: FxHashSet<String>,
    pub units: FxHashMap<String, TimeUnit>,
    pub ago: FxHashSet<String>,
    pub in_words: FxHashSet<String>,
    pub ago_sorted: Vec<String>,
    pub in_words_sorted: Vec<String>,
    pub skip: FxHashSet<String>,
    pub relative_exact: FxHashMap<String, Vec<RelativeSpec>>,
    pub relative_affix: Vec<RelativeAffix>,
    pub word_numbers: FxHashMap<String, u32>,
    pub ordinals: FxHashMap<String, u32>,
    pub time_of_day: FxHashMap<String, u8>,
    // Diacritic-folded secondary tables (populated only where folded != exact)
    pub folded_months: FxHashMap<String, u8>,
    pub folded_weekdays: FxHashMap<String, u8>,
    pub folded_units: FxHashMap<String, TimeUnit>,
    pub folded_ago: FxHashSet<String>,
    pub folded_in_words: FxHashSet<String>,
    pub folded_ago_sorted: Vec<String>,
    pub folded_in_words_sorted: Vec<String>,
    pub folded_ordinals: FxHashMap<String, u32>,
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
    /// Cardinal & ordinal word numeral tokens across 205 locales (cannot start bare prose matches).
    pub known_numeral_tokens: FxHashSet<String>,
    /// Non-ASCII numeral words for agglutinative/spaceless token prefix detection (e.g. Sinhala, Thai).
    pub non_ascii_numeral_words: Vec<String>,
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
        let mut known_numeral_tokens: FxHashSet<String> = FxHashSet::default();

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
                let q_trim = tok.trim_matches(|c: char| matches!(c, '\'' | '’' | 'ꞌ' | 'ʻ'));
                if !q_trim.is_empty() && q_trim != tok {
                    known_date_tokens.insert(q_trim.to_string());
                }
            }
            let folded = fold_diacritics(tok);
            if folded.as_ref() != tok && !folded.is_empty() {
                let folded_s = folded.into_owned();
                let fentry = folded_token_to_langs.entry(folded_s.clone()).or_default();
                if fentry.last().copied() != Some(lang_id) && !fentry.contains(&lang_id) {
                    fentry.push(lang_id);
                }
                if is_search_token {
                    known_date_tokens.insert(folded_s);
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
                        if nf.as_ref() != norm_name {
                            folded_months.entry(nf.into_owned()).or_insert(month_num);
                        }
                        months.insert(norm_name, month_num);
                    }
                    let f = fold_diacritics(&name);
                    if f.as_ref() != name {
                        folded_months.entry(f.into_owned()).or_insert(month_num);
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
                        if nf.as_ref() != norm_name {
                            folded_weekdays.entry(nf.into_owned()).or_insert(wd_num);
                        }
                        weekdays.insert(norm_name, wd_num);
                    }
                    let f = fold_diacritics(&name);
                    if f.as_ref() != name {
                        folded_weekdays.entry(f.into_owned()).or_insert(wd_num);
                    }
                    weekdays.insert(name, wd_num);
                }
            }

            let mut am = FxHashSet::default();
            let mut folded_am = FxHashSet::default();
            for s in raw.am {
                register_token(&s, lang_id, true);
                let f = fold_diacritics(&s);
                if f.as_ref() != s {
                    folded_am.insert(f.into_owned());
                }
                am.insert(s);
            }
            let mut pm = FxHashSet::default();
            let mut folded_pm = FxHashSet::default();
            for s in raw.pm {
                register_token(&s, lang_id, true);
                let f = fold_diacritics(&s);
                if f.as_ref() != s {
                    folded_pm.insert(f.into_owned());
                }
                pm.insert(s);
            }

            let mut units = FxHashMap::default();
            let mut folded_units = FxHashMap::default();
            for (tok, u_id) in raw.units {
                if let Some(u) = TimeUnit::from_u8(u_id) {
                    register_token(&tok, lang_id, true);
                    let f = fold_diacritics(&tok);
                    if f.as_ref() != tok {
                        folded_units.entry(f.into_owned()).or_insert(u);
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
                if f.as_ref() != s {
                    folded_ago.insert(f.into_owned());
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
                if f.as_ref() != s {
                    folded_in_words.insert(f.into_owned());
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
                    if f.as_ref() != phrase {
                        let fs = f.into_owned();
                        folded_exact_phrase_index
                            .entry(fs.clone())
                            .or_default()
                            .push((lang_id, spec));
                        folded_relative_exact.entry(fs).or_default().push(spec);
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
                    if fp.as_ref() != prefix || fs.as_ref() != suffix {
                        folded_relative_affix.push(RelativeAffix {
                            prefix: fp.into_owned(),
                            suffix: fs.into_owned(),
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
                if f.as_ref() != w {
                    let fs = f.into_owned();
                    word_numbers.entry(fs.clone()).or_insert(n);
                    known_numeral_tokens.insert(fs);
                }
                for part in w.split_whitespace() {
                    known_numeral_tokens.insert(part.to_string());
                    let pf = fold_diacritics(part);
                    if pf.as_ref() != part {
                        known_numeral_tokens.insert(pf.into_owned());
                    }
                    if part.chars().count() >= 3 || !part.is_ascii() {
                        register_token(part, lang_id, false);
                    }
                }
                word_numbers.insert(w, n);
            }

            let mut ordinals = FxHashMap::default();
            let mut folded_ordinals = FxHashMap::default();
            for (w, n) in raw.ordinals {
                let f = fold_diacritics(&w);
                if f.as_ref() != w {
                    let fs = f.into_owned();
                    folded_ordinals.entry(fs.clone()).or_insert(n);
                    known_numeral_tokens.insert(fs);
                }
                known_numeral_tokens.insert(w.clone());
                if w.chars().count() >= 3 || !w.is_ascii() {
                    register_token(&w, lang_id, false);
                }
                ordinals.insert(w, n);
            }

            let mut time_of_day = FxHashMap::default();
            let mut folded_time_of_day = FxHashMap::default();
            for (phrase, hour) in raw.time_of_day {
                for part in phrase.split_whitespace() {
                    register_token(part, lang_id, true);
                }
                register_token(&phrase, lang_id, true);
                let f = fold_diacritics(&phrase);
                if f.as_ref() != phrase {
                    folded_time_of_day.entry(f.into_owned()).or_insert(hour);
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

            let sort_desc = |set: &FxHashSet<String>| -> Vec<String> {
                let mut v: Vec<String> = set.iter().cloned().collect();
                v.sort_unstable_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
                v
            };
            let ago_sorted = sort_desc(&ago);
            let in_words_sorted = sort_desc(&in_words);
            let folded_ago_sorted = sort_desc(&folded_ago);
            let folded_in_words_sorted = sort_desc(&folded_in_words);

            locales.push(CompiledLocale {
                id: lang_id,
                code: raw.code,
                date_order,
                months,
                weekdays,
                am,
                pm,
                folded_am,
                folded_pm,
                units,
                ago,
                in_words,
                ago_sorted,
                in_words_sorted,
                skip,
                relative_exact,
                relative_affix,
                word_numbers,
                ordinals,
                time_of_day,
                folded_months,
                folded_weekdays,
                folded_units,
                folded_ago,
                folded_in_words,
                folded_ago_sorted,
                folded_in_words_sorted,
                folded_ordinals,
                folded_relative_exact,
                folded_relative_affix,
                folded_time_of_day,
            });
        }

        // Register disambiguating locale connectors so candidate detection ranks e.g. it/et/id accurately
        for &(tok, langs) in &[
            ("alle", &["it", "de"][..]),
            ("kell", &["et"][..]),
            ("pukul", &["id", "ms"][..]),
            ("ora", &["ro"][..]),
            ("klo", &["fi"][..]),
            ("στις", &["el"][..]),
            ("lúc", &["vi"][..]),
            ("เวลา", &["th"][..]),
            ("الساعة", &["ar"][..]),
            ("at", &["en"][..]),
            ("um", &["de"][..]),
        ] {
            for &code in langs {
                if let Some(&lid) = code_to_id.get(code) {
                    let list = token_to_langs.entry(tok.to_string()).or_default();
                    let lid_u16 = lid as u16;
                    if !list.contains(&lid_u16) {
                        list.push(lid_u16);
                    }
                }
            }
        }

        let non_ascii_numeral_words: Vec<String> = known_numeral_tokens
            .iter()
            .filter(|w| !w.is_ascii() && w.chars().count() >= 2)
            .cloned()
            .collect();

        Self {
            locales,
            code_to_id,
            token_to_langs,
            folded_token_to_langs,
            exact_phrase_index,
            folded_exact_phrase_index,
            known_date_tokens,
            known_numeral_tokens,
            non_ascii_numeral_words,
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

        if normalize
            && !self
                .locales
                .first()
                .is_some_and(|en| en.months.contains_key(normalized) || en.weekdays.contains_key(normalized))
        {
            let f = fold_diacritics(normalized);
            if let Some(matches) = self.folded_exact_phrase_index.get(f.as_ref()) {
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
            let exact_langs = self.token_to_langs.get(tok);
            let exact_matched = if let Some(langs) = exact_langs {
                let weight = if langs.len() <= 3 {
                    16
                } else if langs.len() <= 12 {
                    10
                } else {
                    6
                };
                for &lid in langs {
                    *scores.entry(lid).or_insert(0) += weight;
                }
                true
            } else {
                false
            };
            if normalize {
                let ft = fold_diacritics(tok);
                // Only award meaningful folded points when input wasn't already an exact token
                // (prevents diacritic-stripped rare locales like "jûn"/"nóv"/"ápr" from outscoring "jun"/"nov"/"apr")
                if let Some(langs) = self.folded_token_to_langs.get(ft.as_ref()) {
                    let weight = if !exact_matched || ft.as_ref() != tok {
                        if langs.len() <= 3 {
                            5
                        } else if langs.len() <= 12 {
                            3
                        } else {
                            1
                        }
                    } else if langs.len() <= 3 {
                        2
                    } else {
                        1
                    };
                    for &lid in langs {
                        // Never double-count the same locale for both exact and folded on one token
                        if let Some(ex) = exact_langs {
                            if ex.binary_search(&lid).is_ok() {
                                continue;
                            }
                        }
                        *scores.entry(lid).or_insert(0) += weight;
                    }
                }
            }
        }

        // Also check 2-word phrases (e.g. "il y a", "geçen hafta", "next week", "hace 2") using a single reused scratch buffer
        let mut bigram = String::with_capacity(32);
        for w in tokens.windows(2) {
            bigram.clear();
            bigram.push_str(w[0]);
            bigram.push(' ');
            bigram.push_str(w[1]);
            if let Some(langs) = self.token_to_langs.get(&bigram) {
                for &lid in langs {
                    *scores.entry(lid).or_insert(0) += 6;
                }
            }
            if normalize {
                let fb = fold_diacritics(&bigram);
                if fb.as_ref() != bigram {
                    if let Some(langs) = self.folded_token_to_langs.get(fb.as_ref()) {
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

        let top: Vec<usize> = ranked.into_iter().take(12).map(|(id, _)| id as usize).collect();
        order_with_defaults(top)
    }
}

/// Fold common Latin and Arabic diacritics for accent-tolerant matching (`NORMALIZE=True`), zero-alloc on ASCII.
pub fn fold_diacritics(s: &str) -> Cow<'_, str> {
    if s.is_ascii() {
        return Cow::Borrowed(s);
    }
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
    Cow::Owned(out)
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
    let mut has_oriya = false;
    let mut has_tamil = false;
    let mut has_telugu = false;
    let mut has_kannada = false;
    let mut has_malayalam = false;
    let mut has_sinhala = false;
    let mut has_lao = false;
    let mut has_tibetan = false;
    let mut has_myanmar = false;
    let mut has_georgian = false;
    let mut has_armenian = false;
    let mut has_ethiopic = false;
    let mut has_cherokee = false;
    let mut has_khmer = false;
    let mut has_tifinagh = false;
    let mut has_yi = false;

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
            0x0B00..=0x0B7F => has_oriya = true,
            0x0B80..=0x0BFF => has_tamil = true,
            0x0C00..=0x0C7F => has_telugu = true,
            0x0C80..=0x0CFF => has_kannada = true,
            0x0D00..=0x0D7F => has_malayalam = true,
            0x0D80..=0x0DFF => has_sinhala = true,
            0x0E80..=0x0EFF => has_lao = true,
            0x0F00..=0x0FFF => has_tibetan = true,
            0x1000..=0x109F => has_myanmar = true,
            0x10A0..=0x10FF => has_georgian = true,
            0x0530..=0x058F => has_armenian = true,
            0x1200..=0x137F => has_ethiopic = true,
            0x13A0..=0x13FF | 0xAB70..=0xABBF => has_cherokee = true,
            0x1780..=0x17FF => has_khmer = true,
            0x2D30..=0x2D7F => has_tifinagh = true,
            0xA000..=0xA4CF => has_yi = true,
            _ => {}
        }
    }

    if has_hiragana_katakana {
        Some(&["ja"])
    } else if has_hangul {
        Some(&["ko"])
    } else if has_han {
        Some(&["zh", "ja", "zh-hans", "zh-hant", "yue"])
    } else if has_cyrillic {
        Some(&[
            "ru", "uk", "bg", "sr", "be", "kk", "mk", "ky", "mn", "az-cyrl", "uz-cyrl",
            "bs-cyrl", "sr-cyrl", "ce", "os", "sah",
        ])
    } else if has_arabic {
        Some(&[
            "ar", "fa", "ur", "ps", "ckb", "ug", "ks", "pa-arab", "uz-arab", "lrc", "mzn",
        ])
    } else if has_hebrew {
        Some(&["he", "yi"])
    } else if has_greek {
        Some(&["el"])
    } else if has_thai {
        Some(&["th"])
    } else if has_devanagari {
        Some(&["hi", "mr", "ne", "kok", "brx"])
    } else if has_bengali {
        Some(&["bn", "as"])
    } else if has_gurmukhi {
        Some(&["pa", "pa-guru"])
    } else if has_gujarati {
        Some(&["gu"])
    } else if has_oriya {
        Some(&["or"])
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
    } else if has_tibetan {
        Some(&["bo", "dz"])
    } else if has_myanmar {
        Some(&["my"])
    } else if has_georgian {
        Some(&["ka"])
    } else if has_armenian {
        Some(&["hy"])
    } else if has_ethiopic {
        Some(&["am", "ti"])
    } else if has_cherokee {
        Some(&["chr"])
    } else if has_khmer {
        Some(&["km"])
    } else if has_tifinagh {
        Some(&["zgh", "shi-tfng", "shi"])
    } else if has_yi {
        Some(&["ii"])
    } else {
        None
    }
}

/// Whether `code` is a major auto-detect language safe for short unanchored tokens.
#[inline]
pub fn is_major_auto_locale(code: &str) -> bool {
    matches!(
        code,
        "en" | "tr" | "de" | "fr" | "es" | "it" | "pt" | "nl" | "ru" | "ar" | "fa" | "zh"
            | "ja" | "ko" | "pl" | "uk" | "sv" | "no" | "nb" | "nn" | "da" | "fi" | "cs"
            | "sk" | "ro" | "hu" | "el" | "he" | "hi" | "id" | "ms" | "vi" | "th" | "tl"
    )
}

/// Whether `settings.languages` permits the given base locale code.
#[inline]
pub fn settings_allows_locale(settings: &Settings, code: &str) -> bool {
    let Some(ref ls) = settings.languages else {
        return true;
    };
    let reg = LocaleRegistry::global();
    let target_base = code.split(['-', '_']).next().unwrap_or(code);
    ls.iter().any(|l| {
        let base = l.split(['-', '_']).next().unwrap_or(l);
        if l.eq_ignore_ascii_case(code) || base.eq_ignore_ascii_case(target_base) {
            return true;
        }
        reg.resolve_lang_id(l)
            .is_some_and(|id| reg.locales[id].code == code)
    })
}

/// Returns `true` if `code` is a common uppercase 3-letter ISO-4217 currency code.
#[inline]
pub fn is_iso4217_currency(tok: &str) -> bool {
    tok.len() >= 3
        && tok.len() <= 4
        && tok.bytes().all(|b| b.is_ascii_uppercase())
        && matches!(
            tok,
            "EUR" | "USD" | "GBP" | "SEK" | "NOK" | "DKK" | "CHF" | "JPY" | "CNY" | "RMB"
                | "CAD" | "AUD" | "NZD" | "TRY" | "PLN" | "CZK" | "HUF" | "RON" | "BGN"
                | "RUB" | "UAH" | "INR" | "BRL" | "MXN" | "ARS" | "CLP" | "COP" | "PEN"
                | "ZAR" | "NGN" | "KES" | "EGP" | "MAD" | "KRW" | "SGD" | "HKD" | "TWD"
                | "THB" | "IDR" | "MYR" | "PHP" | "VND" | "ILS" | "AED" | "SAR" | "QAR"
                | "KWD" | "BHD" | "OMR" | "JOD" | "PKR" | "BDT" | "LKR" | "ISK" | "RSD"
                | "HRK" | "GEL" | "AMD" | "AZN" | "KZT" | "UZS" | "BTC" | "ETH" | "USDT"
        )
}

#[inline]
fn has_khmer_chars(s: &str) -> bool {
    s.chars().any(|c| ('\u{1780}'..='\u{17FF}').contains(&c))
}

#[inline]
fn is_invisible_format_control(c: char, keep_zwsp: bool) -> bool {
    if c == '\u{200B}' {
        return !keep_zwsp;
    }
    matches!(
        c,
        '\u{FEFF}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{00AD}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

#[inline]
fn is_edge_emoji(c: char) -> bool {
    matches!(
        c,
        '\u{1F300}'..='\u{1FAFF}' | '\u{2600}'..='\u{27BF}' | '\u{FE0E}' | '\u{FE0F}'
    )
}

#[inline]
fn map_fullwidth_punct(c: char) -> char {
    match c {
        '／' => '/',
        '－' => '-',
        '：' => ':',
        '，' => ',',
        '．' => '.',
        '\u{00A0}' | '\u{2007}' | '\u{2009}' | '\u{202F}' | '\u{3000}' => ' ',
        _ => c,
    }
}

/// Clean invisible Unicode controls, fullwidth ASCII punctuation, and leading/trailing emoji
/// without altering case or stripping NUL bytes. Zero-alloc when no changes are needed.
pub fn clean_unicode_surface(input: &str) -> Cow<'_, str> {
    if input.contains('\0') {
        return Cow::Borrowed(input);
    }
    let keep_zwsp = has_khmer_chars(input);
    let needs_clean = input.bytes().any(|b| b >= 0x80)
        && input.chars().any(|c| {
            is_invisible_format_control(c, keep_zwsp)
                || is_edge_emoji(c)
                || matches!(
                    c,
                    '／' | '－' | '：' | '，' | '．' | '\u{00A0}' | '\u{2007}' | '\u{2009}' | '\u{202F}' | '\u{3000}'
                )
        });
    if !needs_clean {
        return Cow::Borrowed(input);
    }

    let trimmed = input.trim_matches(|c: char| c.is_whitespace() || is_edge_emoji(c));
    let mut out = String::with_capacity(trimmed.len());
    for ch in trimmed.chars() {
        if is_invisible_format_control(ch, keep_zwsp) {
            continue;
        }
        out.push(map_fullwidth_punct(ch));
    }
    Cow::Owned(out)
}

/// Returns `true` if `ch` is an ASCII digit or a Unicode decimal digit across all 76 `Nd` scripts.
#[inline]
pub fn is_unicode_decimal_digit(ch: char) -> bool {
    crate::numerals::is_unicode_decimal_digit(ch)
}

/// Normalize all 76 Unicode `Nd` decimal numeral systems to ASCII 0-9, strip invisible Unicode
/// format controls, normalize fullwidth punctuation, and lowercase cleanly for fast matching.
pub fn normalize_digits_and_lower(input: &str) -> String {
    let keep_zwsp = has_khmer_chars(input);
    let mut out = String::with_capacity(input.len());
    let mut prev_space = true;
    for ch in input.chars() {
        if is_invisible_format_control(ch, keep_zwsp) {
            continue;
        }
        let mapped = match ch {
            // Turkish dotted/dotless I normalization to match lowercase tables cleanly
            '\u{0130}' => 'i',
            other => {
                let nd = crate::numerals::normalize_nd_char(other);
                if nd != other {
                    nd
                } else {
                    map_fullwidth_punct(other)
                }
            }
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
