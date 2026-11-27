#![allow(clippy::manual_is_multiple_of)]

use crate::locale::{fold_diacritics, CompiledLocale};
use std::borrow::Cow;

/// All 75 non-ASCII Unicode `Nd` decimal-digit zero codepoints in ascending order.
/// Combined with ASCII `'0'` (`0x0030`), this covers all 76 Unicode 16.0 decimal numeral systems.
const UNICODE_ND_ZEROS: [u32; 75] = [
    0x00660, 0x006F0, 0x007C0, 0x00966, 0x009E6, 0x00A66, 0x00AE6, 0x00B66, 0x00BE6, 0x00C66,
    0x00CE6, 0x00D66, 0x00DE6, 0x00E50, 0x00ED0, 0x00F20, 0x01040, 0x01090, 0x017E0, 0x01810,
    0x01946, 0x019D0, 0x01A80, 0x01A90, 0x01B50, 0x01BB0, 0x01C40, 0x01C50, 0x0A620, 0x0A8D0,
    0x0A900, 0x0A9D0, 0x0A9F0, 0x0AA50, 0x0ABF0, 0x0FF10, 0x104A0, 0x10D30, 0x10D40, 0x11066,
    0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x116D0, 0x116DA,
    0x11730, 0x118E0, 0x11950, 0x11BF0, 0x11C50, 0x11D50, 0x11DA0, 0x11F50, 0x16130, 0x16A60,
    0x16AC0, 0x16B50, 0x16D70, 0x1CCF0, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140,
    0x1E2F0, 0x1E4F0, 0x1E5F1, 0x1E950, 0x1FBF0,
];

/// Map any Unicode `Nd` decimal digit character (across all 76 scripts) to `0..=9`.
#[inline]
pub fn unicode_nd_digit(ch: char) -> Option<u8> {
    if ch.is_ascii_digit() {
        return Some((ch as u32 - '0' as u32) as u8);
    }
    let cp = ch as u32;
    if cp < UNICODE_ND_ZEROS[0] {
        return None;
    }
    let idx = UNICODE_ND_ZEROS.partition_point(|&z| z <= cp);
    if idx > 0 {
        let zero = UNICODE_ND_ZEROS[idx - 1];
        let diff = cp - zero;
        if diff < 10 {
            return Some(diff as u8);
        }
    }
    None
}

/// Returns `true` if `ch` is a decimal digit in any of the 68 Unicode `Nd` numeral scripts.
#[inline]
pub fn is_unicode_decimal_digit(ch: char) -> bool {
    unicode_nd_digit(ch).is_some()
}

/// Convert any Unicode `Nd` digit to ASCII `'0'..='9'`; other chars pass through unchanged.
#[inline]
pub fn normalize_nd_char(ch: char) -> char {
    match unicode_nd_digit(ch) {
        Some(d) => char::from_u32('0' as u32 + d as u32).unwrap_or(ch),
        None => ch,
    }
}

#[inline]
fn cjk_digit_val(ch: char) -> Option<u32> {
    match ch {
        '〇' | '零' => Some(0),
        '一' | '壹' | '弌' => Some(1),
        '二' | '兩' | '两' | '貳' | '贰' => Some(2),
        '三' | '參' | '参' | '叁' => Some(3),
        '四' | '肆' => Some(4),
        '五' | '伍' => Some(5),
        '六' | '陸' | '陆' => Some(6),
        '七' | '柒' => Some(7),
        '八' | '捌' => Some(8),
        '九' | '玖' => Some(9),
        _ => None,
    }
}

#[inline]
fn cjk_mult_val(ch: char) -> Option<u32> {
    match ch {
        '十' | '拾' => Some(10),
        '廿' => Some(20),
        '卅' => Some(30),
        '百' | '佰' => Some(100),
        '千' | '仟' => Some(1000),
        '万' | '萬' => Some(10000),
        _ => None,
    }
}

#[inline]
pub fn is_cjk_numeral_char(ch: char) -> bool {
    cjk_digit_val(ch).is_some() || cjk_mult_val(ch).is_some()
}

