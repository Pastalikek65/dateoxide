"""Drop-in ``dateparser.date`` compatible submodule."""

from ._dateoxide import DateData, DateDataParser

__all__ = ["DateData", "DateDataParser"]
