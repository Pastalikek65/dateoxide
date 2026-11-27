use crate::get_date_data;
use crate::locale::{fold_diacritics, normalize_digits_and_lower, LocaleRegistry};
use crate::settings::{ParsedDateTime, Period, Settings};
use crate::time_spans;
use crate::timezone::finalize_datetime_checked;

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub text: String,
    pub date_obj: ParsedDateTime,
    pub period: Period,
    pub locale: Option<String>,
    pub parts: Vec<&'static str>,
    pub part_of_day: Option<crate::settings::PartOfDay>,
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Linear-time date finder across free-form text (compatible with `dateparser.search.search_dates`).
pub fn search_dates(text: &str, settings: &Settings) -> Vec<(String, ParsedDateTime)> {
    search_dates_full(text, settings)
        .into_iter()
        .map(|m| (m.text, m.date_obj))
        .collect()
}

/// Linear-time date finder across free-form text including the detected language code.
pub fn search_dates_with_locale(
    text: &str,
    settings: &Settings,
) -> Vec<(String, ParsedDateTime, Option<String>)> {
    search_dates_full(text, settings)
        .into_iter()
        .map(|m| (m.text, m.date_obj, m.locale))
        .collect()
}

/// Full `search_dates` returning matched text, datetime, period, and detected locale.
pub fn search_dates_full(text: &str, settings: &Settings) -> Vec<SearchMatch> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let owned_inner;
    let parse_settings: &Settings = if settings.ignore_surrounding_text {
        owned_inner = {
            let mut c = settings.clone();
            c.ignore_surrounding_text = false;
            c
        };
        &owned_inner
    } else {
        settings
    };

    if parse_settings.return_time_span {
        if let Some(span) = time_spans::detect_time_span(text) {
            let base = parse_settings.effective_base();
            let num = span.number.unwrap_or(1);
            if let Some((start_dt, end_dt)) =
                time_spans::generate_time_span(span.span_type, span.direction, num, base, parse_settings)
            {
                let period = match span.span_type {
                    "week" | "weeks" => Period::Week,
                    "month" | "months" => Period::Month,
                    _ => Period::Day,
                };
                if let (Some(s_dt), Some(e_dt)) = (
                    finalize_datetime_checked(start_dt, None, parse_settings),
                    finalize_datetime_checked(end_dt, None, parse_settings),
                ) {
                    return vec![
                        SearchMatch {
                            text: format!("{} (start)", span.matched_text),
                            date_obj: s_dt,
                            period,
                            locale: Some("en".to_string()),
                            parts: vec!["year", "month", "day"],
                            part_of_day: None,
                            start_byte: span.start_pos,
                            end_byte: span.end_pos,
                        },
                        SearchMatch {
                            text: format!("{} (end)", span.matched_text),
                            date_obj: e_dt,
                            period,
                            locale: Some("en".to_string()),
                            parts: vec!["year", "month", "day"],
                            part_of_day: None,
                            start_byte: span.start_pos,
                            end_byte: span.end_pos,
                        },
                    ];
                }
                return Vec::new();
            }
        }
    }

    let registry = LocaleRegistry::global();

    let spans = collect_word_spans(text);
    if spans.is_empty() {
        return Vec::new();
    }

    let explicit_lang_ids: Option<Vec<usize>> = parse_settings.languages.as_ref().map(|codes| {
        codes
            .iter()
            .filter_map(|c| registry.resolve_lang_id(c))
            .collect()
    });

    let mut kinds = Vec::with_capacity(spans.len());
    for &(start, end) in &spans {
        let raw = &text[start..end];
        let trimmed = raw.trim_matches(|c: char| {
            matches!(
                c,
                ',' | '.' | ';' | '!' | '?' | '(' | ')' | '[' | ']' | '"' | '\'' | '«' | '»'
                    | '،' | '؛' | '؟' | '。' | '，' | '、' | '；' | '！' | '？'
            )
        });
        kinds.push(classify_token(
            trimmed,
            registry,
            settings.normalize,
            explicit_lang_ids.as_deref(),
        ));
    }

    let has_custom_formats = settings
        .date_formats
        .as_deref()
        .map(|f| !f.is_empty())
        .unwrap_or(false);

    let mut results = Vec::new();
    let mut i = 0;
    while i < spans.len() {
        if has_custom_formats {
            let max_end = (i + 4).min(spans.len() - 1);
            let mut custom_matched = false;
            for end_idx in (i..=max_end).rev() {
                let raw_sub = text[spans[i].0..spans[end_idx].1]
                    .trim_matches(|c: char| {
                        matches!(c, ',' | ';' | '!' | '?' | '(' | ')' | '[' | ']' | '"' | '\'')
                    })
                    .trim_end_matches('.');
                if raw_sub.is_empty() {
                    continue;
                }
                if let Some(m) = crate::fast_path::try_custom_formats(raw_sub, parse_settings) {
                    if let Some(dt) = finalize_datetime_checked(m.naive, m.tz, parse_settings) {
                        let raw_start = raw_sub.as_ptr() as usize - text.as_ptr() as usize;
                        let raw_end = raw_start + raw_sub.len();
                        results.push(SearchMatch {
                            text: raw_sub.to_string(),
                            date_obj: dt,
                            period: m.period,
                            locale: m.locale,
                            parts: m.parts,
                            part_of_day: None,
                            start_byte: raw_start,
                            end_byte: raw_end,
                        });
                    }
                    i = end_idx + 1;
                    custom_matched = true;
                    break;
                }
            }
            if custom_matched {
                continue;
            }
        }

        if kinds[i] != TokenKind::Signal && kinds[i] != TokenKind::Numeral {
            i += 1;
            continue;
        }
        if kinds[i] == TokenKind::Numeral {
            // Numeral can start a span ONLY when followed soon by a date Signal (prevents "twenty apples" false positives)
            let has_following_signal = ((i + 1)..spans.len().min(i + 5))
                .any(|k| kinds[k] == TokenKind::Signal);
            if !has_following_signal {
                i += 1;
                continue;
            }
        }
        // Do not absorb a standalone leading time-of-day word (e.g. Turkish "Öğle" = noon) into a following ISO date ("2026-05-15")
        if i + 1 < spans.len() {
            let next_raw = &text[spans[i + 1].0..spans[i + 1].1];
            let nb = next_raw.as_bytes();
            let next_is_iso = nb.len() >= 10 && nb[0..4].iter().all(|b| b.is_ascii_digit()) && nb[4] == b'-';
            if next_is_iso {
                let first_lower = normalize_digits_and_lower(
                    text[spans[i].0..spans[i].1].trim_matches(|c: char| !c.is_alphanumeric()),
                );
                let first_folded = fold_diacritics(&first_lower);
                let is_tod_word = registry.locales.iter().any(|l| {
                    l.time_of_day.contains_key(first_lower.as_str())
                        || l.folded_time_of_day.contains_key(first_folded.as_ref())
                });
                let is_date_anchor = registry.exact_phrase_index.contains_key(&first_lower)
                    || registry.folded_exact_phrase_index.contains_key(first_folded.as_ref())
                    || registry.locales.iter().any(|l| {
                        l.months.contains_key(first_lower.as_str())
                            || l.weekdays.contains_key(first_lower.as_str())
                    });
                if is_tod_word && !is_date_anchor {
                    i += 1;
                    continue;
                }
            }
        }

        let first_raw = &text[spans[i].0..spans[i].1];
        let mut seen_year =
            is_complete_standalone_date_token(first_raw) || is_four_digit_year_token(first_raw);
        let mut best_end = i;
        let mut j = i + 1;
        while j < spans.len() && (j - i) < 10 {
            let prev_raw = &text[spans[j - 1].0..spans[j - 1].1];
            let prev_ends_sentence =
                prev_raw.ends_with([';', '!', '?', '。', '！', '？', '؛', '؟', '、', '，', '：'])
                    || (prev_raw.ends_with(':')
                        && !prev_raw[..prev_raw.len() - 1]
                            .bytes()
                            .all(|b| b.is_ascii_digit()))
                    || (prev_raw.ends_with('.')
                        && !prev_raw[..prev_raw.len() - 1]
                            .bytes()
                            .all(|b| b.is_ascii_digit())
                        && prev_raw.len() > 4);
            if prev_ends_sentence {
                break;
            }

            let tok_j = &text[spans[j].0..spans[j].1];
            if is_complete_standalone_date_token(tok_j) {
                break;
            }
            if tok_j.eq_ignore_ascii_case(prev_raw) && !tok_j.bytes().any(|b| b.is_ascii_digit()) {
                break;
            }
            if seen_year {
                let tok_j_year = is_four_digit_year_token(tok_j);
                let is_time_or_offset = tok_j.contains(':')
                    || kinds[j] == TokenKind::Tz
                    || (tok_j.starts_with(['+', '-'])
                        && tok_j[1..].bytes().all(|b| b.is_ascii_digit() || b == b':'))
                    || matches!(
                        tok_j.trim_end_matches('.').to_ascii_lowercase().as_str(),
                        "am" | "pm" | "a.m" | "p.m" | "at" | "um" | "à" | "saat"
                    );
                if tok_j_year || (!is_time_or_offset && ((j - i) >= 3 || tok_j.contains('-'))) {
                    break;
                }
            }

            match kinds[j] {
                TokenKind::Signal | TokenKind::Tz => {
                    if is_four_digit_year_token(tok_j) {
                        seen_year = true;
                    }
                    best_end = j;
                    j += 1;
                }
                TokenKind::Numeral => {
                    if is_four_digit_year_token(tok_j) {
                        seen_year = true;
                    }
                    best_end = j;
                    j += 1;
                }
                TokenKind::Connector => {
                    if j + 1 < spans.len()
                        && matches!(kinds[j + 1], TokenKind::Signal | TokenKind::Numeral | TokenKind::Tz)
                    {
                        j += 1;
                    } else if j + 2 < spans.len()
                        && kinds[j + 1] == TokenKind::Connector
                        && matches!(kinds[j + 2], TokenKind::Signal | TokenKind::Numeral | TokenKind::Tz)
                    {
                        j += 2;
                    } else {
                        break;
                    }
                }
                TokenKind::Other => break,
            }
        }

        if !((i..=best_end).any(|k| kinds[k] == TokenKind::Signal)) {
            i += 1;
            continue;
        }

        let mut matched = false;
        for end_idx in (i..=best_end).rev() {
            let last_tok_lower = normalize_digits_and_lower(
                text[spans[end_idx].0..spans[end_idx].1].trim_matches(|c: char| {
                    matches!(
                        c,
                        ',' | '.' | ';' | '!' | '?' | '(' | ')' | '،' | '؛' | '؟' | '。' | '，'
                            | '、' | '；' | '！' | '？'
                    )
                }),
            );
            if matches!(
                last_tok_lower.as_str(),
                "in" | "at" | "on" | "of" | "the" | "de" | "del" | "el" | "la" | "le" | "à"
                    | "am" | "um" | "and" | "ve" | "und" | "et" | "for" | "to" | "a" | "an"
                    | "vor" | "hace" | "dans" | "en" | "saat" | "las" | "los" | "ora" | "às"
                    | "as" | "al" | "alle" | "kl" | "klo" | "o" | "v" | "u" | "в" | "о"
                    | "الساعة" | "في"
            ) {
                continue;
            }

            let sub_start = spans[i].0;
            let sub_end = spans[end_idx].1;
            let raw_sub = text[sub_start..sub_end].trim_matches(|c: char| {
                matches!(
                    c,
                    ',' | ';' | '!' | '?' | '(' | ')' | '[' | ']' | '"' | '،' | '؛' | '؟'
                        | '。' | '，' | '、' | '；' | '！' | '？'
                )
            });
            let raw_sub = if raw_sub.starts_with('\'') && raw_sub.ends_with('\'') && raw_sub.len() >= 2 {
                &raw_sub[1..raw_sub.len() - 1]
            } else {
                raw_sub
            };
            let raw_sub = raw_sub.trim_end_matches('.');

            if raw_sub.bytes().all(|b| b.is_ascii_digit()) && raw_sub.len() < 4 {
                continue;
            }
            if settings.languages.is_none()
                && !raw_sub.is_empty()
                && raw_sub.bytes().all(|b| b.is_ascii_digit() || b == b'.' || b == b',')
            {
                if end_idx + 1 < spans.len() {
                    let next_raw = text[spans[end_idx + 1].0..spans[end_idx + 1].1]
                        .trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '!' | '?' | '(' | ')'));
                    if crate::locale::is_iso4217_currency(next_raw) {
                        continue;
                    }
                }
                if i > 0 {
                    let prev_raw = text[spans[i - 1].0..spans[i - 1].1]
                        .trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '!' | '?' | '(' | ')'));
                    if crate::locale::is_iso4217_currency(prev_raw) {
                        continue;
                    }
                }
            }
            if is_version_like_dotted(raw_sub) {
                continue;
            }
            if end_idx == i {
                let cjk_subs = extract_cjk_date_subspans(raw_sub);
                if !cjk_subs.is_empty() {
                    let mut any_cjk = false;
                    for sub in cjk_subs {
                        let dd = get_date_data(sub, parse_settings);
                        if let Some(dt) = dd.date_obj {
                            let sb = sub.as_ptr() as usize - text.as_ptr() as usize;
                            let eb = sb + sub.len();
                            results.push(SearchMatch {
                                text: sub.to_string(),
                                date_obj: dt,
                                period: dd.period,
                                locale: dd.locale,
                                parts: dd.parts,
                                part_of_day: dd.part_of_day,
                                start_byte: sb,
                                end_byte: eb,
                            });
                            any_cjk = true;
                        }
                    }
                    if any_cjk {
                        i = end_idx + 1;
                        matched = true;
                        break;
                    }
                }
                let se_subs = extract_se_asian_date_subspans(raw_sub, registry);
                if !se_subs.is_empty() {
                    let mut any_se = false;
                    for sub in se_subs {
                        let dd = get_date_data(sub, parse_settings);
                        if let Some(dt) = dd.date_obj {
                            let sb = sub.as_ptr() as usize - text.as_ptr() as usize;
                            let eb = sb + sub.len();
                            results.push(SearchMatch {
                                text: sub.to_string(),
                                date_obj: dt,
                                period: dd.period,
                                locale: dd.locale,
                                parts: dd.parts,
                                part_of_day: dd.part_of_day,
                                start_byte: sb,
                                end_byte: eb,
                            });
                            any_se = true;
                        }
                    }
                    if any_se {
                        i = end_idx + 1;
                        matched = true;
                        break;
                    }
                }
                if i > 0
                    && raw_sub.len() == 4
                    && raw_sub.bytes().all(|b| b.is_ascii_digit())
                    && is_identifier_or_code_prefix(&text[spans[i - 1].0..spans[i - 1].1])
                {
                    continue;
                }
                if !is_valid_single_token_date(raw_sub, registry, parse_settings.normalize) {
                    continue;
                }
            }

            let dd = get_date_data(raw_sub, parse_settings);
            if let Some(dt) = dd.date_obj {
                let has_digits = raw_sub.chars().any(crate::numerals::is_unicode_decimal_digit);
                if parse_settings.languages.is_none()
                    && !has_digits
                    && should_skip_ambiguous_phrase(
                        raw_sub,
                        end_idx == i,
                        dd.locale.as_deref(),
                        dd.period,
                    )
                {
                    continue;
                }
                let raw_start = raw_sub.as_ptr() as usize - text.as_ptr() as usize;
                let raw_end = raw_start + raw_sub.len();
                results.push(SearchMatch {
                    text: raw_sub.to_string(),
                    date_obj: dt,
                    period: dd.period,
                    locale: dd.locale,
                    parts: dd.parts,
                    part_of_day: dd.part_of_day,
                    start_byte: raw_start,
                    end_byte: raw_end,
                });
                i = end_idx + 1;
                matched = true;
                break;
            }
        }

        if !matched {
            i += 1;
        }
    }

    results
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Signal,
    Numeral,
    Connector,
    Tz,
    Other,
}

