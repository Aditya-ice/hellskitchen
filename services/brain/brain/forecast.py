"""Demand forecasting from the action log.

A word on method, because it would be easy to oversell this. With a service's
worth of tickets there is nothing to fit a model to — a gradient-boosted
anything trained on twenty rows would be theatre, and worse, theatre that looks
authoritative. So this is a transparent rate projection: measure what has
actually been consumed, divide by elapsed service time, project forward.

Every answer carries its own confidence, computed from how much evidence backs
it, and the confidence is never allowed to read higher than the data supports.
A forecast nobody should act on says so.

When there is enough history to fit something better, `burn_per_hour` and
`parties_per_hour` are the two estimates a model would replace; the shape of
the output would not change.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass
from datetime import datetime, timezone
from typing import Any

from .history import History

# Matches `consumption()` in crates/ember-core/src/reducer.rs: one unit of each
# listed ingredient per serving. If the Rust rule gains real recipe quantities,
# this has to follow or every burn rate here is wrong.
UNITS_PER_SERVING = 1.0

#: Below this many fired tickets, a rate is a guess dressed as a number.
FAIR_EVIDENCE_FIRES = 3
#: And below this much elapsed service, a rate is dominated by one busy minute.
FAIR_EVIDENCE_MINUTES = 20.0


@dataclass
class Confidence:
    level: str  # "none" | "low" | "fair"
    reason: str

    @property
    def actionable(self) -> bool:
        return self.level == "fair"


def confidence_from(history: History, now: datetime) -> Confidence:
    fires = len(history.fires)
    minutes = history.span(now).total_seconds() / 60

    if fires == 0:
        return Confidence("none", "no tickets have been fired yet")
    if fires < FAIR_EVIDENCE_FIRES:
        return Confidence(
            "low", f"only {fires} ticket{'' if fires == 1 else 's'} so far"
        )
    if minutes < FAIR_EVIDENCE_MINUTES:
        return Confidence("low", f"only {minutes:.0f} minutes of service so far")
    return Confidence("fair", f"{fires} tickets over {minutes:.0f} minutes")


@dataclass
class StockoutRisk:
    ingredient_id: str
    name: str
    on_hand: float
    unit: str
    burn_per_hour: float
    minutes_to_zero: float | None
    blocks: list[str]

    def as_dict(self) -> dict[str, Any]:
        return {
            "ingredientId": self.ingredient_id,
            "name": self.name,
            "onHand": self.on_hand,
            "unit": self.unit,
            "burnPerHour": round(self.burn_per_hour, 2),
            "minutesToZero": None if self.minutes_to_zero is None else round(self.minutes_to_zero),
            "blocks": self.blocks,
        }


def consumed_units(history: History, menu_items: list[dict[str, Any]]) -> dict[str, float]:
    """How much of each ingredient the fired tickets used."""
    by_item = {item["id"]: item.get("ingredientIds", []) for item in menu_items}
    totals: dict[str, float] = defaultdict(float)

    for fire in history.fires:
        for menu_item_id, quantity in fire.lines:
            for ingredient_id in by_item.get(menu_item_id, []):
                totals[ingredient_id] += quantity * UNITS_PER_SERVING
    return dict(totals)


def forecast_stockouts(
    history: History,
    ingredients: list[dict[str, Any]],
    menu_items: list[dict[str, Any]],
    now: datetime | None = None,
    horizon_minutes: float = 90.0,
) -> list[StockoutRisk]:
    """Ingredients projected to run out inside the horizon, soonest first.

    Only counts ingredients actually being consumed. Something sitting
    untouched is not "about to run out", however little of it there is —
    reporting it as a risk would bury the ones that matter.
    """
    moment = now or datetime.now(timezone.utc)
    hours = history.span(moment).total_seconds() / 3600
    if hours <= 0:
        return []

    used = consumed_units(history, menu_items)
    blocked_by = defaultdict(list)
    for item in menu_items:
        for ingredient_id in item.get("ingredientIds", []):
            blocked_by[ingredient_id].append(item["name"])

    risks: list[StockoutRisk] = []
    for ingredient in ingredients:
        burn_per_hour = used.get(ingredient["id"], 0.0) / hours
        if burn_per_hour <= 0:
            continue

        on_hand = float(ingredient["onHand"])
        minutes_to_zero = (on_hand / burn_per_hour) * 60 if burn_per_hour else None
        if minutes_to_zero is None or minutes_to_zero > horizon_minutes:
            continue

        risks.append(
            StockoutRisk(
                ingredient_id=ingredient["id"],
                name=ingredient["name"],
                on_hand=on_hand,
                unit=ingredient["unit"],
                burn_per_hour=burn_per_hour,
                minutes_to_zero=minutes_to_zero,
                blocks=sorted(blocked_by.get(ingredient["id"], [])),
            )
        )

    risks.sort(key=lambda risk: risk.minutes_to_zero or 0)
    return risks


@dataclass
class CoverForecast:
    parties_seated: int
    servings_fired: int
    parties_per_hour: float
    projected_servings_next_hour: float

    def as_dict(self) -> dict[str, Any]:
        return {
            "partiesSeated": self.parties_seated,
            "servingsFired": self.servings_fired,
            "partiesPerHour": round(self.parties_per_hour, 2),
            "projectedServingsNextHour": round(self.projected_servings_next_hour, 1),
        }


def forecast_covers(history: History, now: datetime | None = None) -> CoverForecast:
    moment = now or datetime.now(timezone.utc)
    hours = history.span(moment).total_seconds() / 3600
    servings = sum(fire.covers for fire in history.fires)

    if hours <= 0:
        return CoverForecast(len(history.seatings), servings, 0.0, 0.0)

    return CoverForecast(
        parties_seated=len(history.seatings),
        servings_fired=servings,
        parties_per_hour=len(history.seatings) / hours,
        projected_servings_next_hour=servings / hours,
    )


def build_forecast(
    history: History,
    ingredients: list[dict[str, Any]],
    menu_items: list[dict[str, Any]],
    now: datetime | None = None,
    horizon_minutes: float = 90.0,
) -> dict[str, Any]:
    moment = now or datetime.now(timezone.utc)
    confidence = confidence_from(history, moment)
    risks = forecast_stockouts(history, ingredients, menu_items, moment, horizon_minutes)
    covers = forecast_covers(history, moment)

    return {
        "horizonMinutes": horizon_minutes,
        "confidence": confidence.level,
        "confidenceReason": confidence.reason,
        "actionable": confidence.actionable,
        "method": "observed burn rate over elapsed service",
        "stockoutRisks": [risk.as_dict() for risk in risks],
        "covers": covers.as_dict(),
    }