/// Parse a CJK standard or financial numeral string (e.g. `"二零二四"`, `"二十五"`, `"三百六十五"`, `"壹貳參"`, `"拾"`).
pub fn parse_cjk_numeral(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let has_mult = s.chars().any(|c| cjk_mult_val(c).is_some());
    if !has_mult {
        let mut val: u32 = 0;
        let mut count = 0usize;
        for ch in s.chars() {
            let d = cjk_digit_val(ch)?;
            val = val.checked_mul(10)?.checked_add(d)?;
            count += 1;
        }
        return if count > 0 { Some(val) } else { None };
    }

    let mut total: u32 = 0;
    let mut section: u32 = 0;
    let mut current: Option<u32> = None;
    for ch in s.chars() {
        if let Some(d) = cjk_digit_val(ch) {
            if d == 0 {
                current = None;
            } else {
                current = Some(d);
            }
        } else {
            let m = cjk_mult_val(ch)?;
            if m == 10000 {
                let s_val = section.checked_add(current.take().unwrap_or(0))?;
                total = total.checked_add(s_val.max(1).checked_mul(m)?)?;
                section = 0;
            } else if matches!(ch, '廿' | '卅') {
                if let Some(prev) = current.take() {
                    section = section.checked_add(prev)?;
                }
                section = section.checked_add(m)?;
            } else {
                let mult = current.take().unwrap_or(1);
                section = section.checked_add(mult.checked_mul(m)?)?;
            }
        }
    }
    total
        .checked_add(section)?
        .checked_add(current.unwrap_or(0))
}

#[inline]
pub fn is_geez_numeral_char(ch: char) -> bool {
    ('\u{1369}'..='\u{137C}').contains(&ch)
}

/// Parse an Ethiopic / Ge'ez numeral string (`፩..፼`, e.g. `"፲፭"` -> 15, `"፪፻፴፬"` -> 234).
pub fn parse_geez_numeral(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let mut total: u32 = 0;
    let mut group: u32 = 0;
    for ch in s.chars() {
        let cp = ch as u32;
        if (0x1369..=0x1371).contains(&cp) {
            group = group.checked_add(cp - 0x1368)?;
        } else if (0x1372..=0x137A).contains(&cp) {
            let tens = (cp - 0x1371).checked_mul(10)?;
            group = group.checked_add(tens)?;
        } else if cp == 0x137B {
            // ፻ (100)
            group = group.max(1).checked_mul(100)?;
        } else if cp == 0x137C {
            // ፼ (10000)
            total = total.checked_add(group.max(1).checked_mul(10_000)?)?;
            group = 0;
        } else {
            return None;
        }
    }
    total.checked_add(group)
}

#[inline]
fn tamil_malayalam_mult(ch: char) -> Option<u32> {
    match ch {
        '௰' | '൰' => Some(10),
        '௱' | '൱' => Some(100),
        '௲' | '൲' => Some(1000),
        _ => None,
    }
}

/// Parse non-positional Tamil (`௰௱௲`) or Malayalam (`൰൱൲`) additive/multiplicative numerals.
pub fn parse_tamil_malayalam_numeral(s: &str) -> Option<u32> {
    if s.is_empty() || !s.chars().any(|c| tamil_malayalam_mult(c).is_some()) {
        return None;
    }
    let mut total: u32 = 0;
    let mut cur: Option<u32> = None;
    for ch in s.chars() {
        if let Some(d) = unicode_nd_digit(ch) {
            cur = Some(
                cur.unwrap_or(0)
                    .checked_mul(10)?
                    .checked_add(d as u32)?,
            );
        } else {
            let m = tamil_malayalam_mult(ch)?;
            let coef = cur.take().unwrap_or(1);
            total = total.checked_add(coef.checked_mul(m)?)?;
        }
    }
    total.checked_add(cur.unwrap_or(0))
}

#[inline]
fn is_non_decimal_trigger(c: char) -> bool {
    is_cjk_numeral_char(c) || is_geez_numeral_char(c) || tamil_malayalam_mult(c).is_some()
}

