use chrono::{NaiveDateTime, NaiveTime, TimeZone};
use dateoxide_core::settings::{
    DateData, DateOrder, DatePart, ParsedDateTime, ParserKind, PreferDatesFrom, PreferDayOfMonth,
    PreferMonthOfYear, Settings, StartOfWeek, StrictDateOrder,
};
use dateoxide_core::timezone::TzSpec;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyList, PyString, PyTuple};
use rustc_hash::FxHashMap;
use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

static PART_OF_DAY_CLS: OnceLock<Py<PyAny>> = OnceLock::new();

pyo3::create_exception!(_dateoxide, SettingValidationError, PyValueError);

const ALLOWED_SETTINGS: &[&str] = &[
    "DATE_ORDER",
    "PREFER_LOCALE_DATE_ORDER",
    "STRICT_DATE_ORDER",
    "TIMEZONE",
    "TO_TIMEZONE",
    "RETURN_AS_TIMEZONE_AWARE",
    "TIMEZONE_ABBREVIATIONS",
    "PREFER_DAY_OF_MONTH",
    "PREFER_MONTH_OF_YEAR",
    "PREFER_DATES_FROM",
    "RELATIVE_BASE",
    "STRICT_PARSING",
    "REQUIRE_PARTS",
    "SKIP_TOKENS",
    "NORMALIZE",
    "DEFAULT_LANGUAGES",
    "USE_GIVEN_LANGUAGE_ORDER",
    "LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD",
    "RETURN_TIME_SPAN",
    "DEFAULT_START_OF_WEEK",
    "DEFAULT_DAYS_IN_MONTH",
    "RETURN_TIME_AS_PERIOD",
    "PARSERS",
    "IGNORE_SURROUNDING_TEXT",
    "CACHE_SIZE_LIMIT",
    "PARTS_OF_DAY",
];

fn parsed_to_py(py: Python<'_>, dt: ParsedDateTime) -> PyResult<PyObject> {
    match dt {
        ParsedDateTime::Naive(ndt) => Ok(ndt.into_pyobject(py)?.into_any().unbind()),
        ParsedDateTime::FixedOffset(fdt) => Ok(fdt.into_pyobject(py)?.into_any().unbind()),
        ParsedDateTime::Tz(tdt) => Ok(tdt.into_pyobject(py)?.into_any().unbind()),
    }
}

fn parsed_to_timestamp(dt: &ParsedDateTime) -> f64 {
    match dt {
        ParsedDateTime::Naive(ndt) => {
            if let Some(ldt) = chrono::Local.from_local_datetime(ndt).earliest() {
                ldt.timestamp() as f64 + (ldt.timestamp_subsec_nanos() as f64 / 1e9)
            } else {
                let u = ndt.and_utc();
                u.timestamp() as f64 + (u.timestamp_subsec_nanos() as f64 / 1e9)
            }
        }
        ParsedDateTime::FixedOffset(fdt) => {
            fdt.timestamp() as f64 + (fdt.timestamp_subsec_nanos() as f64 / 1e9)
        }
        ParsedDateTime::Tz(tdt) => {
            tdt.timestamp() as f64 + (tdt.timestamp_subsec_nanos() as f64 / 1e9)
        }
    }
}

fn extract_optional_strings(date_strings: &Bound<'_, PyAny>) -> PyResult<Vec<Option<String>>> {
    if date_strings.is_instance_of::<PyString>() || date_strings.is_instance_of::<PyBytes>() {
        return Err(PyTypeError::new_err(
            "date_strings must be an iterable of strings, not a bare str or bytes",
        ));
    }
    let iter = date_strings.try_iter()?;
    let mut out = Vec::with_capacity(date_strings.len().unwrap_or(0));
    for item in iter {
        let obj = item?;
        out.push(obj.extract::<Option<String>>()?);
    }
    Ok(out)
}

fn tzspec_to_py(py: Python<'_>, tz: TzSpec) -> PyResult<PyObject> {
    match tz {
        TzSpec::Fixed(off) => Ok(off.into_pyobject(py)?.into_any().unbind()),
        TzSpec::Iana(iana) => Ok(iana.into_pyobject(py)?.into_any().unbind()),
    }
}

fn extract_time_field(obj: &Bound<'_, PyAny>, attr: &str) -> PyResult<Option<NaiveTime>> {
    if let Ok(dict) = obj.downcast::<PyDict>() {
        if let Some(v) = dict.get_item(attr)? {
            if !v.is_none() {
                return Ok(Some(v.extract()?));
            }
        }
        return Ok(None);
    }
    if obj.hasattr(attr)? {
        let v = obj.getattr(attr)?;
        if !v.is_none() {
            return Ok(Some(v.extract()?));
        }
    }
    Ok(None)
}

fn validate_language_codes(codes: &[String]) -> PyResult<()> {
    let reg = dateoxide_core::locale::LocaleRegistry::global();
    let unknown: Vec<String> = codes
        .iter()
        .filter(|c| reg.resolve_lang_id(c).is_none())
        .map(|c| format!("'{}'", c))
        .collect();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(PyValueError::new_err(format!(
            "Unknown language(s): {}",
            unknown.join(", ")
        )))
    }
}

