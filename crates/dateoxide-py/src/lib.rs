use chrono::NaiveDateTime;
use dateoxide_core::settings::{
    DateOrder, DatePart, ParsedDateTime, PreferDatesFrom, PreferDayOfMonth, PreferMonthOfYear,
    Settings,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

fn parsed_to_py(py: Python<'_>, dt: ParsedDateTime) -> PyResult<PyObject> {
    match dt {
        ParsedDateTime::Naive(ndt) => Ok(ndt.into_pyobject(py)?.into_any().unbind()),
        ParsedDateTime::FixedOffset(fdt) => Ok(fdt.into_pyobject(py)?.into_any().unbind()),
        ParsedDateTime::Tz(tdt) => Ok(tdt.into_pyobject(py)?.into_any().unbind()),
    }
}

fn extract_settings(
    date_formats: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    locales: Option<Vec<String>>,
    settings_dict: Option<&Bound<'_, PyDict>>,
) -> PyResult<Settings> {
    let mut cfg = Settings::default();
    cfg.date_formats = date_formats;

    // Combine explicit languages and locales (e.g. "en-US" -> "en" handled in registry)
    let mut combined_langs: Vec<String> = Vec::new();
    if let Some(ls) = languages {
        combined_langs.extend(ls);
    }
    if let Some(locs) = locales {
        combined_langs.extend(locs);
    }
    if !combined_langs.is_empty() {
        cfg.languages = Some(combined_langs);
    }

    if let Some(dict) = settings_dict {
        if let Some(v) = dict.get_item("DATE_ORDER")? {
            if !v.is_none() {
                let s: String = v.extract()?;
                cfg.date_order = Some(DateOrder::from_str(&s).ok_or_else(|| {
                    PyValueError::new_err(format!("Invalid DATE_ORDER: '{}'", s))
                })?);
            }
        }
        if let Some(v) = dict.get_item("PREFER_DATES_FROM")? {
            if !v.is_none() {
                let s: String = v.extract()?;
                cfg.prefer_dates_from = PreferDatesFrom::from_str(&s).ok_or_else(|| {
                    PyValueError::new_err(format!("Invalid PREFER_DATES_FROM: '{}'", s))
                })?;
            }
        }
        if let Some(v) = dict.get_item("PREFER_DAY_OF_MONTH")? {
            if !v.is_none() {
                let s: String = v.extract()?;
                cfg.prefer_day_of_month = PreferDayOfMonth::from_str(&s).ok_or_else(|| {
                    PyValueError::new_err(format!("Invalid PREFER_DAY_OF_MONTH: '{}'", s))
                })?;
            }
        }
        if let Some(v) = dict.get_item("PREFER_MONTH_OF_YEAR")? {
            if !v.is_none() {
                let s: String = v.extract()?;
                cfg.prefer_month_of_year = PreferMonthOfYear::from_str(&s).ok_or_else(|| {
                    PyValueError::new_err(format!("Invalid PREFER_MONTH_OF_YEAR: '{}'", s))
                })?;
            }
        }
        if let Some(v) = dict.get_item("STRICT_PARSING")? {
            if !v.is_none() {
                cfg.strict_parsing = v.extract()?;
            }
        }
        if let Some(v) = dict.get_item("RELATIVE_BASE")? {
            if !v.is_none() {
                let ndt: NaiveDateTime = v.extract()?;
                cfg.relative_base = Some(ndt);
            }
        }
        if let Some(v) = dict.get_item("TIMEZONE")? {
            if !v.is_none() {
                cfg.timezone = Some(v.extract()?);
            }
        }
        if let Some(v) = dict.get_item("TO_TIMEZONE")? {
            if !v.is_none() {
                cfg.to_timezone = Some(v.extract()?);
            }
        }
        if let Some(v) = dict.get_item("RETURN_AS_TIMEZONE_AWARE")? {
            if !v.is_none() {
                cfg.return_as_timezone_aware = Some(v.extract()?);
            }
        }
        if let Some(v) = dict.get_item("DEFAULT_LANGUAGES")? {
            if !v.is_none() {
                cfg.default_languages = v.extract()?;
            }
        }
        if let Some(v) = dict.get_item("REQUIRE_PARTS")? {
            if !v.is_none() {
                let parts: Vec<String> = v.extract()?;
                for p in parts {
                    let dp = DatePart::from_str(&p).ok_or_else(|| {
                        PyValueError::new_err(format!("Invalid REQUIRE_PARTS item: '{}'", p))
                    })?;
                    cfg.require_parts.push(dp);
                }
            }
        }
    }

    Ok(cfg)
}

#[pyclass(name = "DateData")]
pub struct PyDateData {
    #[pyo3(get)]
    pub date_obj: Option<PyObject>,
    #[pyo3(get)]
    pub period: String,
    #[pyo3(get)]
    pub locale: Option<String>,
}

#[pymethods]
impl PyDateData {
    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        match key {
            "date_obj" => Ok(self
                .date_obj
                .as_ref()
                .map(|o| o.clone_ref(py))
                .unwrap_or_else(|| py.None())),
            "period" => Ok(self.period.clone().into_pyobject(py)?.into_any().unbind()),
            "locale" => Ok(self.locale.clone().into_pyobject(py)?.into_any().unbind()),
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
        Ok(format!(
            "DateData(date_obj={}, period='{}', locale={})",
            dt_repr, self.period, loc_repr
        ))
    }
}

/// Reusable, zero-overhead configured parser (avoids re-parsing settings dict per call).
#[pyclass(name = "DateDataParser")]
pub struct PyDateDataParser {
    settings: Settings,
}

#[pymethods]
impl PyDateDataParser {
    #[new]
    #[pyo3(signature = (languages=None, locales=None, region=None, try_previous_locales=false, use_given_order=false, settings=None, detect_languages_function=None))]
    #[allow(unused_variables)]
    fn new(
        languages: Option<Vec<String>>,
        locales: Option<Vec<String>>,
        region: Option<String>,
        try_previous_locales: bool,
        use_given_order: bool,
        settings: Option<&Bound<'_, PyDict>>,
        detect_languages_function: Option<PyObject>,
    ) -> PyResult<Self> {
        let cfg = extract_settings(None, languages, locales, settings)?;
        Ok(Self { settings: cfg })
    }

    #[pyo3(signature = (date_string, date_formats=None))]
    fn get_date_data(
        &self,
        py: Python<'_>,
        date_string: &str,
        date_formats: Option<Vec<String>>,
    ) -> PyResult<PyDateData> {
        let data = if let Some(fmts) = date_formats {
            let mut cfg = self.settings.clone();
            cfg.date_formats = Some(fmts);
            dateoxide_core::get_date_data(date_string, &cfg)
        } else {
            dateoxide_core::get_date_data(date_string, &self.settings)
        };
        let date_obj = match data.date_obj {
            Some(dt) => Some(parsed_to_py(py, dt)?),
            None => None,
        };
        Ok(PyDateData {
            date_obj,
            period: data.period.as_str().to_string(),
            locale: data.locale,
        })
    }

    #[pyo3(signature = (date_string, date_formats=None))]
    fn parse(
        &self,
        py: Python<'_>,
        date_string: &str,
        date_formats: Option<Vec<String>>,
    ) -> PyResult<Option<PyObject>> {
        let parsed = if let Some(fmts) = date_formats {
            let mut cfg = self.settings.clone();
            cfg.date_formats = Some(fmts);
            dateoxide_core::parse_with_settings(date_string, &cfg)
        } else {
            dateoxide_core::parse_with_settings(date_string, &self.settings)
        };
        match parsed {
            Some(dt) => Ok(Some(parsed_to_py(py, dt)?)),
            None => Ok(None),
        }
    }

    fn parse_batch(&self, py: Python<'_>, date_strings: Vec<String>) -> PyResult<Py<PyList>> {
        let cfg = &self.settings;
        let results = py.allow_threads(|| dateoxide_core::parse_batch(&date_strings, cfg));
        let py_items: Vec<PyObject> = results
            .into_iter()
            .map(|opt| match opt {
                Some(dt) => parsed_to_py(py, dt),
                None => Ok(py.None()),
            })
            .collect::<PyResult<_>>()?;
        Ok(PyList::new(py, py_items)?.unbind())
    }
}

/// Drop-in replacement for `dateparser.parse(...)`.
#[pyfunction]
#[pyo3(signature = (date_string, date_formats=None, languages=None, locales=None, region=None, settings=None, detect_languages_function=None))]
#[allow(unused_variables)]
fn parse(
    py: Python<'_>,
    date_string: &str,
    date_formats: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    locales: Option<Vec<String>>,
    region: Option<String>,
    settings: Option<&Bound<'_, PyDict>>,
    detect_languages_function: Option<PyObject>,
) -> PyResult<Option<PyObject>> {
    let cfg = extract_settings(date_formats, languages, locales, settings)?;
    let parsed = dateoxide_core::parse_with_settings(date_string, &cfg);
    match parsed {
        Some(dt) => Ok(Some(parsed_to_py(py, dt)?)),
        None => Ok(None),
    }
}

/// High-throughput, GIL-free parallel batch parser powered by Rayon.
#[pyfunction]
#[pyo3(signature = (date_strings, date_formats=None, languages=None, locales=None, settings=None))]
fn parse_batch(
    py: Python<'_>,
    date_strings: Vec<String>,
    date_formats: Option<Vec<String>>,
    languages: Option<Vec<String>>,
    locales: Option<Vec<String>>,
    settings: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyList>> {
    let cfg = extract_settings(date_formats, languages, locales, settings)?;
    let results = py.allow_threads(|| dateoxide_core::parse_batch(&date_strings, &cfg));
    let py_items: Vec<PyObject> = results
        .into_iter()
        .map(|opt| match opt {
            Some(dt) => parsed_to_py(py, dt),
            None => Ok(py.None()),
        })
        .collect::<PyResult<_>>()?;
    Ok(PyList::new(py, py_items)?.unbind())
}

/// Drop-in replacement for `dateparser.search.search_dates(...)`.
#[pyfunction]
#[pyo3(signature = (text, languages=None, settings=None, add_detected_language=false))]
#[allow(unused_variables)]
fn search_dates(
    py: Python<'_>,
    text: &str,
    languages: Option<Vec<String>>,
    settings: Option<&Bound<'_, PyDict>>,
    add_detected_language: bool,
) -> PyResult<Option<Vec<(String, PyObject)>>> {
    let cfg = extract_settings(None, languages, None, settings)?;
    let found = py.allow_threads(|| dateoxide_core::search_dates(text, &cfg));
    if found.is_empty() {
        return Ok(None);
    }
    let out: Vec<(String, PyObject)> = found
        .into_iter()
        .map(|(s, dt)| Ok((s, parsed_to_py(py, dt)?)))
        .collect::<PyResult<_>>()?;
    Ok(Some(out))
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
fn _dateoxide(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_batch, m)?)?;
    m.add_function(wrap_pyfunction!(search_dates, m)?)?;
    m.add_function(wrap_pyfunction!(supported_languages, m)?)?;
    m.add_class::<PyDateData>()?;
    m.add_class::<PyDateDataParser>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
