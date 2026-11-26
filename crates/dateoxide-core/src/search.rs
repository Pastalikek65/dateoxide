use crate::get_date_data;
use crate::locale::{fold_diacritics, normalize_digits_and_lower, LocaleRegistry};
use crate::settings::{ParsedDateTime, Period, Settings};
use crate::time_spans;
use crate::timezone::finalize_datetime;

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub text: String,
    pub date_obj: ParsedDateTime,
    pub period: Period,
    pub locale: Option<String>,
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

    if settings.return_time_span {
        if let Some(span) = time_spans::detect_time_span(text) {
            let base = settings.effective_base();
            let num = span.number.unwrap_or(1);
            if let Some((start_dt, end_dt)) =
                time_spans::generate_time_span(span.span_type, span.direction, num, base, settings)
            {
                let period = match span.span_type {
                    "week" | "weeks" => Period::Week,
                    "month" | "months" => Period::Month,
                    _ => Period::Day,
                };
                return vec![
                    SearchMatch {
                        text: format!("{} (start)", span.matched_text),
                        date_obj: finalize_datetime(start_dt, None, settings),
                        period,
                        locale: Some("en".to_string()),
                    },
                    SearchMatch {
                        text: format!("{} (end)", span.matched_text),
                        date_obj: finalize_datetime(end_dt, None, settings),
                        period,
                        locale: Some("en".to_string()),
                    },
                ];
            }
        }
    }

    let registry = LocaleRegistry::global();

    let spans = collect_word_spans(text);
    if spans.is_empty() {
        return Vec::new();
    }

    let mut kinds = Vec::with_capacity(spans.len());
    for &(start, end) in &spans {
        let raw = &text[start..end];
        let trimmed = raw.trim_matches(|c: char| {
            matches!(
                c,
                ',' | '.' | ';' | '!' | '?' | '(' | ')' | '[' | ']' | '"' | '\'' | '«' | '»'
            )
        });
        kinds.push(classify_token(trimmed, registry, settings.normalize));
    }

    let mut results = Vec::new();
    let mut i = 0;
    while i < spans.len() {
        if kinds[i] != TokenKind::Signal {
            i += 1;
            continue;
        }

        let mut best_end = i;
        let mut j = i + 1;
        while j < spans.len() && (j - i) < 8 {
            let prev_raw = &text[spans[j - 1].0..spans[j - 1].1];
            let prev_ends_sentence = prev_raw.ends_with([';', '!', '?'])
                || (prev_raw.ends_with('.')
                    && !prev_raw[..prev_raw.len() - 1]
                        .bytes()
                        .all(|b| b.is_ascii_digit())
                    && prev_raw.len() > 4);
            if prev_ends_sentence {
                break;
            }

            match kinds[j] {
                TokenKind::Signal => {
                    best_end = j;
                    j += 1;
                }
                TokenKind::Connector => {
                    if j + 1 < spans.len() && kinds[j + 1] == TokenKind::Signal {
                        j += 1;
                    } else {
                        break;
                    }
                }
                TokenKind::Other => break,
            }
        }

        let mut matched = false;
        for end_idx in (i..=best_end).rev() {
            let last_tok_lower = normalize_digits_and_lower(
                text[spans[end_idx].0..spans[end_idx].1]
                    .trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '!' | '?' | '(' | ')')),
            );
            if matches!(
                last_tok_lower.as_str(),
                "in" | "at" | "on" | "of" | "the" | "de" | "del" | "el" | "la" | "le" | "à"
                    | "am" | "um" | "and" | "ve" | "und" | "et" | "for" | "to" | "a" | "an"
                    | "vor" | "hace" | "dans" | "en" | "saat"
            ) {
                continue;
            }

            let sub_start = spans[i].0;
            let sub_end = spans[end_idx].1;
            let raw_sub = text[sub_start..sub_end].trim_matches(|c: char| {
                matches!(
                    c,
                    ',' | ';' | '!' | '?' | '(' | ')' | '[' | ']' | '"' | '\''
                )
            });
            let raw_sub = raw_sub.trim_end_matches('.');

            if raw_sub.bytes().all(|b| b.is_ascii_digit()) && raw_sub.len() < 4 {
                continue;
            }
            if is_version_like_dotted(raw_sub) {
                continue;
            }
            if end_idx == i && !is_valid_single_token_date(raw_sub, registry, settings.normalize) {
                continue;
            }

            let dd = get_date_data(raw_sub, settings);
            if let Some(dt) = dd.date_obj {
                let has_digits = raw_sub
                    .chars()
                    .any(|c| c.is_ascii_digit() || ('\u{0660}'..='\u{0669}').contains(&c));
                if settings.languages.is_none()
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
                results.push(SearchMatch {
                    text: raw_sub.to_string(),
                    date_obj: dt,
                    period: dd.period,
                    locale: dd.locale,
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
    Connector,
    Other,
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

fn classify_token(tok: &str, registry: &LocaleRegistry, normalize: bool) -> TokenKind {
    if tok.is_empty() {
        return TokenKind::Other;
    }

    if tok.chars().any(|c| {
        c.is_ascii_digit()
            || matches!(
                c,
                '\u{0660}'..='\u{0669}' | '年' | '月' | '日' | '년' | '월' | '일'
            )
    }) {
        return TokenKind::Signal;
    }

    let lower = normalize_digits_and_lower(tok);

    if matches!(
        lower.as_str(),
        "of" | "the" | "at" | "on" | "de" | "del" | "el" | "la" | "le" | "à" | "am" | "um"
            | "saat" | "and" | "ve" | "und" | "et" | "il" | "y" | "a" | "an" | "in" | "en"
            | "dans" | "vor" | "hace"
    ) {
        if matches!(
            lower.as_str(),
            "in" | "vor" | "hace" | "dans" | "il" | "a" | "an" | "en"
        ) {
            return TokenKind::Signal;
        }
        return TokenKind::Connector;
    }

    if registry.known_date_tokens.contains(&lower) {
        return TokenKind::Signal;
    }
    if normalize {
        let folded = fold_diacritics(&lower);
        if registry.known_date_tokens.contains(&folded) {
            return TokenKind::Signal;
        }
    }

    if tok.len() >= 3
        && tok.len() <= 5
        && tok.bytes().all(|b| b.is_ascii_uppercase())
        && crate::timezone::parse_tz_spec(tok).is_some()
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

fn is_valid_single_token_date(tok: &str, registry: &LocaleRegistry, normalize: bool) -> bool {
    if tok.contains(['-', '/', '.', ':', '年', '月', '日', '년', '월', '일'])
        && tok.chars().any(|c| c.is_ascii_digit())
    {
        return true;
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
        if registry.folded_exact_phrase_index.contains_key(&folded) {
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
                | "far" | "off" | "yet" | "per" | "via" | "ok"
        ) {
            return true;
        }
    }
    if let Some(loc) = locale {
        let is_major = matches!(
            loc,
            "en" | "tr" | "de" | "fr" | "es" | "it" | "pt" | "nl" | "ru" | "ar" | "fa" | "zh"
                | "ja" | "ko" | "pl" | "uk" | "sv" | "no" | "da" | "fi" | "cs" | "ro" | "hu"
                | "el" | "he" | "hi" | "id" | "vi" | "th"
        );
        if !is_major && (is_single_token || tok.len() <= 12) {
            return true;
        }
        if is_single_token && loc != "en" && period != Period::Day && period != Period::Time && tok.len() <= 3 {
            return true;
        }
    }
    false
}