fn is_complete_standalone_date_token(tok: &str) -> bool {
    let t = tok.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '!' | '?' | '(' | ')' | '[' | ']'));
    let b = t.as_bytes();
    if b.len() >= 10 && b[0..4].iter().all(|c| c.is_ascii_digit()) && (b[4] == b'-' || b[4] == b'/') {
        return true;
    }
    if t.contains('年') && t.contains('月') && t.contains('日') {
        return true;
    }
    false
}

fn is_four_digit_year_token(tok: &str) -> bool {
    let t = tok.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '!' | '?' | '(' | ')' | '[' | ']'));
    if t.len() == 4 && t.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(y) = t.parse::<u32>() {
            return (1900..=2100).contains(&y);
        }
    }
    false
}

fn is_identifier_or_code_prefix(prev_tok: &str) -> bool {
    if prev_tok.ends_with([',', ';', ':', '.', '!', '?', '(', '[', '-', '/']) {
        return false;
    }
    let clean = prev_tok.trim_start_matches(['(', '[', '"', '\'']);
    if (2..=4).contains(&clean.len())
        && clean.bytes().all(|b| b.is_ascii_uppercase())
        && !matches!(
            clean,
            "JAN" | "FEB" | "MAR" | "APR" | "MAY" | "JUN" | "JUL" | "AUG" | "SEP" | "OCT"
                | "NOV" | "DEC" | "MON" | "TUE" | "WED" | "THU" | "FRI" | "SAT" | "SUN"
                | "FY" | "AD" | "BC" | "CE" | "Q1" | "Q2" | "Q3" | "Q4"
                | "IN" | "ON" | "OF" | "AT" | "BY" | "TO"
        )
    {
        return true;
    }
    matches!(
        clean.to_ascii_lowercase().as_str(),
        "error" | "err" | "pid" | "port" | "code" | "issue" | "bug" | "ticket"
            | "id" | "ref" | "flight" | "order" | "invoice" | "model" | "version" | "ver"
    )
}