fn extract_settings(
    date_formats: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    locales: Option<Vec<String>>,
    region: Option<String>,
    use_given_order: bool,
    settings_obj: Option<&Bound<'_, PyAny>>,
) -> PyResult<Settings> {
    let mut cfg = Settings {
        date_formats,
        region,
        use_given_language_order: use_given_order,
        ..Default::default()
    };

    let mut combined_langs: Vec<String> = Vec::new();
    if let Some(ls) = languages {
        combined_langs.extend(ls);
    }
    if let Some(locs) = locales {
        combined_langs.extend(locs);
    }
    if !combined_langs.is_empty() {
        validate_language_codes(&combined_langs)?;
        cfg.languages = Some(combined_langs);
    }

    let Some(raw_settings) = settings_obj else {
        return Ok(cfg);
    };
    if raw_settings.is_none() {
        return Ok(cfg);
    }

    // Accept either dict or dateoxide.conf.Settings instance (via .as_effective_dict() / .as_dict())
    let owned_dict;
    let dict: &Bound<'_, PyDict> = if let Ok(d) = raw_settings.downcast::<PyDict>() {
        d
    } else if raw_settings.hasattr("as_effective_dict")? {
        let res = raw_settings.call_method0("as_effective_dict")?;
        owned_dict = res.downcast_into::<PyDict>().map_err(|_| {
            PyValueError::new_err("Settings.as_effective_dict() must return a dict")
        })?;
        &owned_dict
    } else if raw_settings.hasattr("as_dict")? {
        let res = raw_settings.call_method0("as_dict")?;
        owned_dict = res.downcast_into::<PyDict>().map_err(|_| {
            PyValueError::new_err("Settings.as_dict() must return a dict")
        })?;
        &owned_dict
    } else {
        return Err(PyTypeError::new_err(
            "settings must be a dict or a Settings instance",
        ));
    };

    for key in dict.keys() {
        let key_str: String = key.extract().map_err(|_| {
            SettingValidationError::new_err("Setting keys must be strings")
        })?;
        if !ALLOWED_SETTINGS.contains(&key_str.as_str()) {
            return Err(SettingValidationError::new_err(format!(
                "\"{}\" is not a valid setting",
                key_str
            )));
        }
    }

    if let Some(v) = dict.get_item("DATE_ORDER")? {
        if !v.is_none() {
            let s: String = v.extract().map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            cfg.date_order = Some(DateOrder::from_str(&s).ok_or_else(|| {
                SettingValidationError::new_err(format!("Invalid DATE_ORDER: '{}'", s))
            })?);
        }
    }
    if let Some(v) = dict.get_item("STRICT_DATE_ORDER")? {
        if !v.is_none() {
            let s: String = v.extract().map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            cfg.strict_date_order = StrictDateOrder::from_str(&s).ok_or_else(|| {
                SettingValidationError::new_err(format!("Invalid STRICT_DATE_ORDER: '{}'", s))
            })?;
        }
    }
    if let Some(v) = dict.get_item("PREFER_LOCALE_DATE_ORDER")? {
        if !v.is_none() {
            cfg.prefer_locale_date_order = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
        }
    }
    if let Some(v) = dict.get_item("PREFER_DATES_FROM")? {
        if !v.is_none() {
            let s: String = v.extract().map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            cfg.prefer_dates_from = PreferDatesFrom::from_str(&s).ok_or_else(|| {
                SettingValidationError::new_err(format!("Invalid PREFER_DATES_FROM: '{}'", s))
            })?;
        }
    }
    if let Some(v) = dict.get_item("PREFER_DAY_OF_MONTH")? {
        if !v.is_none() {
            let s: String = v.extract().map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            cfg.prefer_day_of_month = PreferDayOfMonth::from_str(&s).ok_or_else(|| {
                SettingValidationError::new_err(format!("Invalid PREFER_DAY_OF_MONTH: '{}'", s))
            })?;
        }
    }
    if let Some(v) = dict.get_item("PREFER_MONTH_OF_YEAR")? {
        if !v.is_none() {
            let s: String = v.extract().map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            cfg.prefer_month_of_year = PreferMonthOfYear::from_str(&s).ok_or_else(|| {
                SettingValidationError::new_err(format!("Invalid PREFER_MONTH_OF_YEAR: '{}'", s))
            })?;
        }
    }
    if let Some(v) = dict.get_item("STRICT_PARSING")? {
        if !v.is_none() {
            cfg.strict_parsing = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
        }
    }
    if let Some(v) = dict.get_item("RELATIVE_BASE")? {
        if !v.is_none() {
            if let Ok(b) = v.extract::<bool>() {
                if b {
                    return Err(SettingValidationError::new_err("RELATIVE_BASE cannot be True"));
                }
            } else if let Ok(ndt) = v.extract::<NaiveDateTime>() {
                cfg.relative_base = Some(ndt);
            } else if let Ok(fdt) = v.extract::<chrono::DateTime<chrono::FixedOffset>>() {
                cfg.relative_base = Some(fdt.naive_local());
            } else if v.hasattr("replace")? {
                let kw = PyDict::new(v.py());
                kw.set_item("tzinfo", v.py().None())?;
                let naive_py = v.call_method("replace", (), Some(&kw)).map_err(|e| {
                    SettingValidationError::new_err(format!("Invalid RELATIVE_BASE: {}", e))
                })?;
                let ndt: NaiveDateTime = naive_py.extract().map_err(|e| {
                    SettingValidationError::new_err(format!("Invalid RELATIVE_BASE: {}", e))
                })?;
                cfg.relative_base = Some(ndt);
            } else {
                return Err(SettingValidationError::new_err(
                    "RELATIVE_BASE must be a datetime object",
                ));
            }
        }
    }
    if let Some(v) = dict.get_item("TIMEZONE")? {
        if !v.is_none() {
            let s: String = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            if !s.eq_ignore_ascii_case("local") {
                cfg.timezone = Some(s);
            }
        }
    }
    if let Some(v) = dict.get_item("TO_TIMEZONE")? {
        if !v.is_none() {
            if let Ok(b) = v.extract::<bool>() {
                if b {
                    return Err(SettingValidationError::new_err("TO_TIMEZONE cannot be True"));
                }
            } else {
                cfg.to_timezone = Some(
                    v.extract()
                        .map_err(|e| SettingValidationError::new_err(e.to_string()))?,
                );
            }
        }
    }
    if let Some(v) = dict.get_item("RETURN_AS_TIMEZONE_AWARE")? {
        if !v.is_none() {
            if let Ok(b) = v.extract::<bool>() {
                cfg.return_as_timezone_aware = Some(b);
            } else if let Ok(s) = v.extract::<String>() {
                if !s.eq_ignore_ascii_case("default") {
                    return Err(SettingValidationError::new_err(format!(
                        "Invalid RETURN_AS_TIMEZONE_AWARE: '{}'",
                        s
                    )));
                }
            } else {
                return Err(SettingValidationError::new_err(
                    "RETURN_AS_TIMEZONE_AWARE must be a bool or 'default'",
                ));
            }
        }
    }
    if let Some(v) = dict.get_item("TIMEZONE_ABBREVIATIONS")? {
        if !v.is_none() {
            let map: std::collections::HashMap<String, String> = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            for (k, val) in map {
                cfg.timezone_abbreviations.insert(k.to_ascii_uppercase(), val);
            }
        }
    }
    if let Some(v) = dict.get_item("DEFAULT_LANGUAGES")? {
        if !v.is_none() {
            if v.is_instance_of::<PyString>() || v.is_instance_of::<PyBytes>() {
                return Err(SettingValidationError::new_err(
                    "DEFAULT_LANGUAGES must be a sequence of language codes, not str/bytes",
                ));
            }
            let def_langs: Vec<String> = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            validate_language_codes(&def_langs)?;
            cfg.default_languages = def_langs;
        }
    }
    if let Some(v) = dict.get_item("USE_GIVEN_LANGUAGE_ORDER")? {
        if !v.is_none() {
            cfg.use_given_language_order = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
        }
    }
    if let Some(v) = dict.get_item("LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD")? {
        if !v.is_none() {
            if v.is_instance_of::<PyBool>() {
                return Err(SettingValidationError::new_err(
                    "LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD cannot be a bool",
                ));
            }
            let val: f64 = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            if !val.is_finite() || !(0.0..=1.0).contains(&val) {
                return Err(SettingValidationError::new_err(format!(
                    "Invalid LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD: {}",
                    val
                )));
            }
            cfg.language_detection_confidence_threshold = val;
        }
    }
    if let Some(v) = dict.get_item("REQUIRE_PARTS")? {
        if !v.is_none() {
            if v.is_instance_of::<PyString>() || v.is_instance_of::<PyBytes>() {
                return Err(SettingValidationError::new_err(
                    "REQUIRE_PARTS must be a sequence of part names, not str/bytes",
                ));
            }
            let parts: Vec<String> = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            for p in parts {
                let dp = DatePart::from_str(&p).ok_or_else(|| {
                    SettingValidationError::new_err(format!("Invalid REQUIRE_PARTS item: '{}'", p))
                })?;
                cfg.require_parts.push(dp);
            }
        }
    }
    if let Some(v) = dict.get_item("NORMALIZE")? {
        if !v.is_none() {
            cfg.normalize = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
        }
    }
    if let Some(v) = dict.get_item("RETURN_TIME_AS_PERIOD")? {
        if !v.is_none() {
            cfg.return_time_as_period = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
        }
    }
    if let Some(v) = dict.get_item("RETURN_TIME_SPAN")? {
        if !v.is_none() {
            cfg.return_time_span = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
        }
    }
    if let Some(v) = dict.get_item("DEFAULT_START_OF_WEEK")? {
        if !v.is_none() {
            let s: String = v.extract().map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            cfg.default_start_of_week = StartOfWeek::from_str(&s).ok_or_else(|| {
                SettingValidationError::new_err(format!("Invalid DEFAULT_START_OF_WEEK: '{}'", s))
            })?;
        }
    }
    if let Some(v) = dict.get_item("DEFAULT_DAYS_IN_MONTH")? {
        if !v.is_none() {
            if v.is_instance_of::<PyBool>() {
                return Err(SettingValidationError::new_err(
                    "DEFAULT_DAYS_IN_MONTH cannot be a bool",
                ));
            }
            let d: i64 = v.extract().map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            if !(1..=31).contains(&d) {
                return Err(SettingValidationError::new_err(format!(
                    "Invalid DEFAULT_DAYS_IN_MONTH: {} (must be 1..=31)",
                    d
                )));
            }
            cfg.default_days_in_month = d as u32;
        }
    }
    if let Some(v) = dict.get_item("IGNORE_SURROUNDING_TEXT")? {
        if !v.is_none() {
            cfg.ignore_surrounding_text = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
        }
    }
    if let Some(v) = dict.get_item("SKIP_TOKENS")? {
        if !v.is_none() {
            if v.is_instance_of::<PyString>() || v.is_instance_of::<PyBytes>() {
                return Err(SettingValidationError::new_err(
                    "SKIP_TOKENS must be a sequence of strings, not str/bytes",
                ));
            }
            cfg.skip_tokens = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
        }
    }
    if let Some(v) = dict.get_item("PARSERS")? {
        if !v.is_none() {
            if v.is_instance_of::<PyString>() || v.is_instance_of::<PyBytes>() {
                return Err(SettingValidationError::new_err(
                    "PARSERS must be a sequence of parser names, not str/bytes",
                ));
            }
            let p_strs: Vec<String> = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            let mut parsed_kinds = Vec::with_capacity(p_strs.len());
            for ps in p_strs {
                let kind = ParserKind::from_str(&ps).ok_or_else(|| {
                    SettingValidationError::new_err(format!("Invalid PARSERS item: '{}'", ps))
                })?;
                parsed_kinds.push(kind);
            }
            cfg.parsers = Some(parsed_kinds);
        }
    }
    if let Some(v) = dict.get_item("PARTS_OF_DAY")? {
        if !v.is_none() {
            if let Some(t) = extract_time_field(&v, "early_morning")
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?
            {
                cfg.parts_of_day.early_morning = t;
            }
            if let Some(t) = extract_time_field(&v, "morning")
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?
            {
                cfg.parts_of_day.morning = t;
            }
            if let Some(t) = extract_time_field(&v, "afternoon")
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?
            {
                cfg.parts_of_day.afternoon = t;
            }
            if let Some(t) = extract_time_field(&v, "evening")
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?
            {
                cfg.parts_of_day.evening = t;
            }
            if let Some(t) = extract_time_field(&v, "night")
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?
            {
                cfg.parts_of_day.night = t;
            }
        }
    }
    if let Some(v) = dict.get_item("CACHE_SIZE_LIMIT")? {
        if !v.is_none() {
            if v.is_instance_of::<PyBool>() {
                return Err(SettingValidationError::new_err(
                    "CACHE_SIZE_LIMIT cannot be a bool",
                ));
            }
            let limit: isize = v
                .extract()
                .map_err(|e| SettingValidationError::new_err(e.to_string()))?;
            if limit < 0 {
                return Err(SettingValidationError::new_err(format!(
                    "Invalid CACHE_SIZE_LIMIT: {} (must be >= 0)",
                    limit
                )));
            }
            cfg.cache_size_limit = limit as usize;
        }
    }

    if let Some(ref tz) = cfg.timezone {
        if dateoxide_core::timezone::parse_tz_spec_with_abbrevs(tz, &cfg.timezone_abbreviations)
            .is_none()
        {
            return Err(SettingValidationError::new_err(format!(
                "Invalid TIMEZONE: '{}'",
                tz
            )));
        }
    }
    if let Some(ref tz) = cfg.to_timezone {
        if dateoxide_core::timezone::parse_tz_spec_with_abbrevs(tz, &cfg.timezone_abbreviations)
            .is_none()
        {
            return Err(SettingValidationError::new_err(format!(
                "Invalid TO_TIMEZONE: '{}'",
                tz
            )));
        }
    }

    Ok(cfg)
}

