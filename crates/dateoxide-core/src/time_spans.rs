use crate::relative::apply_relative_offset;
use crate::locale::TimeUnit;
use crate::settings::{Settings, StartOfWeek};
use chrono::{Datelike, Duration, NaiveDateTime};
use regex::Regex;
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct TimeSpanMatch {
    pub span_type: &'static str,
    pub direction: &'static str,
    pub number: Option<u32>,
    pub matched_text: String,
    pub start_pos: usize,
    pub end_pos: usize,
}

struct CompiledSpanPattern {
    regex: Regex,
    span_type: &'static str,
    direction: &'static str,
    has_num: bool,
}

static SPAN_PATTERNS: OnceLock<Vec<CompiledSpanPattern>> = OnceLock::new();

fn patterns() -> &'static [CompiledSpanPattern] {
    SPAN_PATTERNS.get_or_init(|| {
        let specs = [
            // English
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+month\b", "month", "past", false),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+week\b", "week", "past", false),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+(\d+)\s+days?\b", "days", "past", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+(\d+)\s+weeks?\b", "weeks", "past", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:past|last|previous)\s+(\d+)\s+months?\b", "months", "past", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+month\b", "month", "future", false),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+week\b", "week", "future", false),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+(\d+)\s+days?\b", "days", "future", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+(\d+)\s+weeks?\b", "weeks", "future", true),
            (r"(?i)\b(?:for\s+the\s+|during\s+the\s+|in\s+the\s+)?(?:next|coming|following)\s+(\d+)\s+months?\b", "months", "future", true),
            // Turkish
            (r"(?i)\b(?:geçen|gecen|önceki|onceki)\s+ay\b", "month", "past", false),
            (r"(?i)\b(?:geçen|gecen|önceki|onceki)\s+hafta\b", "week", "past", false),
            (r"(?i)\b(?:son|geçen|gecen)\s+(\d+)\s+(?:gün|gun)\b", "days", "past", true),
            (r"(?i)\b(?:son|geçen|gecen)\s+(\d+)\s+hafta\b", "weeks", "past", true),
            (r"(?i)\b(?:son|geçen|gecen)\s+(\d+)\s+ay\b", "months", "past", true),
            (r"(?i)\b(?:gelecek|önümüzdeki|onumuzdeki|sonraki)\s+ay\b", "month", "future", false),
            (r"(?i)\b(?:gelecek|önümüzdeki|onumuzdeki|sonraki)\s+hafta\b", "week", "future", false),
            (r"(?i)\b(?:gelecek|önümüzdeki|onumuzdeki|sonraki)\s+(\d+)\s+(?:gün|gun)\b", "days", "future", true),
            (r"(?i)\b(?:gelecek|önümüzdeki|onumuzdeki|sonraki)\s+(\d+)\s+hafta\b", "weeks", "future", true),
            (r"(?i)\b(?:gelecek|önümüzdeki|onumuzdeki|sonraki)\s+(\d+)\s+ay\b", "months", "future", true),
            // German
            (r"(?i)\b(?:letzten?|vergangenen?)\s+monat\b", "month", "past", false),
            (r"(?i)\bletzte\s+woche\b", "week", "past", false),
            (r"(?i)\b(?:letzten?|vergangenen?)\s+(\d+)\s+tage?n?\b", "days", "past", true),
            (r"(?i)\b(?:letzten?|vergangenen?)\s+(\d+)\s+wochen?\b", "weeks", "past", true),
            (r"(?i)\b(?:letzten?|vergangenen?)\s+(\d+)\s+monate?n?\b", "months", "past", true),
            (r"(?i)\b(?:nächsten?|naechsten?|kommenden?)\s+monat\b", "month", "future", false),
            (r"(?i)\b(?:nächste|naechste|kommende)\s+woche\b", "week", "future", false),
            (r"(?i)\b(?:nächsten?|naechsten?|kommenden?)\s+(\d+)\s+tage?n?\b", "days", "future", true),
            (r"(?i)\b(?:nächsten?|naechsten?|kommenden?)\s+(\d+)\s+wochen?\b", "weeks", "future", true),
            (r"(?i)\b(?:nächsten?|naechsten?|kommenden?)\s+(\d+)\s+monate?n?\b", "months", "future", true),
            // French
            (r"(?i)\b(?:mois\s+dernier|dernier\s+mois)\b", "month", "past", false),
            (r"(?i)\b(?:semaine\s+dernière|semaine\s+derniere|dernière\s+semaine)\b", "week", "past", false),
            (r"(?i)\b(?:(\d+)\s+derniers?\s+jours|derniers?\s+(\d+)\s+jours)\b", "days", "past", true),
            (r"(?i)\b(?:(\d+)\s+dernières?\s+semaines|(\d+)\s+dernieres?\s+semaines)\b", "weeks", "past", true),
            (r"(?i)\b(?:(\d+)\s+derniers?\s+mois)\b", "months", "past", true),
            (r"(?i)\b(?:mois\s+prochain|prochain\s+mois)\b", "month", "future", false),
            (r"(?i)\b(?:semaine\s+prochaine|prochaine\s+semaine)\b", "week", "future", false),
            (r"(?i)\b(\d+)\s+prochains?\s+jours\b", "days", "future", true),
            (r"(?i)\b(\d+)\s+prochaines?\s+semaines\b", "weeks", "future", true),
            (r"(?i)\b(\d+)\s+prochains?\s+mois\b", "months", "future", true),
            // Spanish
            (r"(?i)\b(?:mes\s+pasado|último\s+mes|ultimo\s+mes)\b", "month", "past", false),
            (r"(?i)\b(?:semana\s+pasada|última\s+semana|ultima\s+semana)\b", "week", "past", false),
            (r"(?i)\b(?:últimos|ultimos|pasados)\s+(\d+)\s+(?:días|dias)\b", "days", "past", true),
            (r"(?i)\b(?:últimas|ultimas|pasadas)\s+(\d+)\s+semanas\b", "weeks", "past", true),
            (r"(?i)\b(?:últimos|ultimos|pasados)\s+(\d+)\s+meses\b", "months", "past", true),
            (r"(?i)\b(?:próximo|proximo)\s+mes\b", "month", "future", false),
            (r"(?i)\b(?:próxima|proxima)\s+semana\b", "week", "future", false),
            (r"(?i)\b(?:próximos|proximos|siguientes)\s+(\d+)\s+(?:días|dias)\b", "days", "future", true),
            (r"(?i)\b(?:próximas|proximas|siguientes)\s+(\d+)\s+semanas\b", "weeks", "future", true),
            (r"(?i)\b(?:próximos|proximos|siguientes)\s+(\d+)\s+meses\b", "months", "future", true),
            // Italian
            (r"(?i)\b(?:mese\s+scorso|ultimo\s+mese)\b", "month", "past", false),
            (r"(?i)\b(?:settimana\s+scorsa|ultima\s+settimana)\b", "week", "past", false),
            (r"(?i)\b(?:ultimi|scorsi)\s+(\d+)\s+giorni?\b", "days", "past", true),
            (r"(?i)\b(?:ultime|scorse)\s+(\d+)\s+settimane?\b", "weeks", "past", true),
            (r"(?i)\b(?:ultimi|scorsi)\s+(\d+)\s+mesi\b", "months", "past", true),
            (r"(?i)\bprossimo\s+mese\b", "month", "future", false),
            (r"(?i)\bprossima\s+settimana\b", "week", "future", false),
            (r"(?i)\bprossimi\s+(\d+)\s+giorni?\b", "days", "future", true),
            (r"(?i)\bprossime\s+(\d+)\s+settimane?\b", "weeks", "future", true),
            (r"(?i)\bprossimi\s+(\d+)\s+mesi\b", "months", "future", true),
            // Portuguese
            (r"(?i)\b(?:mês\s+passado|mes\s+passado|último\s+mês)\b", "month", "past", false),
            (r"(?i)\b(?:semana\s+passada|última\s+semana)\b", "week", "past", false),
            (r"(?i)\b(?:últimos|ultimos|passados)\s+(\d+)\s+dias?\b", "days", "past", true),
            (r"(?i)\b(?:últimas|ultimas|passadas)\s+(\d+)\s+semanas\b", "weeks", "past", true),
            (r"(?i)\b(?:últimos|ultimos|passados)\s+(\d+)\s+meses\b", "months", "past", true),
            (r"(?i)\b(?:próximo|proximo)\s+(?:mês|mes)\b", "month", "future", false),
            (r"(?i)\b(?:próxima|proxima)\s+semana\b", "week", "future", false),
            (r"(?i)\b(?:próximos|proximos)\s+(\d+)\s+dias?\b", "days", "future", true),
            (r"(?i)\b(?:próximas|proximas)\s+(\d+)\s+semanas\b", "weeks", "future", true),
            (r"(?i)\b(?:próximos|proximos)\s+(\d+)\s+meses\b", "months", "future", true),
            // Dutch
            (r"(?i)\b(?:vorige|afgelopen)\s+maand\b", "month", "past", false),
            (r"(?i)\b(?:vorige|afgelopen)\s+week\b", "week", "past", false),
            (r"(?i)\b(?:afgelopen|laatste)\s+(\d+)\s+dag(?:en)?\b", "days", "past", true),
            (r"(?i)\b(?:afgelopen|laatste)\s+(\d+)\s+we(?:ek|ken)\b", "weeks", "past", true),
            (r"(?i)\b(?:afgelopen|laatste)\s+(\d+)\s+maand(?:en)?\b", "months", "past", true),
            (r"(?i)\bkomende\s+maand\b", "month", "future", false),
            (r"(?i)\bvolgende\s+week\b", "week", "future", false),
            (r"(?i)\bkomende\s+(\d+)\s+dag(?:en)?\b", "days", "future", true),
            (r"(?i)\bkomende\s+(\d+)\s+we(?:ek|ken)\b", "weeks", "future", true),
            (r"(?i)\bkomende\s+(\d+)\s+maand(?:en)?\b", "months", "future", true),
            // Polish
            (r"(?i)\b(?:ostatni|zeszły|poprzedni)\s+miesiąc\b", "month", "past", false),
            (r"(?i)\b(?:ostatni|zeszły|poprzedni)\s+tydzień\b", "week", "past", false),
            (r"(?i)\bostatni(?:e|ch)\s+(\d+)\s+(?:dni|dzień)\b", "days", "past", true),
            (r"(?i)\bostatni(?:e|ch)\s+(\d+)\s+tygodni(?:e|a)?\b", "weeks", "past", true),
            (r"(?i)\bostatni(?:e|ch)\s+(\d+)\s+miesi(?:ące|ęcy)\b", "months", "past", true),
            (r"(?i)\bnastępny\s+miesiąc\b", "month", "future", false),
            (r"(?i)\bnastępny\s+tydzień\b", "week", "future", false),
            (r"(?i)\bnastępne\s+(\d+)\s+(?:dni|dzień)\b", "days", "future", true),
            // Russian
            (r"(?i)(?:прошлый|последний)\s+месяц", "month", "past", false),
            (r"(?i)(?:прошлая|последняя)\s+неделя", "week", "past", false),
            (r"(?i)(?:последние|прошлые)\s+(\d+)\s+(?:дня|дней|день)", "days", "past", true),
            (r"(?i)(?:последние|прошлые)\s+(\d+)\s+(?:недели|недель|неделю)", "weeks", "past", true),
            (r"(?i)(?:последние|прошлые)\s+(\d+)\s+(?:месяца|месяцев|месяц)", "months", "past", true),
            (r"(?i)следующий\s+месяц", "month", "future", false),
            (r"(?i)следующая\s+неделя", "week", "future", false),
            (r"(?i)следующие\s+(\d+)\s+(?:дня|дней|день)", "days", "future", true),
            // Japanese / Chinese / Korean
            (r"(?:先月|上个月|上個月|지난달|지난\s*달)", "month", "past", false),
            (r"(?:先週|上周|上週|지난주|지난\s*주)", "week", "past", false),
            (r"(?:来月|下个月|下個月|다음달|다음\s*달)", "month", "future", false),
            (r"(?:来週|下周|下週|다음주|다음\s*주)", "week", "future", false),
            (r"(?:過去|过去|지난)\s*(\d+)\s*(?:日間?|天|일)", "days", "past", true),
            (r"(?:過去|过去|지난)\s*(\d+)\s*(?:週間?|周|週|주)", "weeks", "past", true),
            (r"(?:今後|未来|앞으로|다음)\s*(\d+)\s*(?:日間?|天|일)", "days", "future", true),
        ];
        specs
            .into_iter()
            .map(|(pat, span_type, direction, has_num)| CompiledSpanPattern {
                regex: Regex::new(pat).expect("valid static span regex"),
                span_type,
                direction,
                has_num,
            })
            .collect()
    })
}