fn collect_word_spans(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, ch) in s.char_indices() {
        if ch.is_whitespace() {
            if let Some(st) = start.take() {
                out.push((st, idx));
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }
    if let Some(st) = start {
        out.push((st, s.len()));
    }
    out
}

fn classify_token_explicit(
    lower: &str,
    lower_untrimmed: &str,
    registry: &LocaleRegistry,
    normalize: bool,
    explicit_ids: &[usize],
) -> Option<TokenKind> {
    let folded_cow = if normalize {
        Some(fold_diacritics(lower))
    } else {
        None
    };
    let folded = folded_cow.as_deref();

    let is_article_connector = matches!(
        lower,
        "el" | "la" | "le" | "de" | "del" | "al" | "the" | "of" | "on" | "at" | "o" | "в" | "о"
    );
    for &lid in explicit_ids {
        let loc = &registry.locales[lid];
        for cand in [Some(lower_untrimmed), Some(lower), folded].into_iter().flatten() {
            if loc.months.contains_key(cand)
                || loc.folded_months.contains_key(cand)
                || loc.weekdays.contains_key(cand)
                || loc.folded_weekdays.contains_key(cand)
                || loc.units.contains_key(cand)
                || loc.folded_units.contains_key(cand)
                || loc.ago.contains(cand)
                || loc.folded_ago.contains(cand)
                || loc.in_words.contains(cand)
                || loc.folded_in_words.contains(cand)
                || loc.time_of_day.contains_key(cand)
                || loc.folded_time_of_day.contains_key(cand)
                || loc.am.contains(cand)
                || loc.folded_am.contains(cand)
                || loc.pm.contains(cand)
                || loc.folded_pm.contains(cand)
                || (!is_article_connector
                    && (loc
                        .relative_exact
                        .keys()
                        .any(|p| p.split_whitespace().any(|w| w == cand))
                        || loc.relative_affix.iter().any(|a| {
                            a.prefix.split_whitespace().any(|w| w == cand)
                                || a.suffix.split_whitespace().any(|w| w == cand)
                        })))
            {
                return Some(TokenKind::Signal);
            }
        }
    }
    for &lid in explicit_ids {
        let loc = &registry.locales[lid];
        for cand in [Some(lower), folded].into_iter().flatten() {
            if loc.word_numbers.contains_key(cand)
                || loc.ordinals.contains_key(cand)
                || loc.folded_ordinals.contains_key(cand)
            {
                return Some(TokenKind::Numeral);
            }
        }
    }
    None
}

fn classify_token(
    tok: &str,
    registry: &LocaleRegistry,
    normalize: bool,
    explicit_ids: Option<&[usize]>,
) -> TokenKind {
    if tok.is_empty() {
        return TokenKind::Other;
    }

    if tok.chars().any(|c| {
        crate::numerals::is_unicode_decimal_digit(c)
            || crate::numerals::is_cjk_numeral_char(c)
            || matches!(
                c,
                '年' | '月' | '日' | '년' | '월' | '일' | '天' | '前' | '後' | '后' | '週' | '周'
            )
            || crate::numerals::is_geez_numeral_char(c)
            || matches!(c, '௰' | '௱' | '௲' | '൰' | '൱' | '൲')
    }) {
        return TokenKind::Signal;
    }

    let lower_untrimmed = normalize_digits_and_lower(tok);
    let trimmed_tok = tok.trim_matches(|c: char| matches!(c, '\'' | '’' | 'ꞌ' | 'ʻ' | ',' | '.'));
    let lower = normalize_digits_and_lower(if trimmed_tok.is_empty() { tok } else { trimmed_tok });

    if let Some(ids) = explicit_ids {
        if let Some(kind) = classify_token_explicit(&lower, &lower_untrimmed, registry, normalize, ids) {
            return kind;
        }
    }

    if explicit_ids.is_none()
        && matches!(
            lower.as_str(),
            "or" | "if" | "so" | "as" | "by" | "is" | "are" | "was" | "were" | "be" | "to"
                | "for" | "from" | "not" | "but" | "with" | "us" | "we" | "me" | "my" | "it"
                | "its" | "let" | "ok"
        )
    {
        return TokenKind::Other;
    }

    if lower == "uhr" {
        return TokenKind::Signal;
    }

    if matches!(
        lower.as_str(),
        "of" | "the" | "at" | "on" | "de" | "del" | "el" | "la" | "le" | "à" | "am" | "um"
            | "saat" | "and" | "ve" | "und" | "et" | "il" | "y" | "a" | "an" | "in" | "en"
            | "dans" | "vor" | "hace" | "las" | "los" | "ora" | "às" | "as" | "al" | "alle"
            | "kl" | "klo" | "o" | "v" | "u" | "в" | "о" | "р" | "г" | "ч" | "الساعة" | "في"
            | "lúc" | "pukul" | "στις" | "เวลา" | "น" | "ເວລາ"
    ) {
        if matches!(
            lower.as_str(),
            "in" | "vor" | "hace" | "dans" | "il" | "a" | "an" | "en"
        ) {
            return TokenKind::Signal;
        }
        return TokenKind::Connector;
    }

    if registry.known_date_tokens.contains(&lower_untrimmed) {
        return TokenKind::Signal;
    }

    if crate::numerals::parse_date_number_or_ordinal(&lower).is_some() {
        return TokenKind::Signal;
    }

    if registry.known_date_tokens.contains(&lower) {
        return TokenKind::Signal;
    }
    if registry.known_numeral_tokens.contains(&lower) {
        return TokenKind::Numeral;
    }
    if lower.contains('-') {
        let hparts: Vec<&str> = lower.split('-').filter(|p| !p.is_empty()).collect();
        if !hparts.is_empty() && hparts.iter().any(|p| registry.known_date_tokens.contains(*p)) {
            return TokenKind::Signal;
        }
        if !hparts.is_empty() && hparts.iter().all(|p| registry.known_numeral_tokens.contains(*p)) {
            return TokenKind::Numeral;
        }
    }
    if normalize {
        let folded = fold_diacritics(&lower);
        if registry.known_date_tokens.contains(folded.as_ref()) {
            return TokenKind::Signal;
        }
        if registry.known_numeral_tokens.contains(folded.as_ref()) {
            return TokenKind::Numeral;
        }
        if folded.contains('-')
            && folded
                .split('-')
                .filter(|p| !p.is_empty())
                .all(|p| registry.known_numeral_tokens.contains(p))
        {
            return TokenKind::Numeral;
        }
    }
    if !lower.is_ascii()
        && registry
            .non_ascii_numeral_words
            .iter()
            .any(|w| lower.starts_with(w.as_str()))
    {
        return TokenKind::Numeral;
    }
    if let Some((l, r)) = lower.split_once("und") {
        if !l.is_empty()
            && !r.is_empty()
            && (registry.known_numeral_tokens.contains(l) || l == "ein")
            && registry.known_numeral_tokens.contains(r)
        {
            return TokenKind::Numeral;
        }
    }

    if tok.len() >= 3
        && tok.len() <= 5
        && tok.bytes().all(|b| b.is_ascii_uppercase())
        && crate::timezone::parse_tz_spec(tok).is_some()
    {
        return TokenKind::Tz;
    }

    let trimmed_raw = tok.trim_matches(|c: char| matches!(c, ',' | '.' | '(' | ')' | '[' | ']'));
    if trimmed_raw.len() >= 2
        && trimmed_raw.len() <= 15
        && trimmed_raw.bytes().all(|b| matches!(b, b'I' | b'V' | b'X' | b'L' | b'C' | b'D' | b'M'))
    {
        if let Some(rv) = crate::numerals::parse_roman_numeral(trimmed_raw) {
            if (1..=31).contains(&rv) || (1000..=2999).contains(&rv) {
                return TokenKind::Numeral;
            }
        }
    }

    if !tok.is_ascii()
        && tok.chars().any(is_se_asian_char)
        && (tok.contains("ที่แล้ว") || tok.contains("ที่ผ่านมา") || !extract_se_asian_date_subspans(tok, registry).is_empty())
    {
        return TokenKind::Signal;
    }

    if matches!(
        lower.as_str(),
        "last" | "next" | "this" | "geçen" | "gecen" | "gelecek" | "bu" | "bir" | "iki" | "üç"
            | "one" | "two" | "three" | "four" | "five" | "zwei" | "drei" | "ein" | "une"
            | "deux" | "tonight" | "anoche" | "noon" | "midnight"
    ) {
        return TokenKind::Signal;
    }

    TokenKind::Other
}

fn is_cjk_date_starter(s: &str) -> Option<usize> {
    for era in ["令和", "平成", "昭和", "大正", "明治", "民國", "民国"] {
        if let Some(after) = s.strip_prefix(era) {
            if after.starts_with('元')
                || after
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit() || ('０'..='９').contains(&c))
                    .unwrap_or(false)
            {
                return Some(era.len());
            }
        }
    }
    None
}