fn call_detect_languages(
    py: Python<'_>,
    func: &PyObject,
    text: &str,
    confidence_threshold: f64,
) -> PyResult<Option<Vec<String>>> {
    let bound = func.bind(py);
    let res = match bound.call1((text, confidence_threshold)) {
        Ok(r) => r,
        Err(err) if err.is_instance_of::<PyTypeError>(py) => bound.call1((text,))?,
        Err(err) => return Err(err),
    };
    if res.is_none() {
        return Ok(None);
    }
    if res.is_instance_of::<PyString>() || res.is_instance_of::<PyBytes>() {
        return Err(PyTypeError::new_err(
            "detect_languages_function must return a sequence of language codes, not a bare str",
        ));
    }
    let langs: Vec<String> = res.extract()?;
    if langs.is_empty() {
        Ok(None)
    } else {
        Ok(Some(langs))
    }
}

fn datedata_to_py(py: Python<'_>, data: DateData) -> PyResult<PyDateData> {
    let date_obj = match data.date_obj {
        Some(dt) => Some(parsed_to_py(py, dt)?),
        None => None,
    };
    let part_of_day = match data.part_of_day {
        Some(pod) => {
            let enum_cls = if let Some(cached) = PART_OF_DAY_CLS.get() {
                cached.bind(py).clone()
            } else {
                let imported = py.import("dateoxide.conf")?.getattr("PartOfDay")?.unbind();
                let _ = PART_OF_DAY_CLS.set(imported);
                PART_OF_DAY_CLS.get().unwrap().bind(py).clone()
            };
            Some(enum_cls.call1((pod.as_str(),))?.unbind())
        }
        None => None,
    };
    Ok(PyDateData {
        date_obj,
        period: data.period.as_str().to_string(),
        locale: data.locale,
        parts_vec: data.parts.into_iter().map(|s| s.to_string()).collect(),
        part_of_day,
    })
}

