"""Real-world example: Multilingual Telegram reminder message parser using dateoxide.

Parses natural-language Turkish/English/German/French reminder messages, extracts
both the target reminder text and normalized UTC/local trigger datetime, and can
plug directly into `python-telegram-bot` or `aiogram` handlers.

Run CLI demo:
  python examples/telegram_reminder_bot.py
"""
from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime, time, timezone
import re
from typing import Iterable

import dateoxide
from dateoxide import PartsOfDay
from dateoxide.search import search_dates


@dataclass
class ReminderTask:
    raw_message: str
    task_text: str
    trigger_local: datetime
    trigger_utc: datetime
    detected_language: str | None


REMINDER_PARTS_OF_DAY = PartsOfDay(
    early_morning=time(7, 0),
    morning=time(9, 0),
    afternoon=time(14, 0),
    evening=time(19, 0),
    night=time(21, 30),
)


def parse_reminder(
    message: str,
    user_tz: str = "Europe/Istanbul",
    now: datetime | None = None,
) -> ReminderTask | None:
    """Extract scheduled timestamp and clean reminder description from a chat message."""
    base = now or datetime.now()
    settings = {
        "RELATIVE_BASE": base,
        "PREFER_DATES_FROM": "future",
        "PARTS_OF_DAY": REMINDER_PARTS_OF_DAY,
        "TIMEZONE": user_tz,
        "RETURN_AS_TIMEZONE_AWARE": True,
    }
    found = search_dates(message, settings=settings, add_detected_language=True)
    if not found:
        return None

    matched_phrase, local_dt, lang = found[0]
    cleaned = re.sub(re.escape(matched_phrase), " ", message, count=1).strip(" ,:-;")
    cleaned = re.sub(r"\s{2,}", " ", cleaned) or message.strip()
    utc_dt = local_dt.astimezone(timezone.utc)
    return ReminderTask(
        raw_message=message,
        task_text=cleaned,
        trigger_local=local_dt,
        trigger_utc=utc_dt,
        detected_language=lang,
    )


def handle_batch_messages(messages: Iterable[str], now: datetime) -> list[ReminderTask]:
    results = []
    for msg in messages:
        task = parse_reminder(msg, now=now)
        if task is not None:
            results.append(task)
    return results


def main() -> None:
    fixed_now = datetime(2026, 11, 26, 15, 0, 0)
    sample_messages = [
        "Yarın saat 14:30 dişçi randevusu hatırlat",
        "2 saat 30 dakika sonra ilacı iç",
        "In 45 minutes join backend standup",
        "Morgen um 09:00 Uhr Rechnung bezahlen",
        "Demain à 18h30 appeler le client",
        "2026-12-01 10:00 sunucu yedekleme kontrolü",
    ]
    tasks = handle_batch_messages(sample_messages, now=fixed_now)
    print(f"Parsed {len(tasks)}/{len(sample_messages)} Telegram reminders:")
    for t in tasks:
        print(
            f"  [{t.detected_language or '?':2}] {t.trigger_local.isoformat()} "
            f"(UTC {t.trigger_utc.strftime('%Y-%m-%d %H:%M')}) -> {t.task_text}"
        )


if __name__ == "__main__":
    main()