/// Extract clean date/time subspans from spaceless CJK prose sentences
/// (e.g. "締め切りは2026年11月26日です。" -> ["2026年11月26日"], "3日前に通知" -> ["3日前"]).
fn extract_cjk_date_subspans(s: &str) -> Vec<&str> {
    if !s.contains([
        '年', '月', '日', '号', '년', '월', '일', '天', '前', '後', '后', '週', '周', '点',
        '點', '時', '时', '－', '／', '-', '/',
    ]) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < s.len() {
        let rest = &s[pos..];
        let start_advance = if let Some(era_len) = is_cjk_date_starter(rest) {
            Some(era_len)
        } else if let Some(ch) = rest.chars().next() {
            if crate::numerals::is_unicode_decimal_digit(ch)
                || crate::numerals::is_cjk_numeral_char(ch)
            {
                Some(ch.len_utf8())
            } else {
                None
            }
        } else {
            None
        };

        let Some(adv) = start_advance else {
            let ch = rest.chars().next().unwrap();
            pos += ch.len_utf8();
            continue;
        };

        let start = pos;
        let mut cur = pos + adv;
        let mut saw_date_unit = false;
        let mut sep_count = 0usize;
        while cur < s.len() {
            let rem = &s[cur..];
            let mut advanced_marker = false;
            for marker in ["午前", "午後", "上午", "下午", "오전", "오후"] {
                if rem.starts_with(marker) {
                    cur += marker.len();
                    advanced_marker = true;
                    break;
                }
            }
            if advanced_marker {
                continue;
            }
            let mut advanced_rel_unit = false;
            for unit in [
                "時間", "小时", "小時", "分間", "分钟", "分鐘", "週間", "星期", "个月", "個月",
                "か月", "ヶ月",
            ] {
                if rem.starts_with(unit) {
                    saw_date_unit = true;
                    cur += unit.len();
                    advanced_rel_unit = true;
                    break;
                }
            }
            if advanced_rel_unit {
                continue;
            }
            let ch = rem.chars().next().unwrap();
            if crate::numerals::is_unicode_decimal_digit(ch)
                || crate::numerals::is_cjk_numeral_char(ch)
                || matches!(
                    ch,
                    '元' | '時' | '时' | '点' | '點' | '分' | '秒' | '시' | '분' | '초' | ':' | '：'
                )
            {
                cur += ch.len_utf8();
            } else if matches!(ch, '-' | '－' | '/' | '／') {
                let after = &rem[ch.len_utf8()..];
                if after
                    .chars()
                    .next()
                    .is_some_and(crate::locale::is_unicode_decimal_digit)
                {
                    sep_count += 1;
                    cur += ch.len_utf8();
                } else {
                    break;
                }
            } else if matches!(
                ch,
                '年' | '月' | '日' | '号' | '년' | '월' | '일' | '天' | '週' | '周'
            ) {
                saw_date_unit = true;
                cur += ch.len_utf8();
                // Optional parenthesized CJK weekday right after 日/일, e.g. 2026年11月26日(木)
                if matches!(ch, '日' | '일') {
                    let after_day = &s[cur..];
                    if let Some(after_open) = after_day.strip_prefix('(') {
                        if let Some(wd_ch) = after_open.chars().next() {
                            let after_wd = &after_open[wd_ch.len_utf8()..];
                            if after_wd.starts_with(')') {
                                cur += 1 + wd_ch.len_utf8() + 1;
                            }
                        }
                    }
                }
            } else if matches!(ch, '前' | '後' | '后') && saw_date_unit {
                cur += ch.len_utf8();
                break;
            } else {
                break;
            }
        }

        let cand = s[start..cur].trim_end_matches([':', '：', '-', '－', '/', '／']);
        if (saw_date_unit || sep_count >= 2)
            && (cand.chars().any(crate::numerals::is_unicode_decimal_digit)
                || cand.chars().any(crate::numerals::is_cjk_numeral_char)
                || cand.contains('元'))
        {
            // Whole token is already a single CJK/ISO date: return empty so single-token parser handles it intact
            if start == 0 && cand.len() == s.len() {
                return Vec::new();
            }
            if cand.len() < s.len() {
                out.push(cand);
                pos = cur;
                continue;
            }
        }
        // Advance past non-date digit runs to avoid O(N^2) re-scanning
        pos = cur.max(start + adv);
    }
    out
}