#[pyclass(name = "DateData", module = "dateoxide.date")]
pub struct PyDateData {
    #[pyo3(get)]
    pub date_obj: Option<PyObject>,
    #[pyo3(get)]
    pub period: String,
    #[pyo3(get)]
    pub locale: Option<String>,
    parts_vec: Vec<String>,
    #[pyo3(get)]
    pub part_of_day: Option<PyObject>,
}

fn eq_opt_py(py: Python<'_>, a: &Option<PyObject>, b: &Option<PyObject>) -> PyResult<bool> {
    match (a, b) {
        (None, None) => Ok(true),
        (Some(x), Some(y)) => x.bind(py).eq(y.bind(py)),
        _ => Ok(false),
    }
}

#[pyfunction]
fn _unpickle_datedata(
    date_obj: Option<PyObject>,
    period: String,
    locale: Option<String>,
    parts: Vec<String>,
    part_of_day: Option<PyObject>,
) -> PyDateData {
    PyDateData {
        date_obj,
        period,
        locale,
        parts_vec: parts,
        part_of_day,
    }
}

#[pymethods]
impl PyDateData {
    #[new]
    #[pyo3(signature = (*, date_obj=None, period=None, locale=None, parts=None, part_of_day=None))]
    fn new(
        date_obj: Option<PyObject>,
        period: Option<String>,
        locale: Option<String>,
        parts: Option<Vec<String>>,
        part_of_day: Option<PyObject>,
    ) -> Self {
        Self {
            date_obj,
            period: period.unwrap_or_else(|| "day".to_string()),
            locale,
            parts_vec: parts.unwrap_or_default(),
            part_of_day,
        }
    }

    #[getter]
    fn parts(&self, py: Python<'_>) -> PyResult<Py<PyTuple>> {
        Ok(PyTuple::new(py, &self.parts_vec)?.unbind())
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        match key {
            "date_obj" => Ok(self
                .date_obj
                .as_ref()
                .map(|o| o.clone_ref(py))
                .unwrap_or_else(|| py.None())),
            "period" => Ok(self.period.clone().into_pyobject(py)?.into_any().unbind()),
            "locale" => Ok(self.locale.clone().into_pyobject(py)?.into_any().unbind()),
            "parts" => Ok(self.parts(py)?.into_any()),
            "part_of_day" => Ok(self
                .part_of_day
                .as_ref()
                .map(|o| o.clone_ref(py))
                .unwrap_or_else(|| py.None())),
            _ => Err(pyo3::exceptions::PyKeyError::new_err(key.to_string())),
        }
    }

    #[pyo3(signature = (key, default=None))]
    fn get(&self, py: Python<'_>, key: &str, default: Option<PyObject>) -> PyResult<PyObject> {
        match self.__getitem__(py, key) {
            Ok(v) => Ok(v),
            Err(_) => Ok(default.unwrap_or_else(|| py.None())),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let dt_repr = match &self.date_obj {
            Some(obj) => obj.bind(py).repr()?.to_string(),
            None => "None".to_string(),
        };
        let loc_repr = match &self.locale {
            Some(l) => format!("'{}'", l),
            None => "None".to_string(),
        };
        let parts_repr = format!("({})", self.parts_vec.iter().map(|p| format!("'{}'", p)).collect::<Vec<_>>().join(", "));
        Ok(format!(
            "DateData(date_obj={}, period='{}', locale={}, parts={})",
            dt_repr, self.period, loc_repr, parts_repr
        ))
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<PyObject> {
        let Ok(o) = other.extract::<PyRef<'_, PyDateData>>() else {
            return Ok(py.NotImplemented());
        };
        let is_eq = self.period == o.period
            && self.locale == o.locale
            && self.parts_vec == o.parts_vec
            && eq_opt_py(py, &self.date_obj, &o.date_obj)?
            && eq_opt_py(py, &self.part_of_day, &o.part_of_day)?;
        Ok(is_eq.into_pyobject(py)?.to_owned().into_any().unbind())
    }

    fn __reduce__(&self, py: Python<'_>) -> PyResult<(PyObject, PyObject)> {
        let cls_fn = py
            .import("dateoxide._dateoxide")?
            .getattr("_unpickle_datedata")?
            .unbind();
        let dt_obj = self
            .date_obj
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None());
        let pod_obj = self
            .part_of_day
            .as_ref()
            .map(|o| o.clone_ref(py))
            .unwrap_or_else(|| py.None());
        let args = (
            dt_obj,
            self.period.clone(),
            self.locale.clone(),
            self.parts_vec.clone(),
            pod_obj,
        )
            .into_pyobject(py)?
            .into_any()
            .unbind();
        Ok((cls_fn, args))
    }
}

