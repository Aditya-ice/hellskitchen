"""Turns the append-only action log into something to learn from.

`send-order` records only which guest fired, not what was on the ticket — the
contents live in the state at that moment. So the lines are reconstructed by
folding the add/remove events that preceded each fire. The log is complete, so
this is exact rather than an approximation.

Nothing here talks to a model. It is the tidy-up step that both the forecaster
and the reranker sit on top of.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass, field
from datetime import datetime, timedelta, timezone
from typing import Any, Iterable


def parse_time(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        return None
    return parsed if parsed.tzinfo else parsed.replace(tzinfo=timezone.utc)


@dataclass(frozen=True)
class Fire:
    """A ticket that reached the kitchen, and what was on it."""

    at: datetime
    guest_id: str
    lines: tuple[tuple[str, int], ...]

    @property
    def covers(self) -> int:
        return sum(quantity for _, quantity in self.lines)


@dataclass(frozen=True)
class Seating:
    at: datetime
    guest_id: str


@dataclass(frozen=True)
class Restock:
    at: datetime
    ingredient_id: str
    quantity: float


@dataclass
class History:
    fires: list[Fire] = field(default_factory=list)
    seatings: list[Seating] = field(default_factory=list)
    restocks: list[Restock] = field(default_factory=list)
    first_at: datetime | None = None
    last_at: datetime | None = None

    def span(self, now: datetime | None = None) -> timedelta:
        """How long the recorded service covers.

        Measured to `now` rather than to the last event, so a rate does not
        spike just because nothing has happened for a while.
        """
        if self.first_at is None:
            return timedelta(0)
        end = now or datetime.now(timezone.utc)
        return max(end - self.first_at, timedelta(0))

    def dishes_ordered(self) -> dict[str, int]:
        """How many servings of each dish actually left the kitchen."""
        totals: dict[str, int] = defaultdict(int)
        for fire in self.fires:
            for menu_item_id, quantity in fire.lines:
                totals[menu_item_id] += quantity
        return dict(totals)

    def dishes_by_guest(self) -> dict[str, dict[str, int]]:
        by_guest: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
        for fire in self.fires:
            for menu_item_id, quantity in fire.lines:
                by_guest[fire.guest_id][menu_item_id] += quantity
        return {guest: dict(items) for guest, items in by_guest.items()}


def replay(entries: Iterable[dict[str, Any]]) -> History:
    """Folds the log into a History.

    Draft order contents are tracked per guest and snapshotted when the ticket
    is fired. `reset` clears everything, because a reset service shares nothing
    with the one before it — carrying totals across would make the first
    forecast after a reset wrong in a way nobody would spot.
    """
    history = History()
    drafts: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))

    for entry in entries:
        action = entry.get("action", entry)
        at = parse_time(action.get("at"))
        kind = action.get("type")

        if at is not None:
            history.first_at = at if history.first_at is None else min(history.first_at, at)
            history.last_at = at if history.last_at is None else max(history.last_at, at)

        if kind == "add-order-item":
            drafts[action["guestId"]][action["menuItemId"]] += 1

        elif kind == "remove-order-item":
            guest = drafts[action["guestId"]]
            item = action["menuItemId"]
            if item in guest:
                guest[item] -= 1
                if guest[item] <= 0:
                    del guest[item]

        elif kind == "send-order":
            guest_id = action["guestId"]
            lines = tuple(sorted(drafts.get(guest_id, {}).items()))
            if at is not None and lines:
                history.fires.append(Fire(at=at, guest_id=guest_id, lines=lines))
            # The order is closed now; anything after this belongs to a new one.
            drafts.pop(guest_id, None)

        elif kind == "seat-guest":
            if at is not None:
                history.seatings.append(Seating(at=at, guest_id=action["guestId"]))

        elif kind == "restock-ingredient":
            if at is not None:
                history.restocks.append(
                    Restock(
                        at=at,
                        ingredient_id=action["ingredientId"],
                        quantity=float(action.get("quantity", 0)),
                    )
                )

        elif kind == "reset":
            history = History()
            drafts = defaultdict(lambda: defaultdict(int))

    return history
