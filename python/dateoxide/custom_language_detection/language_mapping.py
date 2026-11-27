"""Language code mapping helper compatible with ``dateparser.custom_language_detection.language_mapping``."""

from __future__ import annotations

from collections.abc import Iterable

from .._dateoxide import supported_languages

_SUPPORTED_SET: set[str] | None = None

_ALIASES: dict[str, list[str]] = {
    "no": ["nb"],
    "iw": ["he"],
    "in": ["id"],
    "ji": ["yi"],
    "fil": ["fil", "tl"],
    "zh-cn": ["zh"],
    "zh-tw": ["zh"],
    "zh-hk": ["zh"],
    "zh-sg": ["zh"],
    "zh-hans": ["zh"],
    "zh-hant": ["zh"],
}


def _get_supported() -> set[str]:
    global _SUPPORTED_SET
    if _SUPPORTED_SET is None:
        _SUPPORTED_SET = set(supported_languages())
    return _SUPPORTED_SET


def map_languages(language_codes: Iterable[str]) -> list[str]:
    """Map ISO / regional language codes to supported ``dateoxide`` locale codes."""
    sup = _get_supported()
    out: list[str] = []
    seen: set[str] = set()
    for code in language_codes:
        norm = code.strip().lower().replace("_", "-")
        if not norm:
            continue
        candidates: list[str] = []
        if norm in sup:
            candidates.append(norm)
        elif norm in _ALIASES:
            candidates.extend(_ALIASES[norm])
        else:
            base = norm.split("-")[0]
            if base in sup:
                candidates.append(base)
            elif base in _ALIASES:
                candidates.extend(_ALIASES[base])
        for c in candidates:
            if c in sup and c not in seen:
                seen.add(c)
                out.append(c)
    return out
