"""Reranking the engine's dish suggestions using what has actually been ordered.

The engine in `ember-core` scores dishes from hand-tuned weights — popularity,
margin, prep time, stated preferences. Those weights encode what the restaurant
believes. This adds what the service has actually done tonight.

Two rules make it safe to bolt on:

1. **Eligibility is never touched.** The engine decides what a guest may be
   sold; this only reorders the dishes it already cleared. A blocked dish stays
   blocked and stays at the bottom, no matter how popular it is. That is the
   whole reason the model sits here and not in the engine.
2. **The model never dominates.** Its influence scales with how much evidence
   backs it and caps out well below half, so a thin service produces the
   engine's ranking essentially unchanged. Falling back to the heuristic is the
   correct behaviour, not a failure mode.

Like the forecaster, this is deliberately a transparent estimator rather than a
fitted model: there is not enough history to fit anything honest yet. The
interface is what a real model would slot into.
"""

from __future__ import annotations

from collections import defaultdict
from dataclasses import dataclass, field
from typing import Any

from .history import History

#: Fires needed before the model carries its full (still limited) weight.
EVIDENCE_FOR_FULL_WEIGHT = 12
#: The most the model may ever shift a ranking. The engine knows about margin,
#: pacing and what the guest asked for; observed orders are one input, not the
#: deciding one.
MAX_MODEL_WEIGHT = 0.4


@dataclass
class RankModel:
    """What tonight's orders say about each dish."""

    orders_by_dish: dict[str, int] = field(default_factory=dict)
    orders_by_trait: dict[str, dict[str, int]] = field(default_factory=dict)
    total_fires: int = 0

    @property
    def weight(self) -> float:
        if self.total_fires <= 0:
            return 0.0
        evidence = min(self.total_fires / EVIDENCE_FOR_FULL_WEIGHT, 1.0)
        return evidence * MAX_MODEL_WEIGHT

    def popularity(self, dish_id: str) -> float:
        """0–100, relative to the most-ordered dish tonight."""
        if not self.orders_by_dish:
            return 0.0
        busiest = max(self.orders_by_dish.values())
        if busiest <= 0:
            return 0.0
        return 100.0 * self.orders_by_dish.get(dish_id, 0) / busiest

    def affinity(self, dish_id: str, traits: list[str]) -> float:
        """0–100, from what guests sharing a trait with this one ordered."""
        counts = [self.orders_by_trait.get(trait, {}).get(dish_id, 0) for trait in traits]
        if not counts:
            return 0.0
        best = max(
            (max(dishes.values(), default=0) for dishes in self.orders_by_trait.values()),
            default=0,
        )
        if best <= 0:
            return 0.0
        return 100.0 * max(counts) / best

    def score(self, dish_id: str, traits: list[str]) -> float:
        return 0.6 * self.popularity(dish_id) + 0.4 * self.affinity(dish_id, traits)


def guest_traits(guest: dict[str, Any]) -> list[str]:
    """The things about a guest that plausibly predict what they order.

    Deliberately not the guest's identity: a model keyed on who someone is
    would just memorise them, and would say nothing about a new guest.
    """
    traits = [f"diet:{need.lower()}" for need in guest.get("dietaryNeeds", [])]
    traits += [f"likes:{like.lower()}" for like in guest.get("likes", [])]
    return traits


def build_model(history: History, guests: list[dict[str, Any]]) -> RankModel:
    by_guest_id = {guest["id"]: guest for guest in guests}
    orders_by_dish: dict[str, int] = defaultdict(int)
    orders_by_trait: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))

    for fire in history.fires:
        guest = by_guest_id.get(fire.guest_id)
        traits = guest_traits(guest) if guest else []
        for dish_id, quantity in fire.lines:
            orders_by_dish[dish_id] += quantity
            for trait in traits:
                orders_by_trait[trait][dish_id] += quantity

    return RankModel(
        orders_by_dish=dict(orders_by_dish),
        orders_by_trait={trait: dict(dishes) for trait, dishes in orders_by_trait.items()},
        total_fires=len(history.fires),
    )


def rerank(
    dishes: list[dict[str, Any]],
    model: RankModel,
    guest: dict[str, Any],
) -> list[dict[str, Any]]:
    """Reorders the engine's eligible dishes. Blocked dishes are left alone.

    Returns a new list; the input is not modified.
    """
    traits = guest_traits(guest)
    weight = model.weight

    eligible: list[dict[str, Any]] = []
    blocked: list[dict[str, Any]] = []

    for dish in dishes:
        entry = dict(dish)
        if not entry.get("eligible"):
            # Untouched, and it keeps its zero score. Nothing the model knows
            # is a reason to put a blocked dish in front of a server.
            blocked.append(entry)
            continue

        engine_score = float(entry.get("score", 0))
        model_score = model.score(entry["id"], traits)
        ordered = model.orders_by_dish.get(entry["id"], 0)
        # A dish nobody has ordered tonight tells us nothing, so it keeps the
        # engine's score. Blending a zero into it would read as "this is less
        # good than we thought" when the truth is that we have no opinion —
        # and it would drag every untried dish down by the same amount, which
        # looks like the whole menu getting worse.
        has_signal = ordered > 0 or model_score > 0

        entry["engineScore"] = engine_score
        entry["modelScore"] = round(model_score, 1)
        entry["score"] = (
            round((1 - weight) * engine_score + weight * model_score, 1)
            if weight > 0 and has_signal
            else engine_score
        )

        if weight > 0 and ordered > 0:
            # Make room for it rather than appending to a full list and then
            # truncating, which silently dropped the one reason that explains
            # why the dish moved.
            engine_reasons = list(entry.get("reasons", []))[:2]
            entry["reasons"] = [
                *engine_reasons,
                f"Ordered {ordered} time{'' if ordered == 1 else 's'} tonight",
            ]
        eligible.append(entry)

    eligible.sort(key=lambda dish: dish["score"], reverse=True)
    return eligible + blocked


def build_ranking(
    dishes: list[dict[str, Any]],
    history: History,
    guests: list[dict[str, Any]],
    guest: dict[str, Any],
) -> dict[str, Any]:
    model = build_model(history, guests)
    ranked = rerank(dishes, model, guest)

    return {
        "dishes": ranked,
        # Honest about which ranking the caller is actually looking at, so the
        # UI never claims model assistance it did not get.
        "rankedBy": "model" if model.weight > 0 else "engine",
        "modelWeight": round(model.weight, 3),
        "ticketsSeen": model.total_fires,
    }