#[inline]
fn is_se_asian_char(ch: char) -> bool {
    matches!(ch as u32, 0x0E00..=0x0E7F | 0x0E80..=0x0EFF | 0x1000..=0x109F | 0x1780..=0x17FF)
}

/// Extract spaceless date/relative subspans containing a month name or Thai relative unit inside Thai/Khmer/Lao/Myanmar prose.
fn extract_se_asian_date_subspans<'a>(s: &'a str, registry: &LocaleRegistry) -> Vec<&'a str> {
    let has_thai = s.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    if s.is_ascii()
        || !s.chars().any(is_se_asian_char)
        || (!s.chars().any(crate::numerals::is_unicode_decimal_digit) && !has_thai)
    {
        return Vec::new();
    }
    #[derive(Clone, Copy)]
    enum HitState {
        Unsearched,
        Found(usize),
        Exhausted,
    }
    let mut patterns: Vec<(&str, HitState)> = Vec::new();
    for code in ["th", "km", "lo", "my"] {
        if let Some(lid) = registry.resolve_lang_id(code) {
            for month_name in registry.locales[lid].months.keys() {
                if !month_name.is_ascii() && month_name.chars().count() >= 2 {
                    patterns.push((month_name.as_str(), HitState::Unsearched));
                }
            }
        }
    }

    let mut out = Vec::new();
    if !patterns.is_empty() && s.chars().any(crate::numerals::is_unicode_decimal_digit) {
        let mut scan_from = 0usize;
        while scan_from < s.len() {
            let mut best_match: Option<(usize, usize)> = None;
            for (pat, state) in &mut patterns {
                let abs_start = match *state {
                    HitState::Exhausted => continue,
                    HitState::Found(p) if p >= scan_from => p,
                    HitState::Unsearched | HitState::Found(_) => {
                        if let Some(rel) = s[scan_from..].find(*pat) {
                            let p = scan_from + rel;
                            *state = HitState::Found(p);
                            p
                        } else {
                            *state = HitState::Exhausted;
                            continue;
                        }
                    }
                };
                let abs_end = abs_start + pat.len();
                if best_match
                    .is_none_or(|(bs, be)| abs_start < bs || (abs_start == bs && abs_end > be))
                {
                    best_match = Some((abs_start, abs_end));
                }
            }
            let Some((m_start, m_end)) = best_match else {
                break;
            };
            // Expand left over whitespace and up to 2 digits (day)
            let mut span_start = m_start;
            let mut digit_left = 0usize;
            for (idx, ch) in s[..m_start].char_indices().rev() {
                if crate::numerals::is_unicode_decimal_digit(ch) && digit_left < 4 {
                    span_start = idx;
                    digit_left += 1;
                } else if ch.is_whitespace() && digit_left == 0 {
                    span_start = idx;
                } else {
                    break;
                }
            }
            // Expand right over whitespace and up to 4 digits (year)
            let mut span_end = m_end;
            let mut digit_right = 0usize;
            for (rel_idx, ch) in s[m_end..].char_indices() {
                if crate::numerals::is_unicode_decimal_digit(ch) && digit_right < 4 {
                    span_end = m_end + rel_idx + ch.len_utf8();
                    digit_right += 1;
                } else if ch.is_whitespace() && digit_right == 0 {
                    span_end = m_end + rel_idx + ch.len_utf8();
                } else {
                    break;
                }
            }
            if digit_left > 0 || digit_right > 0 {
                let cand = s[span_start..span_end].trim();
                if !cand.is_empty() && cand.len() < s.len() {
                    out.push(cand);
                    scan_from = span_end.max(m_end);
                    continue;
                }
            }
            scan_from = m_end;
        }
    }

    if out.is_empty() && has_thai {
        if let Some(th_id) = registry.resolve_lang_id("th") {
            let th = &registry.locales[th_id];
            for sfx in [
                "วันที่ผ่านมา",
                "วันที่แล้ว",
                "ชั่วโมงที่ผ่านมา",
                "ชั่วโมงที่แล้ว",
                "นาทีที่ผ่านมา",
                "นาทีที่แล้ว",
                "เดือนที่ผ่านมา",
                "เดือนที่แล้ว",
                "วินาทีที่ผ่านมา",
                "วินาทีที่แล้ว",
                "สัปดาห์ที่ผ่านมา",
                "สัปดาห์ที่แล้ว",
                "ปีที่แล้ว",
            ] {
                if let Some(pos) = s.find(sfx) {
                    let prefix = &s[..pos];
                    let trimmed_p = prefix.trim_end();
                    // Check digits or Thai word number immediately before the unit+ago suffix
                    let mut span_start = None;
                    let mut d_start = trimmed_p.len();
                    for (idx, ch) in trimmed_p.char_indices().rev() {
                        if crate::numerals::is_unicode_decimal_digit(ch) {
                            d_start = idx;
                        } else {
                            break;
                        }
                    }
                    if d_start < trimmed_p.len() {
                        span_start = Some(d_start);
                    } else {
                        let mut best_w_len = 0usize;
                        for w in th.word_numbers.keys() {
                            if !w.is_ascii() && trimmed_p.ends_with(w.as_str()) && w.len() > best_w_len {
                                best_w_len = w.len();
                                span_start = Some(trimmed_p.len() - w.len());
                            }
                        }
                    }
                    if let Some(mut st) = span_start {
                        if let Some(b_start) = s[..st].strip_suffix("เมื่อ") {
                            st = b_start.len();
                        }
                        let cand = &s[st..pos + sfx.len()];
                        if !cand.is_empty() && cand.len() < s.len() {
                            out.push(cand);
                            break;
                        }
                    }
                }
            }
        }
    }
    out
}

