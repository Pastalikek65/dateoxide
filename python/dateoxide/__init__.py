"""dateoxide: Blazing-fast, low-memory multilingual date parser written in Rust.

Drop-in accelerator for Python's ``dateparser`` supporting 205 languages,
zero-backtracking parsing, and GIL-free parallel batch processing.
"""

from ._dateoxide import (
    DateData,
    DateDataParser,
    __version__,
    parse,
    parse_batch,
    search_dates,
    supported_languages,
)
from . import date, search

__all__ = [
    "DateData",
    "DateDataParser",
    "__version__",
    "date",
    "parse",
    "parse_batch",
    "search",
    "search_dates",
    "supported_languages",
]