/// Convert CJK Kanji/financial, Ge'ez (`፩..፼`), and Tamil/Malayalam (`௰௱௲`/`൰൱൲`) numeral runs
/// into ASCII decimal digits while preserving weekday compounds like Chinese `星期一` or Japanese `一昨日`.
pub fn normalize_non_decimal_numerals(input: &str) -> Cow<'_, str> {
    if !input.chars().any(is_non_decimal_trigger) {
        return Cow::Borrowed(input);
    }

    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if is_geez_numeral_char(ch) {
            let start = i;
            while i < chars.len() && is_geez_numeral_char(chars[i]) {
                i += 1;
            }
            let run: String = chars[start..i].iter().collect();
            if let Some(val) = parse_geez_numeral(&run) {
                out.push_str(&val.to_string());
            } else {
                out.push_str(&run);
            }
            continue;
        }

        if tamil_malayalam_mult(ch).is_some() {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit() || tamil_malayalam_mult(chars[i]).is_some())
            {
                i += 1;
            }
            let run: String = chars[start..i].iter().collect();
            if let Some(val) = parse_tamil_malayalam_numeral(&run) {
                out.push_str(&val.to_string());
                continue;
            }
            out.push_str(&run);
            continue;
        }

        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if chars
                .get(i)
                .is_some_and(|c| tamil_malayalam_mult(*c).is_some())
            {
                while i < chars.len()
                    && (chars[i].is_ascii_digit() || tamil_malayalam_mult(chars[i]).is_some())
                {
                    i += 1;
                }
                let run: String = chars[start..i].iter().collect();
                if let Some(val) = parse_tamil_malayalam_numeral(&run) {
                    out.push_str(&val.to_string());
                    continue;
                }
                out.push_str(&run);
                continue;
            }
            for &c in &chars[start..i] {
                out.push(c);
            }
            continue;
        }

        if is_cjk_numeral_char(ch) {
            if ch == '四' && chars.get(i + 1..i + 3) == Some(&['半', '期'][..]) {
                out.push(ch);
                i += 1;
                continue;
            }
            let start = i;
            while i < chars.len() && is_cjk_numeral_char(chars[i]) {
                if chars[i] == '四' && chars.get(i + 1..i + 3) == Some(&['半', '期'][..]) {
                    break;
                }
                i += 1;
            }
            let prev = if start > 0 { Some(chars[start - 1]) } else { None };
            let next = chars.get(i).copied();
            let after_weekday_marker =
                prev.is_some_and(|p| matches!(p, '星' | '期' | '周' | '週' | '拜'));
            let has_date_unit_after = next.is_some_and(|n| {
                matches!(
                    n,
                    '年' | '月' | '日' | '号' | '時' | '时' | '点' | '點' | '分' | '秒' | '天' | '個' | '个'
                )
            });
            let is_japanese_day_compound =
                start + 1 == i && chars[start] == '一' && next == Some('昨');
            let after_demonstrative_yi =
                start + 1 == i && chars[start] == '一' && prev.is_some_and(|p| matches!(p, '这' | '這'));
            let run: String = chars[start..i].iter().collect();
            if (!after_weekday_marker || has_date_unit_after)
                && !is_japanese_day_compound
                && !after_demonstrative_yi
            {
                if let Some(val) = parse_cjk_numeral(&run) {
                    out.push_str(&val.to_string());
                    continue;
                }
            }
            out.push_str(&run);
            continue;
        }

        out.push(ch);
        i += 1;
    }

    Cow::Owned(out)
}

/// Strictly validate and parse an uppercase ASCII Roman numeral (`1..=3999`).
/// Rejects lowercase tokens and non-canonical sequences to prevent prose false positives.
pub fn parse_roman_numeral(raw_tok: &str) -> Option<u32> {
    if raw_tok.is_empty() || raw_tok.len() > 15 {
        return None;
    }
    let bytes = raw_tok.as_bytes();
    let val_of = |b: u8| -> Option<u32> {
        match b {
            b'I' => Some(1),
            b'V' => Some(5),
            b'X' => Some(10),
            b'L' => Some(50),
            b'C' => Some(100),
            b'D' => Some(500),
            b'M' => Some(1000),
            _ => None,
        }
    };
    let mut total: u32 = 0;
    let mut i = 0usize;
    while i < bytes.len() {
        let cur = val_of(bytes[i])?;
        if i + 1 < bytes.len() {
            let next = val_of(bytes[i + 1])?;
            if cur < next {
                total = total.checked_add(next - cur)?;
                i += 2;
                continue;
            }
        }
        total = total.checked_add(cur)?;
        i += 1;
    }
    if !(1..=3999).contains(&total) {
        return None;
    }
    // Canonical check: reject invalid strings like "IIII", "VV", "IC", "IIX"
    if to_roman_canonical(total) == raw_tok {
        Some(total)
    } else {
        None
    }
}