/// Detect time span expressions such as `"last week"`, `"past 3 days"`, `"next 2 months"`.
pub fn detect_time_span(text: &str) -> Option<TimeSpanMatch> {
    if let Some(m) = detect_time_span_inner(text, text, None) {
        return Some(m);
    }
    if let Some((norm, segs)) = replace_span_word_numbers(text) {
        return detect_time_span_inner(&norm, text, Some(&segs));
    }
    None
}

struct SpanSeg {
    scan_start: usize,
    scan_end: usize,
    orig_start: usize,
    orig_end: usize,
}

fn map_scan_span_to_orig(full_start: usize, full_end: usize, orig: &str, segs: &[SpanSeg]) -> (usize, usize) {
    let mut orig_start = None;
    let mut orig_end = None;
    for seg in segs {
        if seg.scan_end > full_start && orig_start.is_none() {
            orig_start = Some(if full_start <= seg.scan_start {
                seg.orig_start
            } else {
                let delta = full_start - seg.scan_start;
                let max_d = seg.orig_end - seg.orig_start;
                if seg.orig_end - seg.orig_start == seg.scan_end - seg.scan_start && delta <= max_d {
                    seg.orig_start + delta
                } else {
                    seg.orig_start
                }
            });
        }
        if seg.scan_start < full_end {
            orig_end = Some(if full_end >= seg.scan_end {
                seg.orig_end
            } else {
                let delta = full_end - seg.scan_start;
                let max_d = seg.orig_end - seg.orig_start;
                if seg.orig_end - seg.orig_start == seg.scan_end - seg.scan_start && delta <= max_d {
                    seg.orig_start + delta
                } else {
                    seg.orig_end
                }
            });
        }
    }
    let s = orig_start.unwrap_or(0).min(orig.len());
    let e = orig_end.unwrap_or(orig.len()).clamp(s, orig.len());
    (s, e)
}

