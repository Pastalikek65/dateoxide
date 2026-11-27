"""Optional ``langdetect`` adapter compatible with ``dateparser.custom_language_detection.langdetect``."""

from __future__ import annotations

from typing import Any


class _Factory:
    data: Any = None


def _init_factory() -> Any:
    try:
        import langdetect as _langdetect
    except ImportError as exc:
        raise ImportError(
            "langdetect is not installed. Install optional dependency with: pip install langdetect"
        ) from exc

    if _Factory.data is None:
        _Factory.data = _langdetect.detector_factory.DetectorFactory()
        _Factory.data.load_profile(_langdetect.detector_factory.PROFILES_DIRECTORY)
        _Factory.data.seed = 0
    return _langdetect


def _get_language_probabilities(text: str) -> Any:
    _langdetect = _init_factory()
    detector = _Factory.data.create()
    detector.append(text)
    return detector.get_probabilities(), _langdetect


def detect_languages(text: str, confidence_threshold: float = 0.5) -> list[str]:
    """Detect likely ISO 639-1 language codes for ``text`` above ``confidence_threshold``."""
    language_codes: list[str] = []
    probs, _langdetect = _get_language_probabilities(text)
    try:
        for candidate in probs:
            if candidate.prob > confidence_threshold:
                language_codes.append(candidate.lang)
    except _langdetect.lang_detect_exception.LangDetectException:
        pass
    return language_codes
