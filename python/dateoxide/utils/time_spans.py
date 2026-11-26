"""Time span detection & generation compatible with ``dateparser.utils.time_spans``."""

from .._dateoxide import detect_time_span, generate_time_span

__all__ = ["detect_time_span", "generate_time_span"]