#[derive(Default)]
struct ParserCache {
    map: FxHashMap<String, DateData>,
    order: VecDeque<String>,
}

/// Reusable, zero-overhead configured parser (avoids re-parsing settings dict per call).
#[pyclass(name = "DateDataParser", module = "dateoxide.date")]
pub struct PyDateDataParser {
    settings: Settings,
    detect_languages_function: Option<PyObject>,
    try_previous_locales: bool,
    previous_locales: Mutex<Vec<String>>,
    cache: Mutex<ParserCache>,
}

impl PyDateDataParser {
    fn record_previous_locale(&self, locale: Option<&str>) {
        if !self.try_previous_locales {
            return;
        }
        let Some(loc) = locale else {
            return;
        };
        if loc.is_empty() {
            return;
        }
        if let Ok(mut prev) = self.previous_locales.lock() {
            prev.retain(|x| x != loc);
            prev.insert(0, loc.to_string());
            prev.truncate(8);
        }
    }

    fn build_previous_locales_config(&self, base_cfg: &Settings, prev: &[String]) -> Settings {
        let mut c = base_cfg.clone();
        c.use_given_language_order = true;
        match base_cfg.languages {
            None => {
                c.languages = Some(prev.to_vec());
            }
            Some(ref explicit) => {
                let mut reordered = Vec::with_capacity(explicit.len());
                for p in prev {
                    for e in explicit {
                        let base_e = e.split(['-', '_']).next().unwrap_or(e);
                        if (e.eq_ignore_ascii_case(p) || base_e.eq_ignore_ascii_case(p))
                            && !reordered.contains(e)
                        {
                            reordered.push(e.clone());
                        }
                    }
                }
                for e in explicit {
                    if !reordered.contains(e) {
                        reordered.push(e.clone());
                    }
                }
                c.languages = Some(reordered);
            }
        }
        c
    }

    fn resolve_date_data(
        &self,
        py: Python<'_>,
        date_string: &str,
        date_formats: Option<Vec<String>>,
    ) -> PyResult<DateData> {
        let mut cfg_owned: Option<Settings> = None;
        if let Some(fmts) = date_formats {
            let mut c = self.settings.clone();
            c.date_formats = Some(fmts);
            cfg_owned = Some(c);
        }
        if self.settings.languages.is_none() {
            if let Some(ref func) = self.detect_languages_function {
                if let Some(detected) = call_detect_languages(
                    py,
                    func,
                    date_string,
                    self.settings.language_detection_confidence_threshold,
                )? {
                    let c = cfg_owned.get_or_insert_with(|| self.settings.clone());
                    c.languages = Some(detected);
                }
            }
        }

        if self.try_previous_locales {
            let effective_cfg = cfg_owned.as_ref().unwrap_or(&self.settings);
            let prev: Vec<String> = self
                .previous_locales
                .lock()
                .map(|g| g.clone())
                .unwrap_or_default();
            if !prev.is_empty() {
                let prev_cfg = self.build_previous_locales_config(effective_cfg, &prev);
                let res = py.allow_threads(|| dateoxide_core::get_date_data(date_string, &prev_cfg));
                if res.date_obj.is_some() {
                    self.record_previous_locale(res.locale.as_deref());
                    return Ok(res);
                }
            }
            let res = py.allow_threads(|| dateoxide_core::get_date_data(date_string, effective_cfg));
            if res.date_obj.is_some() {
                self.record_previous_locale(res.locale.as_deref());
            }
            return Ok(res);
        }

        if let Some(ref cfg) = cfg_owned {
            return Ok(py.allow_threads(|| dateoxide_core::get_date_data(date_string, cfg)));
        }

        let limit = self.settings.cache_size_limit;
        if limit > 0 && self.settings.relative_base.is_some() {
            if let Ok(guard) = self.cache.lock() {
                if let Some(cached) = guard.map.get(date_string) {
                    return Ok(cached.clone());
                }
            }
            let data = py.allow_threads(|| dateoxide_core::get_date_data(date_string, &self.settings));
            if let Ok(mut guard) = self.cache.lock() {
                if !guard.map.contains_key(date_string) {
                    while guard.order.len() >= limit {
                        if let Some(oldest) = guard.order.pop_front() {
                            guard.map.remove(&oldest);
                        } else {
                            break;
                        }
                    }
                    let key = date_string.to_string();
                    guard.order.push_back(key.clone());
                    guard.map.insert(key, data.clone());
                }
            }
            return Ok(data);
        }

        Ok(py.allow_threads(|| dateoxide_core::get_date_data(date_string, &self.settings)))
    }
}

#[pymethods]
impl PyDateDataParser {
    #[new]
    #[pyo3(signature = (languages=None, locales=None, region=None, try_previous_locales=false, use_given_order=false, settings=None, detect_languages_function=None))]
    fn new(
        languages: Option<Vec<String>>,
        locales: Option<Vec<String>>,
        region: Option<String>,
        try_previous_locales: bool,
        use_given_order: bool,
        settings: Option<&Bound<'_, PyAny>>,
        detect_languages_function: Option<PyObject>,
    ) -> PyResult<Self> {
        let cfg = extract_settings(None, languages, locales, region, use_given_order, settings)?;
        Ok(Self {
            settings: cfg,
            detect_languages_function,
            try_previous_locales,
            previous_locales: Mutex::new(Vec::new()),
            cache: Mutex::new(ParserCache::default()),
        })
    }

