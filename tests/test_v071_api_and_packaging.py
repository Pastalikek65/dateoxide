"""v0.7.1 Python API, Settings validation, stub parity, and packaging regression tests."""
from __future__ import annotations

import ast
from datetime import datetime, timedelta, timezone
import inspect
from pathlib import Path
import subprocess
import sys

import pytest

import dateoxide
from dateoxide.conf import SettingValidationError, Settings
from dateoxide.date import DateData, DateDataParser
from dateoxide.timezone_parser import get_local_tz_offset, local_tz_offset, pop_tz_offset_from_string
from dateoxide.utils.time_spans import detect_time_span, generate_time_span

REPO_ROOT = Path(__file__).resolve().parent.parent


def test_version_bumped_to_0_7_1() -> None:
    assert dateoxide.__version__ == "0.7.1"


def test_top_level_exports_and_dir() -> None:
    for sym in (
        "detect_time_span",
        "generate_time_span",
        "parse_freshness",
        "pop_tz_offset_from_string",
    ):
        assert hasattr(dateoxide, sym)
        assert sym in dateoxide.__all__
        assert sym in dir(dateoxide)


def test_datedataparser_module_attribute() -> None:
    assert DateData.__module__ == "dateoxide.date"
    assert DateDataParser.__module__ == "dateoxide.date"


def test_parse_batch_rejects_bare_str_and_bytes() -> None:
    with pytest.raises(TypeError):
        dateoxide.parse_batch("2026-01-01")  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        dateoxide.parse_batch(b"2026-01-01")  # type: ignore[arg-type]
    with pytest.raises(TypeError):
        dateoxide.parse_batch_timestamps("2026-01-01")  # type: ignore[arg-type]
    dp = DateDataParser()
    with pytest.raises(TypeError):
        dp.parse_batch("2026-01-01")  # type: ignore[arg-type]


def test_detect_languages_callback_rejects_bare_str() -> None:
    with pytest.raises(TypeError):
        dateoxide.parse("2026-05-15", detect_languages_function=lambda _t: "en")  # type: ignore[arg-type,return-value]
    dp = DateDataParser(detect_languages_function=lambda _t: "tr")  # type: ignore[arg-type,return-value]
    with pytest.raises(TypeError):
        dp.parse("15 Mayıs 2026")


def test_settings_validation_raises_setting_validation_error() -> None:
    bad_cases = [
        {"RETURN_AS_TIMEZONE_AWARE": 123},
        {"RETURN_AS_TIMEZONE_AWARE": "maybe"},
        {"DEFAULT_DAYS_IN_MONTH": True},
        {"DEFAULT_DAYS_IN_MONTH": 0},
        {"DEFAULT_DAYS_IN_MONTH": 32},
        {"CACHE_SIZE_LIMIT": True},
        {"CACHE_SIZE_LIMIT": -1},
        {"LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD": True},
        {"LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD": 1.5},
        {"LANGUAGE_DETECTION_CONFIDENCE_THRESHOLD": float("nan")},
        {"TIMEZONE": "Invalid/NoSuchZone"},
        {"TO_TIMEZONE": "Invalid/NoSuchZone"},
        {"TO_TIMEZONE": True},
        {"RELATIVE_BASE": True},
        {"DEFAULT_LANGUAGES": "en"},
        {"REQUIRE_PARTS": "day"},
        {"SKIP_TOKENS": "utc"},
        {"PARSERS": "absolute-time"},
        {"NORMALIZE": "yes"},
    ]
    for bad in bad_cases:
        with pytest.raises(SettingValidationError):
            dateoxide.parse("2026-05-15", settings=bad)
        with pytest.raises(SettingValidationError):
            Settings(bad)


def test_settings_replace_accepts_positional_dict_and_kwargs() -> None:
    base = Settings()
    s1 = base.replace({"DATE_ORDER": "DMY"})
    assert s1.DATE_ORDER == "DMY"
    assert s1.as_effective_dict().get("DATE_ORDER") == "DMY"
    s2 = s1.replace({"PREFER_DATES_FROM": "past"}, RETURN_TIME_AS_PERIOD=True)
    assert s2.DATE_ORDER == "DMY"
    assert s2.PREFER_DATES_FROM == "past"
    assert s2.RETURN_TIME_AS_PERIOD is True


def test_get_local_tz_offset_and_pop_tz_as_offset() -> None:
    off = get_local_tz_offset()
    assert isinstance(off, timedelta)
    assert isinstance(local_tz_offset, timedelta)

    cleaned1, tz_obj = pop_tz_offset_from_string("2026-11-26 15:00 EST", as_offset=True)
    assert cleaned1 == "2026-11-26 15:00"
    assert tz_obj is not None and not isinstance(tz_obj, str)

    cleaned2, tz_str = pop_tz_offset_from_string("2026-11-26 15:00 EST", as_offset=False)
    assert cleaned2 == "2026-11-26 15:00"
    assert tz_str == "EST"


def test_generate_time_span_accepts_aware_base_date() -> None:
    span = detect_time_span("past 3 days")
    assert span is not None
    base_aware = datetime(2026, 5, 15, 12, 0, tzinfo=timezone.utc)
    start, end = generate_time_span(span, base_date=base_aware)
    assert start == datetime(2026, 5, 12, 12, 0, tzinfo=timezone.utc)
    assert end == base_aware


def test_runtime_signatures_match_init_pyi() -> None:
    stub_src = (REPO_ROOT / "python" / "dateoxide" / "__init__.pyi").read_text(encoding="utf-8")
    tree = ast.parse(stub_src)
    stub_funcs = [
        node
        for node in tree.body
        if isinstance(node, ast.FunctionDef)
        and not any(isinstance(d, ast.Name) and d.id == "overload" for d in node.decorator_list)
    ]
    for fn_node in stub_funcs:
        runtime_fn = getattr(dateoxide, fn_node.name)
        sig = inspect.signature(runtime_fn)
        runtime_params = [p.name for p in sig.parameters.values()]
        stub_params = [a.arg for a in fn_node.args.posonlyargs + fn_node.args.args + fn_node.args.kwonlyargs]
        assert runtime_params == stub_params, (
            f"Parameter mismatch for {fn_node.name}: runtime={runtime_params} stub={stub_params}"
        )


def test_mypy_and_stubtest_if_installed() -> None:
    res_mypy = subprocess.run(
        [sys.executable, "-m", "mypy", "--strict", "python/dateoxide"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
    )
    assert res_mypy.returncode == 0, res_mypy.stdout + res_mypy.stderr
    # mypy.stubtest reports stdlib typing.Any / GenericAlias / (str, Enum).__new__ mismatches on Python < 3.11
    if sys.version_info >= (3, 11):
        res_stub = subprocess.run(
            [sys.executable, "-m", "mypy.stubtest", "dateoxide"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert res_stub.returncode == 0, res_stub.stdout + res_stub.stderr
        res_ext = subprocess.run(
            [sys.executable, "-m", "mypy.stubtest", "dateoxide._dateoxide"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert res_ext.returncode == 0, res_ext.stdout + res_ext.stderr