fn detect_time_span_inner(scan: &str, orig: &str, segs: Option<&[SpanSeg]>) -> Option<TimeSpanMatch> {
    for pat in patterns() {
        if let Some(caps) = pat.regex.captures(scan) {
            let full = caps.get(0)?;
            let number = if pat.has_num {
                let num_str = caps.iter().skip(1).flatten().next()?.as_str();
                match num_str.parse::<u32>() {
                    Ok(n) => Some(n),
                    Err(_) => continue,
                }
            } else {
                None
            };
            let (start_pos, end_pos, matched_text) = match segs {
                None => (full.start(), full.end(), full.as_str().to_string()),
                Some(map) => {
                    let (s, e) = map_scan_span_to_orig(full.start(), full.end(), orig, map);
                    let mt = orig.get(s..e).unwrap_or(full.as_str()).to_string();
                    (s, e, mt)
                }
            };
            return Some(TimeSpanMatch {
                span_type: pat.span_type,
                direction: pat.direction,
                number,
                matched_text,
                start_pos,
                end_pos,
            });
        }
    }
    None
}

fn replace_span_word_numbers(text: &str) -> Option<(String, Vec<SpanSeg>)> {
    let mut toks: Vec<(usize, usize, &str)> = Vec::new();
    let mut start: Option<usize> = None;
    for (idx, ch) in text.char_indices() {
        if ch.is_whitespace() {
            if let Some(s) = start.take() {
                toks.push((s, idx, &text[s..idx]));
            }
        } else if start.is_none() {
            start = Some(idx);
        }
    }
    if let Some(s) = start {
        toks.push((s, text.len(), &text[s..]));
    }
    if toks.is_empty() {
        return None;
    }

    let reg = crate::locale::LocaleRegistry::global();
    let cand_locs: Vec<&crate::locale::CompiledLocale> = ["en", "tr", "de", "fr", "es", "it", "pt", "nl", "pl", "ru"]
        .iter()
        .filter_map(|c| reg.resolve_lang_id(c).map(|id| &reg.locales[id]))
        .collect();
    let lowers: Vec<String> = toks
        .iter()
        .map(|(_, _, raw)| crate::locale::normalize_digits_and_lower(raw))
        .collect();
    let lower_refs: Vec<&str> = lowers.iter().map(String::as_str).collect();

    let mut scan = String::with_capacity(text.len());
    let mut segs: Vec<SpanSeg> = Vec::with_capacity(toks.len() * 2);
    let mut changed = false;
    let mut cursor = 0usize;
    let mut ti = 0usize;

    while ti < toks.len() {
        let (tok_start, tok_end, _) = toks[ti];
        if tok_start > cursor {
            let ss = scan.len();
            scan.push_str(&text[cursor..tok_start]);
            segs.push(SpanSeg {
                scan_start: ss,
                scan_end: scan.len(),
                orig_start: cursor,
                orig_end: tok_start,
            });
        }

        let mut best: Option<(u32, usize)> = None;
        for loc in &cand_locs {
            if let Some((val, used)) = crate::numerals::parse_cardinal_slice(&lower_refs[ti..], loc, true) {
                // Skip bare indefinite article "a" unless "a couple of" (used == 3)
                if lower_refs[ti] == "a" && used == 1 {
                    continue;
                }
                if best.is_none_or(|(_, bu)| used > bu) {
                    best = Some((val, used));
                }
            }
        }

        if let Some((val, used)) = best {
            let group_end = toks[ti + used - 1].1;
            let ss = scan.len();
            scan.push_str(&val.to_string());
            segs.push(SpanSeg {
                scan_start: ss,
                scan_end: scan.len(),
                orig_start: tok_start,
                orig_end: group_end,
            });
            cursor = group_end;
            ti += used;
            changed = true;
        } else {
            let ss = scan.len();
            scan.push_str(&text[tok_start..tok_end]);
            segs.push(SpanSeg {
                scan_start: ss,
                scan_end: scan.len(),
                orig_start: tok_start,
                orig_end: tok_end,
            });
            cursor = tok_end;
            ti += 1;
        }
    }
    if cursor < text.len() {
        let ss = scan.len();
        scan.push_str(&text[cursor..]);
        segs.push(SpanSeg {
            scan_start: ss,
            scan_end: scan.len(),
            orig_start: cursor,
            orig_end: text.len(),
        });
    }

    if changed {
        Some((scan, segs))
    } else {
        None
    }
}