    #[pyo3(signature = (date_string, date_formats=None))]
    fn get_date_data(
        &self,
        py: Python<'_>,
        date_string: &str,
        date_formats: Option<Vec<String>>,
    ) -> PyResult<PyDateData> {
        let data = self.resolve_date_data(py, date_string, date_formats)?;
        datedata_to_py(py, data)
    }

    #[pyo3(signature = (date_string, date_formats=None))]
    fn get_date_tuple(
        &self,
        py: Python<'_>,
        date_string: &str,
        date_formats: Option<Vec<String>>,
    ) -> PyResult<Py<PyTuple>> {
        let dd = self.get_date_data(py, date_string, date_formats)?;
        let dt_obj = dd.date_obj.unwrap_or_else(|| py.None());
        let loc_obj = dd
            .locale
            .map(|l| l.into_pyobject(py).unwrap().into_any().unbind())
            .unwrap_or_else(|| py.None());
        let period_obj = dd.period.into_pyobject(py)?.into_any().unbind();
        Ok(PyTuple::new(py, [dt_obj, period_obj, loc_obj])?.unbind())
    }

    #[pyo3(signature = (date_string, date_formats=None))]
    fn parse(
        &self,
        py: Python<'_>,
        date_string: &str,
        date_formats: Option<Vec<String>>,
    ) -> PyResult<Option<PyObject>> {
        let data = self.resolve_date_data(py, date_string, date_formats)?;
        match data.date_obj {
            Some(dt) => Ok(Some(parsed_to_py(py, dt)?)),
            None => Ok(None),
        }
    }

    fn parse_batch(
        &self,
        py: Python<'_>,
        date_strings: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyList>> {
        let date_strings = extract_optional_strings(date_strings)?;
        let results = match (&self.settings.languages, &self.detect_languages_function) {
            (None, Some(func)) => {
            let mut out = Vec::with_capacity(date_strings.len());
            for item in &date_strings {
                let Some(s) = item.as_deref() else {
                    out.push(None);
                    continue;
                };
                let mut local_cfg = None;
                if let Some(detected) = call_detect_languages(
                    py,
                    func,
                    s,
                    self.settings.language_detection_confidence_threshold,
                )? {
                    let mut c = self.settings.clone();
                    c.languages = Some(detected);
                    local_cfg = Some(c);
                }
                let cfg_ref = local_cfg.as_ref().unwrap_or(&self.settings);
                out.push(py.allow_threads(|| dateoxide_core::parse_with_settings(s, cfg_ref)));
            }
            out
            }
            _ => {
                let cfg = &self.settings;
                py.allow_threads(|| dateoxide_core::parse_batch_opt(&date_strings, cfg))
            }
        };
        let py_items: Vec<PyObject> = results
            .into_iter()
            .map(|opt| match opt {
                Some(dt) => parsed_to_py(py, dt),
                None => Ok(py.None()),
            })
            .collect::<PyResult<_>>()?;
        Ok(PyList::new(py, py_items)?.unbind())
    }

    fn parse_date_range(
        &self,
        py: Python<'_>,
        date_range_string: &str,
    ) -> PyResult<Option<(PyObject, PyObject)>> {
        let mut cfg_owned: Option<Settings> = None;
        if self.settings.languages.is_none() {
            if let Some(ref func) = self.detect_languages_function {
                if let Some(detected) = call_detect_languages(
                    py,
                    func,
                    date_range_string,
                    self.settings.language_detection_confidence_threshold,
                )? {
                    let mut c = self.settings.clone();
                    c.languages = Some(detected);
                    cfg_owned = Some(c);
                }
            }
        }
        let cfg_ref = cfg_owned.as_ref().unwrap_or(&self.settings);
        match py.allow_threads(|| dateoxide_core::parse_date_range(date_range_string, cfg_ref)) {
            Some((start, end)) => Ok(Some((parsed_to_py(py, start)?, parsed_to_py(py, end)?))),
            None => Ok(None),
        }
    }
}

/// Drop-in replacement for `dateparser.parse(...)`.
#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (date_string, date_formats=None, languages=None, locales=None, region=None, settings=None, detect_languages_function=None))]
fn parse(
    py: Python<'_>,
    date_string: &str,
    date_formats: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    locales: Option<Vec<String>>,
    region: Option<String>,
    settings: Option<&Bound<'_, PyAny>>,
    detect_languages_function: Option<PyObject>,
) -> PyResult<Option<PyObject>> {
    let mut cfg = extract_settings(date_formats, languages, locales, region, false, settings)?;
    if cfg.languages.is_none() {
        if let Some(ref func) = detect_languages_function {
            if let Some(detected) = call_detect_languages(
                py,
                func,
                date_string,
                cfg.language_detection_confidence_threshold,
            )? {
                cfg.languages = Some(detected);
            }
        }
    }
    let parsed = py.allow_threads(|| dateoxide_core::parse_with_settings(date_string, &cfg));
    match parsed {
        Some(dt) => Ok(Some(parsed_to_py(py, dt)?)),
        None => Ok(None),
    }
}

/// High-throughput, GIL-free parallel batch parser powered by Rayon (supports iterables and nullable strings).
#[pyfunction]
#[pyo3(signature = (date_strings, date_formats=None, languages=None, locales=None, region=None, settings=None))]
fn parse_batch(
    py: Python<'_>,
    date_strings: &Bound<'_, PyAny>,
    date_formats: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    locales: Option<Vec<String>>,
    region: Option<String>,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyList>> {
    let date_strings = extract_optional_strings(date_strings)?;
    let cfg = extract_settings(date_formats, languages, locales, region, false, settings)?;
    let results = py.allow_threads(|| dateoxide_core::parse_batch_opt(&date_strings, &cfg));
    let py_items: Vec<PyObject> = results
        .into_iter()
        .map(|opt| match opt {
            Some(dt) => parsed_to_py(py, dt),
            None => Ok(py.None()),
        })
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, py_items)?.unbind())
}

