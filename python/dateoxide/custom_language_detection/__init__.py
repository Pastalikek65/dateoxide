"""Custom language detection helpers compatible with ``dateparser.custom_language_detection``."""

from __future__ import annotations

import importlib
from typing import TYPE_CHECKING, Any

if TYPE_CHECKING:
    from . import fasttext, langdetect, language_mapping

_SUBMODULES = {"fasttext", "langdetect", "language_mapping"}


def __getattr__(name: str) -> Any:
    if name in _SUBMODULES:
        mod = importlib.import_module(f".{name}", __name__)
        globals()[name] = mod
        return mod
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


__all__ = ["fasttext", "langdetect", "language_mapping"]