fn week_start(date: NaiveDateTime, start_of_week: StartOfWeek) -> NaiveDateTime {
    let mut days_back = date.weekday().num_days_from_monday() as i64;
    if start_of_week != StartOfWeek::Monday {
        days_back = (days_back + 1) % 7;
    }
    date - Duration::days(days_back)
}

/// Return `(start, end)` datetimes for a detected time span relative to `base_date`.
pub fn generate_time_span(
    span_type: &str,
    direction: &str,
    number: u32,
    base_date: NaiveDateTime,
    settings: &Settings,
) -> Option<(NaiveDateTime, NaiveDateTime)> {
    let past = direction == "past";
    if span_type == "week" {
        let ws = week_start(base_date, settings.default_start_of_week);
        let (s_dt, e_dt) = if past {
            (ws.checked_sub_signed(Duration::days(7))?, ws.checked_sub_signed(Duration::days(1))?)
        } else {
            let start = ws.checked_add_signed(Duration::days(7))?;
            (start, start.checked_add_signed(Duration::days(6))?)
        };
        if !(1..=9999).contains(&s_dt.year()) || !(1..=9999).contains(&e_dt.year()) {
            return None;
        }
        return Some((s_dt, e_dt));
    }

    let sign: i8 = if past { -1 } else { 1 };
    let other = match span_type {
        "month" => apply_relative_offset(
            base_date,
            sign,
            settings.default_days_in_month as f64,
            TimeUnit::Day,
        )?,
        "days" => apply_relative_offset(base_date, sign, number as f64, TimeUnit::Day)?,
        "weeks" => apply_relative_offset(base_date, sign, number as f64, TimeUnit::Week)?,
        _ => apply_relative_offset(base_date, sign, number as f64, TimeUnit::Month)?,
    };
    if !(1..=9999).contains(&other.year()) || !(1..=9999).contains(&base_date.year()) {
        return None;
    }

    if past {
        Some((other, base_date))
    } else {
        Some((base_date, other))
    }
}
