"""Deprecated fastText language detection stub matching ``dateparser 1.5.1``."""

from __future__ import annotations

import warnings
from typing import NoReturn

warnings.warn(
    "fastText support is deprecated and will be removed in a future version. "
    "The fastText library is archived and unmaintained. "
    "Please migrate to langdetect: from dateoxide.custom_language_detection.langdetect import detect_languages",
    DeprecationWarning,
    stacklevel=2,
)


def detect_languages(text: str, confidence_threshold: float = 0.5) -> NoReturn:
    """Deprecated function mirroring ``dateparser.custom_language_detection.fasttext``."""
    raise ImportError(
        "fastText is no longer supported as the library is archived and unmaintained. "
        "Please use langdetect instead:\n"
        "  pip install langdetect\n"
        "  from dateoxide.custom_language_detection.langdetect import detect_languages"
    )
