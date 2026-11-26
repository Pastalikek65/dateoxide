"""Drop-in ``dateparser.search`` compatible submodule."""

from ._dateoxide import search_dates

__all__ = ["search_dates"]