fn to_roman_canonical(mut num: u32) -> String {
    const TABLE: &[(u32, &str)] = &[
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut out = String::with_capacity(12);
    for &(v, sym) in TABLE {
        while num >= v {
            out.push_str(sym);
            num -= v;
        }
    }
    out
}

/// Parse multilingual numeric ordinal strings:
/// `"1st"`, `"2nd"`, `"3rd"`, `"4th"`, `"1er"`, `"1re"`, `"2e"`, `"2ème"`, `"1ste"`, `"2de"`,
/// `"1°"`, `"1º"`, `"1ª"`, `"2o"`, `"15'inci"`, `"15inci"`, `"1-го"`, `"1-й"`, `"2-gi"`, etc.
pub fn parse_date_number_or_ordinal(s: &str) -> Option<(u32, usize)> {
    if s.is_empty() || s.len() > 16 {
        return None;
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        if s.len() > 4 {
            return None;
        }
        return s.parse::<u32>().ok().map(|v| (v, s.len()));
    }

    let suffix_stripped = s
        .strip_suffix("st")
        .or_else(|| s.strip_suffix("nd"))
        .or_else(|| s.strip_suffix("rd"))
        .or_else(|| s.strip_suffix("th"))
        .or_else(|| s.strip_suffix("ste"))
        .or_else(|| s.strip_suffix("ème"))
        .or_else(|| s.strip_suffix("eme"))
        .or_else(|| s.strip_suffix("ere"))
        .or_else(|| s.strip_suffix("er"))
        .or_else(|| s.strip_suffix("re"))
        .or_else(|| s.strip_suffix("de"))
        .or_else(|| s.strip_suffix("e"))
        .or_else(|| s.strip_suffix('°'))
        .or_else(|| s.strip_suffix('º'))
        .or_else(|| s.strip_suffix('ª'))
        .or_else(|| s.strip_suffix("-го"))
        .or_else(|| s.strip_suffix("-й"))
        .or_else(|| s.strip_suffix("-я"))
        .or_else(|| s.strip_suffix("-е"))
        .or_else(|| s.strip_suffix("-gi"))
        .or_else(|| s.strip_suffix("-wszy"))
        .or_else(|| s.strip_suffix("-ви"))
        .or_else(|| s.strip_suffix("-ри"))
        .or_else(|| {
            // Turkish apostrophe / attached ordinals: "15'inci", "3'üncü", "15inci", "2nci"
            let base_tr = s
                .strip_suffix("inci")
                .or_else(|| s.strip_suffix("ıncı"))
                .or_else(|| s.strip_suffix("uncu"))
                .or_else(|| s.strip_suffix("üncü"))
                .or_else(|| s.strip_suffix("nci"))
                .or_else(|| s.strip_suffix("ncı"))
                .or_else(|| s.strip_suffix("ncu"))
                .or_else(|| s.strip_suffix("ncü"))?;
            Some(
                base_tr
                    .trim_end_matches(['\'', '’', 'ʼ']),
            )
        });

    let digits = match suffix_stripped {
        Some(d) => d,
        None => {
            let trimmed_oa = s.strip_suffix('o').or_else(|| s.strip_suffix('a'))?;
            if trimmed_oa.is_empty()
                || trimmed_oa.len() > 2
                || !trimmed_oa.bytes().all(|b| b.is_ascii_digit())
            {
                return None;
            }
            let val: u32 = trimmed_oa.parse().ok()?;
            return (1..=31).contains(&val).then_some((val, trimmed_oa.len()));
        }
    };
    if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let val: u32 = digits.parse().ok()?;
    Some((val, digits.len()))
}

#[inline]
fn lookup_word_num(tok: &str, loc: &CompiledLocale, normalize: bool) -> Option<u32> {
    if let Some(&n) = loc.word_numbers.get(tok) {
        return Some(n);
    }
    if normalize {
        let f = fold_diacritics(tok);
        if let Some(&n) = loc.word_numbers.get(f.as_ref()) {
            return Some(n);
        }
    }
    None
}

/// Look up an ordinal word or hyphenated compound ordinal (`"second"` -> 2, `"thirty-first"` -> 31).
pub fn lookup_ordinal(tok: &str, loc: &CompiledLocale, normalize: bool) -> Option<u32> {
    if let Some(&n) = loc.ordinals.get(tok) {
        return Some(n);
    }
    if normalize {
        let f = fold_diacritics(tok);
        if let Some(&n) = loc.ordinals.get(f.as_ref()) {
            return Some(n);
        }
    }
    if let Some((left, right)) = tok.split_once('-') {
        let lv = lookup_word_num(left, loc, normalize)?;
        let rv = lookup_ordinal(right, loc, normalize)?;
        if lv >= 20 && lv % 10 == 0 && (1..=9).contains(&rv) {
            return Some(lv + rv);
        }
    }
    None
}

#[inline]
fn is_numeral_connector(loc_code: &str, tok: &str) -> bool {
    match tok {
        "and" => matches!(loc_code, "en" | "af"),
        "ve" => matches!(loc_code, "tr" | "az"),
        "y" | "e" => matches!(loc_code, "es" | "pt" | "it" | "ca" | "gl" | "ast"),
        "i" => matches!(loc_code, "pl" | "hr" | "sr" | "bs" | "uk" | "ca" | "sk" | "sl"),
        "et" => loc_code == "fr",
        "und" => matches!(loc_code, "de" | "gsw" | "lb" | "ksh" | "wae"),
        "en" => matches!(loc_code, "nl" | "af"),
        "и" => matches!(loc_code, "ru" | "bg" | "mk" | "sr" | "be" | "uk"),
        "ו" | "و" => true,
        _ => false,
    }
}

#[inline]
fn ko_sino_digit(ch: char) -> Option<u32> {
    match ch {
        '일' => Some(1),
        '이' => Some(2),
        '삼' => Some(3),
        '사' => Some(4),
        '오' => Some(5),
        '육' | '륙' => Some(6),
        '칠' => Some(7),
        '팔' => Some(8),
        '구' => Some(9),
        _ => None,
    }
}

/// Parse single-token compound words (`"twenty-one"`, German `"einundzwanzig"`, Dutch `"eenentwintig"`,
/// French `"quatre-vingts"`/`"quatre-vingt-un"`, Korean `"스물한"`/`"이십오"`, Japanese `"二十五"`).
pub fn parse_single_word_cardinal(tok: &str, loc: &CompiledLocale, normalize: bool) -> Option<u32> {
    if let Some(v) = lookup_word_num(tok, loc, normalize) {
        return Some(v);
    }

    // Korean single-token Pure Korean ("스물한" -> 21) and Sino-Korean ("이십오" -> 25) compounds
    if loc.code == "ko" {
        for &(ten_p, ten_v) in &[
            ("스물", 20u32),
            ("서른", 30),
            ("마흔", 40),
            ("쉰", 50),
            ("예순", 60),
            ("일흔", 70),
            ("여든", 80),
            ("아흔", 90),
        ] {
            if let Some(rest) = tok.strip_prefix(ten_p) {
                if !rest.is_empty() {
                    if let Some(ones) = lookup_word_num(rest, loc, normalize) {
                        if (1..=9).contains(&ones) {
                            return Some(ten_v + ones);
                        }
                    }
                }
            }
        }
        if let Some((left, right)) = tok.split_once('십') {
            let tens = if left.is_empty() {
                1
            } else {
                let mut lc = left.chars();
                match (lc.next(), lc.next()) {
                    (Some(c), None) => ko_sino_digit(c)?,
                    _ => return None,
                }
            };
            let ones = if right.is_empty() {
                0
            } else {
                let mut rc = right.chars();
                match (rc.next(), rc.next()) {
                    (Some(c), None) => ko_sino_digit(c)?,
                    _ => return None,
                }
            };
            if (1..=9).contains(&tens) {
                return Some(tens * 10 + ones);
            }
        }
    }

    // Dutch inverted compounds: "tweeëntwintig" -> 22, "eenentwintig" -> 21, "zevenentwintig" -> 27
    if matches!(loc.code.as_str(), "nl" | "af") {
        for sep in ["ën", "en"] {
            for (idx, _) in tok.match_indices(sep) {
                let left = &tok[..idx];
                let right = &tok[idx + sep.len()..];
                if left.is_empty() || right.is_empty() {
                    continue;
                }
                if let (Some(ones), Some(tens)) = (
                    lookup_word_num(left, loc, normalize),
                    lookup_word_num(right, loc, normalize),
                ) {
                    if (1..=9).contains(&ones) && (20..=90).contains(&tens) && tens % 10 == 0 {
                        return Some(tens + ones);
                    }
                }
            }
        }
    }

    // German inverted "einundzwanzig" / "fünfunddreißig" -> ones + "und" + tens
    if let Some((left, right)) = tok.split_once("und") {
        if !left.is_empty() && !right.is_empty() {
            let l_norm = if left == "ein" { "eins" } else { left };
            let lv = lookup_word_num(l_norm, loc, normalize)
                .or_else(|| lookup_word_num(left, loc, normalize));
            let rv = lookup_word_num(right, loc, normalize);
            if let (Some(ones), Some(tens)) = (lv, rv) {
                if (1..=9).contains(&ones) && (20..=90).contains(&tens) {
                    return Some(tens + ones);
                }
            }
        }
    }

    // German/Dutch multiplier glued prefix: "zweihundert", "dreihundert"
    for &(mult_suffix, mult_val) in &[("hundert", 100u32), ("tausend", 1000u32), ("honderd", 100u32)] {
        if let Some(stem) = tok.strip_suffix(mult_suffix) {
            if let Some(coef) = lookup_word_num(stem, loc, normalize) {
                if (1..=99).contains(&coef) {
                    return Some(coef * mult_val);
                }
            }
        }
    }

    // Hyphenated multi-word numbers ("twenty-one", "quatre-vingts", "quatre-vingt-un", "soixante-dix")
    if tok.contains('-') {
        if loc.code == "fr" {
            if let Some(rest) = tok.strip_prefix("quatre-vingt-") {
                if let Some(tail) = lookup_word_num(rest, loc, normalize) {
                    if (1..=19).contains(&tail) {
                        return Some(80 + tail);
                    }
                }
            }
            if let Some(rest) = tok.strip_prefix("soixante-") {
                if let Some(tail) = lookup_word_num(rest, loc, normalize) {
                    if (1..=19).contains(&tail) {
                        return Some(60 + tail);
                    }
                }
            }
        }
        let parts: Vec<&str> = tok.split('-').filter(|p| !p.is_empty()).collect();
        if parts.len() >= 2 {
            if let Some((val, consumed)) = parse_cardinal_slice(&parts, loc, normalize) {
                if consumed == parts.len() {
                    return Some(val);
                }
            }
        }
    }

    // Attached Arabic/Hebrew "and" prefix on compound number tokens ("ואחד", "وعشرون")
    if let Some(rest) = tok.strip_prefix('ו').or_else(|| tok.strip_prefix('و')) {
        if !rest.is_empty() {
            if let Some(v) = lookup_word_num(rest, loc, normalize) {
                return Some(v);
            }
        }
    }

    None
}

/// Parse a compositional cardinal slice starting at `tokens[0]`, returning `(value, tokens_consumed)`.
pub fn parse_cardinal_slice(
    tokens: &[&str],
    loc: &CompiledLocale,
    normalize: bool,
) -> Option<(u32, usize)> {
    if tokens.is_empty() {
        return None;
    }

    // English indefinite pair: "a couple of" / "couple of" -> 2
    if loc.code == "en" {
        if tokens.len() >= 3 && tokens[0] == "a" && tokens[1] == "couple" && tokens[2] == "of" {
            return Some((2, 3));
        }
        if tokens.len() >= 2 && tokens[0] == "couple" && tokens[1] == "of" {
            return Some((2, 2));
        }
    }

    let first_val = parse_single_word_cardinal(tokens[0], loc, normalize)?;
    // Special case: avoid swallowing standalone article "a"/"an" when followed by a cardinal
    if tokens.len() == 1 || tokens[0] == "an" {
        return Some((first_val, 1));
    }

    let mut total: u32 = 0;
    let mut current: u32 = first_val;
    let mut consumed: usize = 1;
    let mut expect_after_conn = false;

    while consumed < tokens.len() {
        let tok = tokens[consumed];
        if is_numeral_connector(loc.code.as_str(), tok) {
            // Look ahead: only treat connector as part of number if next token is a numeral
            if consumed + 1 < tokens.len()
                && parse_single_word_cardinal(tokens[consumed + 1], loc, normalize).is_some()
            {
                consumed += 1;
                expect_after_conn = true;
                continue;
            }
            break;
        }

        // Vietnamese tens multiplier: "hai mươi một" -> 2 * 10 + 1 = 21
        if loc.code == "vi"
            && (2..=9).contains(&current)
            && matches!(fold_diacritics(tok).as_ref(), "muoi" | "mươi")
        {
            current = current.checked_mul(10)?;
            expect_after_conn = false;
            consumed += 1;
            continue;
        }

        let Some(v) = parse_single_word_cardinal(tok, loc, normalize) else {
            break;
        };
        let has_attached_conn = (tok.starts_with('و') || tok.starts_with('ו'))
            && lookup_word_num(tok, loc, normalize).is_none();
        if v == 1000 {
            total = total.checked_add(current.max(1).checked_mul(1000)?)?;
            current = 0;
        } else if v == 100 {
            current = current.max(1).checked_mul(100)?;
        } else if current >= 100 {
            total = total.checked_add(current)?;
            current = v;
        } else if (current >= 20 && current % 10 == 0 && v < 10)
            || (loc.code == "tr" && current == 10 && (1..=9).contains(&v))
            || (expect_after_conn && v < 10)
            || (loc.code == "fr" && current == 60 && (10..=19).contains(&v))
            || (loc.code == "fr" && current == 80 && (1..=19).contains(&v))
            || (matches!(loc.code.as_str(), "ar" | "he")
                && current < 10
                && (20..100).contains(&v)
                && (expect_after_conn || has_attached_conn))
        {
            current = current.checked_add(v)?;
        } else if current == 1 && v >= 100 {
            current = v;
        } else {
            // Stop before adjacent unrelated words
            break;
        }
        expect_after_conn = false;
        consumed += 1;
    }

    total.checked_add(current).map(|val| (val, consumed))
}

/// Collapse compositional cardinals in `s` into ASCII numbers (e.g. `"twenty-one days ago"` -> `"21 days ago"`).
/// Returns `None` if no word numbers matched.
pub fn collapse_word_numbers(s: &str, loc: &CompiledLocale, normalize: bool) -> Option<String> {
    if loc.word_numbers.is_empty() {
        return None;
    }

    // Spaceless Thai leading numeral extraction (e.g. "ห้านาทีที่แล้ว" / "เมื่อห้านาทีที่แล้ว" -> "5 นาทีที่แล้ว")
    if loc.code == "th" {
        let th_body = s.strip_prefix("เมื่อ").unwrap_or(s).trim_start();
        let mut best: Option<(&str, u32)> = None;
        for (word, &val) in &loc.word_numbers {
            if !word.is_ascii() && th_body.starts_with(word.as_str()) {
                let rem = &th_body[word.len()..];
                if !rem.is_empty() && best.is_none_or(|(bw, _)| word.len() > bw.len()) {
                    best = Some((word.as_str(), val));
                }
            }
        }
        if let Some((bw, val)) = best {
            let rem = th_body[bw.len()..].trim_start();
            return Some(format!("{} {}", val, rem));
        }
    }

    let raw_toks: Vec<&str> = s.split_whitespace().collect();
    if raw_toks.is_empty() {
        return None;
    }

    let mut out = Vec::with_capacity(raw_toks.len());
    let mut changed = false;
    let mut i = 0usize;
    while i < raw_toks.len() {
        if let Some((val, used)) = parse_cardinal_slice(&raw_toks[i..], loc, normalize) {
            out.push(val.to_string());
            i += used;
            changed = true;
            continue;
        }
        out.push(raw_toks[i].to_string());
        i += 1;
    }

    if changed {
        Some(out.join(" "))
    } else {
        None
    }
}