fn is_valid_single_token_date(tok: &str, registry: &LocaleRegistry, normalize: bool) -> bool {
    if tok.contains([
        '-', '/', '－', '／', '.', ':', '：', '年', '月', '日', '년', '월', '일', '天', '前', '後',
        '后', '週', '周', '点', '點', '時', '时',
    ]) && tok.chars().any(|c| {
        crate::numerals::is_unicode_decimal_digit(c) || crate::numerals::is_cjk_numeral_char(c)
    }) {
        return true;
    }
    // Single spaceless Thai token starting with a numeral word or digit (e.g. "ห้านาทีที่แล้ว", "เมื่อ10นาทีที่แล้ว")
    if let Some(th_id) = registry.resolve_lang_id("th") {
        let th = &registry.locales[th_id];
        let th_body = tok.strip_prefix("เมื่อ").unwrap_or(tok);
        if !tok.is_ascii()
            && (th.word_numbers.keys().any(|w| !w.is_ascii() && th_body.starts_with(w.as_str()))
                || th_body
                    .chars()
                    .next()
                    .is_some_and(crate::numerals::is_unicode_decimal_digit))
            && th.units.keys().any(|u| !u.is_ascii() && th_body.contains(u.as_str()))
        {
            return true;
        }
    }
    if tok.len() == 4 && tok.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(y) = tok.parse::<u32>() {
            return (1900..=2100).contains(&y);
        }
    }
    let lower = normalize_digits_and_lower(tok);
    if registry.exact_phrase_index.contains_key(&lower)
        || matches!(lower.as_str(), "tonight" | "anoche")
    {
        return true;
    }
    if normalize {
        let folded = fold_diacritics(&lower);
        if registry.folded_exact_phrase_index.contains_key(folded.as_ref()) {
            return true;
        }
    }
    false
}

