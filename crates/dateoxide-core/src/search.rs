use crate::locale::{normalize_digits_and_lower, LocaleRegistry};
use crate::settings::{ParsedDateTime, Settings};
use crate::parse_with_settings;

/// Linear-time date finder across free-form text (compatible with `dateparser.search.search_dates`).
/// Returns `Vec<(matched_substring, ParsedDateTime)>`.
pub fn search_dates(text: &str, settings: &Settings) -> Vec<(String, ParsedDateTime)> {
    if text.trim().is_empty() {
        return Vec::new();
    }

    let registry = LocaleRegistry::global();

    // Collect whitespace-delimited tokens with their exact byte ranges in the original string
    let spans = collect_word_spans(text);
    if spans.is_empty() {
        return Vec::new();
    }

    // Classify each token as: strong date signal, connector/filler, or non-date
    let mut kinds = Vec::with_capacity(spans.len());
    for &(start, end) in &spans {
        let raw = &text[start..end];
        let trimmed = raw.trim_matches(|c: char| matches!(c, ',' | '.' | ';' | '!' | '?' | '(' | ')' | '[' | ']' | '"' | '\'' | '«' | '»'));
        kinds.push(classify_token(trimmed, registry));
    }

    let mut results = Vec::new();
    let mut i = 0;
    while i < spans.len() {
        if kinds[i] != TokenKind::Signal {
            i += 1;
            continue;
        }

        // Expand candidate span greedily across adjacent Signal or single Connector tokens (max 8 tokens)
        let mut best_end = i;
        let mut j = i + 1;
        while j < spans.len() && (j - i) < 8 {
            // Stop at sentence-ending punctuation after the previous token (except day dots like "26.")
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
                    // Include connector only if followed by a Signal token
                    if j + 1 < spans.len() && kinds[j + 1] == TokenKind::Signal {
                        j += 1;
                    } else {
                        break;
                    }
                }
                TokenKind::Other => break,
            }
        }

        // Try longest subspan from [i..=best_end] down to [i..=i]
        let mut matched = false;
        for end_idx in (i..=best_end).rev() {
            // Skip spans that end on a dangling preposition/article ("in", "at", "on", "de", "of", "à", "the")
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
            let raw_sub = text[sub_start..sub_end]
                .trim_matches(|c: char| matches!(c, ',' | ';' | '!' | '?' | '(' | ')' | '[' | ']' | '"' | '\''));
            let raw_sub = raw_sub.trim_end_matches('.');

            // Avoid matching bare 1–3 digit numbers like "42" or "2" as dates inside prose
            if raw_sub.bytes().all(|b| b.is_ascii_digit()) && raw_sub.len() < 4 {
                continue;
            }
            // Avoid matching standalone words like "in" or "ago" or "at"
            if end_idx == i && !is_valid_single_token_date(raw_sub, registry) {
                continue;
            }

            if let Some(dt) = parse_with_settings(raw_sub, settings) {
                results.push((raw_sub.to_string(), dt));
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

fn classify_token(tok: &str, registry: &LocaleRegistry) -> TokenKind {
    if tok.is_empty() {
        return TokenKind::Other;
    }

    // Contains any ASCII or Unicode digit (dates, times, years, counts, CJK dates)
    if tok.chars().any(|c| c.is_ascii_digit() || matches!(c, '\u{0660}'..='\u{0669}' | '年' | '月' | '日')) {
        return TokenKind::Signal;
    }

    let lower = normalize_digits_and_lower(tok);

    // Common connector words inside dates ("de", "of", "at", "à", "saat", "and", "ve", "le", "the", "il", "y", "a")
    if matches!(
        lower.as_str(),
        "of" | "the" | "at" | "on" | "de" | "del" | "el" | "la" | "le" | "à" | "am" | "um"
            | "saat" | "and" | "ve" | "und" | "et" | "il" | "y" | "a" | "an" | "in" | "en"
            | "dans" | "vor" | "hace"
    ) {
        // Note: "in", "vor", "hace", "dans", "en" can START a relative expression like "vor 2 Wochen"
        if matches!(lower.as_str(), "in" | "vor" | "hace" | "dans" | "il" | "a" | "an" | "en") {
            return TokenKind::Signal;
        }
        return TokenKind::Connector;
    }

    if registry.known_date_tokens.contains(&lower) {
        return TokenKind::Signal;
    }

    // Common timezone abbreviations
    if tok.len() >= 3
        && tok.len() <= 5
        && tok.bytes().all(|b| b.is_ascii_uppercase())
        && crate::timezone::parse_tz_spec(tok).is_some()
    {
        return TokenKind::Signal;
    }

    // Relative modifiers
    if matches!(
        lower.as_str(),
        "last" | "next" | "this" | "geçen" | "gelecek" | "bu" | "bir" | "iki" | "üç" | "one"
            | "two" | "three" | "four" | "five" | "zwei" | "drei" | "ein" | "une" | "deux"
    ) {
        return TokenKind::Signal;
    }

    TokenKind::Other
}

fn is_valid_single_token_date(tok: &str, registry: &LocaleRegistry) -> bool {
    // ISO/numeric date, time, or CJK date
    if tok.contains(['-', '/', '.', ':', '年', '月', '日']) && tok.chars().any(|c| c.is_ascii_digit())
    {
        return true;
    }
    // 4-digit year in plausible range
    if tok.len() == 4 && tok.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(y) = tok.parse::<u32>() {
            return (1900..=2100).contains(&y);
        }
    }
    // Exact relative word ("yesterday", "today", "tomorrow", "bugün", "dün", "yarın", "demain", "gestern", "今日")
    let lower = normalize_digits_and_lower(tok);
    if registry.exact_phrase_index.contains_key(&lower) {
        return true;
    }
    false
}