/// High-throughput batch parser returning Unix epoch float timestamps (or `None`).
#[pyfunction]
#[pyo3(signature = (date_strings, date_formats=None, languages=None, locales=None, region=None, settings=None))]
fn parse_batch_timestamps(
    py: Python<'_>,
    date_strings: &Bound<'_, PyAny>,
    date_formats: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    locales: Option<Vec<String>>,
    region: Option<String>,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<Py<PyList>> {
    let date_strings = extract_optional_strings(date_strings)?;
    let cfg = extract_settings(date_formats, languages, locales, region, false, settings)?;
    let results: Vec<Option<f64>> = py.allow_threads(|| {
        dateoxide_core::parse_batch_opt(&date_strings, &cfg)
            .into_iter()
            .map(|opt| opt.as_ref().map(parsed_to_timestamp))
            .collect()
    });
    let py_items: Vec<PyObject> = results
        .into_iter()
        .map(|opt| match opt {
            Some(ts) => Ok(ts.into_pyobject(py)?.into_any().unbind()),
            None => Ok(py.None()),
        })
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, py_items)?.unbind())
}

/// Parse natural-language or numeric date ranges into `(start, end)`.
#[pyfunction]
#[pyo3(signature = (date_range_string, languages=None, locales=None, region=None, settings=None))]
fn parse_date_range(
    py: Python<'_>,
    date_range_string: &str,
    languages: Option<Vec<String>>,
    locales: Option<Vec<String>>,
    region: Option<String>,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<Option<(PyObject, PyObject)>> {
    let cfg = extract_settings(None, languages, locales, region, false, settings)?;
    match py.allow_threads(|| dateoxide_core::parse_date_range(date_range_string, &cfg)) {
        Some((start, end)) => Ok(Some((parsed_to_py(py, start)?, parsed_to_py(py, end)?))),
        None => Ok(None),
    }
}

/// Drop-in replacement for `dateparser.search.search_dates(...)`.
#[allow(clippy::too_many_arguments)]
#[pyfunction]
#[pyo3(signature = (text, languages=None, settings=None, add_detected_language=false, detect_languages_function=None, strategy="split", date_formats=None, add_period=false, return_spans=false))]
fn search_dates(
    py: Python<'_>,
    text: &str,
    languages: Option<Vec<String>>,
    settings: Option<&Bound<'_, PyAny>>,
    add_detected_language: bool,
    detect_languages_function: Option<PyObject>,
    strategy: &str,
    date_formats: Option<Vec<String>>,
    add_period: bool,
    return_spans: bool,
) -> PyResult<Option<Py<PyList>>> {
    if !matches!(strategy, "split" | "ngram") {
        return Err(PyValueError::new_err(format!(
            "Invalid strategy: '{}'. Expected 'split' or 'ngram'.",
            strategy
        )));
    }
    let mut cfg = extract_settings(date_formats, languages, None, None, false, settings)?;
    if cfg.languages.is_none() {
        if let Some(ref func) = detect_languages_function {
            if let Some(detected) = call_detect_languages(
                py,
                func,
                text,
                cfg.language_detection_confidence_threshold,
            )? {
                cfg.languages = Some(detected);
            }
        }
    }
    let found = py.allow_threads(|| dateoxide_core::search_dates_full(text, &cfg));
    if found.is_empty() {
        return Ok(None);
    }
    let items: Vec<PyObject> = found
        .into_iter()
        .map(|m| {
            let py_dt = parsed_to_py(py, m.date_obj)?;
            let period_s = m.period.as_str().to_string();
            let span = if return_spans {
                let end_b = m.end_byte.min(text.len());
                let start_b = m.start_byte.min(end_b);
                let sc = text[..start_b].chars().count();
                let ec = sc + text[start_b..end_b].chars().count();
                Some((sc, ec))
            } else {
                None
            };
            match (add_period, add_detected_language, span) {
                (false, false, None) => Ok((m.text, py_dt).into_pyobject(py)?.into_any().unbind()),
                (false, true, None) => Ok((m.text, py_dt, m.locale).into_pyobject(py)?.into_any().unbind()),
                (true, false, None) => Ok((m.text, py_dt, period_s).into_pyobject(py)?.into_any().unbind()),
                (true, true, None) => {
                    Ok((m.text, py_dt, period_s, m.locale).into_pyobject(py)?.into_any().unbind())
                }
                (false, false, Some(sp)) => Ok((m.text, py_dt, sp).into_pyobject(py)?.into_any().unbind()),
                (false, true, Some(sp)) => {
                    Ok((m.text, py_dt, m.locale, sp).into_pyobject(py)?.into_any().unbind())
                }
                (true, false, Some(sp)) => {
                    Ok((m.text, py_dt, period_s, sp).into_pyobject(py)?.into_any().unbind())
                }
                (true, true, Some(sp)) => {
                    Ok((m.text, py_dt, period_s, m.locale, sp).into_pyobject(py)?.into_any().unbind())
                }
            }
        })
        .collect::<PyResult<_>>()?;
    Ok(Some(PyList::new(py, items)?.unbind()))
}

#[pyfunction]
#[pyo3(signature = (date_string, languages=None, settings=None))]
fn parse_freshness(
    py: Python<'_>,
    date_string: &str,
    languages: Option<Vec<String>>,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyDateData> {
    let cfg = extract_settings(None, languages, None, None, false, settings)?;
    let dd = py.allow_threads(|| dateoxide_core::parse_freshness(date_string, &cfg));
    datedata_to_py(py, dd)
}

#[pyfunction]
#[pyo3(signature = (date_string, as_offset=true, timezone_abbreviations=None))]
fn pop_tz_offset_from_string(
    py: Python<'_>,
    date_string: &str,
    as_offset: bool,
    timezone_abbreviations: Option<&Bound<'_, PyDict>>,
) -> PyResult<(String, Option<PyObject>)> {
    let mut map = rustc_hash::FxHashMap::default();
    if let Some(d) = timezone_abbreviations {
        let extracted: std::collections::HashMap<String, String> = d.extract()?;
        for (k, v) in extracted {
            map.insert(k.to_ascii_uppercase(), v);
        }
    }
    let (cleaned, tz_pair) = dateoxide_core::timezone::pop_tz_token_and_offset(
        date_string,
        if map.is_empty() { None } else { Some(&map) },
    );
    let tz_py = match tz_pair {
        Some((raw_tok, tz)) => {
            if as_offset {
                Some(tzspec_to_py(py, tz)?)
            } else {
                Some(raw_tok.into_pyobject(py)?.into_any().unbind())
            }
        }
        None => None,
    };
    Ok((cleaned, tz_py))
}

#[pyfunction]
#[pyo3(signature = (source, settings=None))]
fn parse_jalali(
    py: Python<'_>,
    source: &str,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyDateData> {
    let cfg = extract_settings(None, None, None, None, false, settings)?;
    let dd = py.allow_threads(|| dateoxide_core::calendars::parse_jalali(source, &cfg));
    datedata_to_py(py, dd)
}

#[pyfunction]
#[pyo3(signature = (source, settings=None))]
fn parse_hijri(
    py: Python<'_>,
    source: &str,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyDateData> {
    let cfg = extract_settings(None, None, None, None, false, settings)?;
    let dd = py.allow_threads(|| dateoxide_core::calendars::parse_hijri(source, &cfg));
    datedata_to_py(py, dd)
}

#[pyfunction]
#[pyo3(signature = (source, settings=None))]
fn parse_minguo(
    py: Python<'_>,
    source: &str,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyDateData> {
    let cfg = extract_settings(None, None, None, None, false, settings)?;
    let dd = py.allow_threads(|| dateoxide_core::calendars::parse_minguo(source, &cfg));
    datedata_to_py(py, dd)
}

#[pyfunction]
#[pyo3(signature = (source, settings=None))]
fn parse_french_republican(
    py: Python<'_>,
    source: &str,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<PyDateData> {
    let cfg = extract_settings(None, None, None, None, false, settings)?;
    let dd = py.allow_threads(|| dateoxide_core::calendars::parse_french_republican(source, &cfg));
    datedata_to_py(py, dd)
}

#[pyfunction]
fn detect_time_span(py: Python<'_>, text: &str) -> PyResult<Option<Py<PyDict>>> {
    let Some(m) = dateoxide_core::time_spans::detect_time_span(text) else {
        return Ok(None);
    };
    let d = PyDict::new(py);
    d.set_item("type", m.span_type)?;
    d.set_item("direction", m.direction)?;
    d.set_item("matched_text", m.matched_text)?;
    let start_char = text[..m.start_pos].chars().count();
    let end_char = start_char + text[m.start_pos..m.end_pos].chars().count();
    d.set_item("start_pos", start_char)?;
    d.set_item("end_pos", end_char)?;
    if let Some(n) = m.number {
        d.set_item("number", n)?;
    }
    Ok(Some(d.unbind()))
}

#[pyfunction]
#[pyo3(signature = (span_info, base_date=None, settings=None))]
fn generate_time_span(
    py: Python<'_>,
    span_info: &Bound<'_, PyDict>,
    base_date: Option<&Bound<'_, PyAny>>,
    settings: Option<&Bound<'_, PyAny>>,
) -> PyResult<(PyObject, PyObject)> {
    let cfg = extract_settings(None, None, None, None, false, settings)?;
    let span_type: String = span_info
        .get_item("type")?
        .ok_or_else(|| PyValueError::new_err("span_info missing 'type'"))?
        .extract()?;
    let direction: String = span_info
        .get_item("direction")?
        .ok_or_else(|| PyValueError::new_err("span_info missing 'direction'"))?
        .extract()?;
    let number: u32 = match span_info.get_item("number")? {
        Some(v) if !v.is_none() => v.extract()?,
        _ => 1,
    };
    let (base, preserved_tz): (NaiveDateTime, Option<PyObject>) = match base_date {
        None => (chrono::Local::now().naive_local(), None),
        Some(bd) if bd.is_none() => (chrono::Local::now().naive_local(), None),
        Some(bd) => {
            if let Ok(ndt) = bd.extract::<NaiveDateTime>() {
                (ndt, None)
            } else if bd.hasattr("tzinfo")? && bd.hasattr("replace")? {
                let tzi = bd.getattr("tzinfo")?;
                let tz_opt = if tzi.is_none() { None } else { Some(tzi.unbind()) };
                let kw = PyDict::new(py);
                kw.set_item("tzinfo", py.None())?;
                let naive_py = bd.call_method("replace", (), Some(&kw))?;
                let ndt: NaiveDateTime = naive_py.extract()?;
                (ndt, tz_opt)
            } else {
                return Err(PyTypeError::new_err("base_date must be a datetime object or None"));
            }
        }
    };
    let (start, end) =
        dateoxide_core::time_spans::generate_time_span(&span_type, &direction, number, base, &cfg)
            .ok_or_else(|| PyValueError::new_err("invalid time span parameters"))?;
    let start_py = start.into_pyobject(py)?;
    let end_py = end.into_pyobject(py)?;
    if let Some(ref tz_obj) = preserved_tz {
        let kw = PyDict::new(py);
        kw.set_item("tzinfo", tz_obj.bind(py))?;
        Ok((
            start_py.call_method("replace", (), Some(&kw))?.unbind(),
            end_py.call_method("replace", (), Some(&kw))?.unbind(),
        ))
    } else {
        Ok((start_py.into_any().unbind(), end_py.into_any().unbind()))
    }
}

/// Return all 205 compiled locale language codes.
#[pyfunction]
fn supported_languages() -> Vec<String> {
    dateoxide_core::locale::LocaleRegistry::global()
        .locales
        .iter()
        .map(|l| l.code.clone())
        .collect()
}

#[pymodule]
fn _dateoxide(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("SettingValidationError", py.get_type::<SettingValidationError>())?;
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_batch, m)?)?;
    m.add_function(wrap_pyfunction!(parse_batch_timestamps, m)?)?;
    m.add_function(wrap_pyfunction!(parse_date_range, m)?)?;
    m.add_function(wrap_pyfunction!(search_dates, m)?)?;
    m.add_function(wrap_pyfunction!(parse_freshness, m)?)?;
    m.add_function(wrap_pyfunction!(pop_tz_offset_from_string, m)?)?;
    m.add_function(wrap_pyfunction!(parse_jalali, m)?)?;
    m.add_function(wrap_pyfunction!(parse_hijri, m)?)?;
    m.add_function(wrap_pyfunction!(parse_minguo, m)?)?;
    m.add_function(wrap_pyfunction!(parse_french_republican, m)?)?;
    m.add_function(wrap_pyfunction!(detect_time_span, m)?)?;
    m.add_function(wrap_pyfunction!(generate_time_span, m)?)?;
    m.add_function(wrap_pyfunction!(supported_languages, m)?)?;
    m.add_function(wrap_pyfunction!(_unpickle_datedata, m)?)?;
    m.add_class::<PyDateData>()?;
    m.add_class::<PyDateDataParser>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