fn is_version_like_dotted(s: &str) -> bool {
    if !s.contains('.') || s.contains([' ', '-', '/', ':']) {
        return false;
    }
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() == 3
        && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
    {
        // Reject semantic versions like "3.12.1" or "0.1.0" where the last segment is 1 digit
        // and no segment is a 4-digit year.
        let has_4digit = parts.iter().any(|p| p.len() == 4);
        if !has_4digit && parts[2].len() == 1 {
            return true;
        }
    }
    false
}

fn should_skip_ambiguous_phrase(
    tok: &str,
    is_single_token: bool,
    locale: Option<&str>,
    period: Period,
) -> bool {
    if !tok.is_ascii() {
        return false;
    }
    if is_single_token {
        let lower = tok.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "let" | "set" | "get" | "put" | "can" | "has" | "was" | "are" | "her" | "his"
                | "she" | "out" | "man" | "see" | "did" | "say" | "use" | "way" | "new" | "old"
                | "for" | "not" | "but" | "you" | "all" | "one" | "two" | "who" | "how" | "why"
                | "its" | "him" | "our" | "run" | "sit" | "try" | "ask" | "own" | "big" | "end"
                | "far" | "off" | "yet" | "per" | "via" | "ok" | "no"
        ) {
            return true;
        }
    }
    if let Some(loc) = locale {
        if !crate::locale::is_major_auto_locale(loc) && (is_single_token || tok.len() <= 12) {
            return true;
        }
        if is_single_token && loc != "en" && period != Period::Day && period != Period::Time && tok.len() <= 3 {
            return true;
        }
    }
    false
}
