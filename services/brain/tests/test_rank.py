"""Reranking.

The arithmetic matters less than the invariant: the model reorders what the
engine cleared and can never do anything else. Most of these exist to pin that
down, because it is the property that makes it safe to put a model anywhere
near a menu a guest with an allergy is being read from.
"""

from brain.history import replay
from brain.rank import (
    MAX_MODEL_WEIGHT,
    build_model,
    build_ranking,
    guest_traits,
    rerank,
)
from tests.test_history import add, fire

MAYA = {
    "id": "guest-maya",
    "name": "Maya Chen",
    "dietaryNeeds": ["gluten-free"],
    "likes": ["salmon", "citrus"],
}
JORDAN = {"id": "guest-jordan", "name": "Jordan Ellis", "dietaryNeeds": ["vegan"], "likes": []}
GUESTS = [MAYA, JORDAN]


def dishes(*specs):
    """specs: (id, score, eligible)"""
    return [
        {"id": id, "score": score, "eligible": eligible, "reasons": [], "warnings": []}
        for id, score, eligible in specs
    ]


def service(pairs, guest="guest-maya"):
    """pairs: list of (dish_id, times_ordered_on_one_ticket)"""
    log = []
    minute = 0
    for index, (dish, count) in enumerate(pairs):
        who = f"{guest}" if isinstance(guest, str) else guest[index]
        for _ in range(count):
            minute += 1
            log.append(add(dish, minute, guest=who))
        minute += 1
        log.append(fire(minute, guest=who))
    return replay(log)


class TestSafetyInvariant:
    def test_a_blocked_dish_stays_blocked(self):
        history = service([("carrot-tartare", 9)] * 3)
        ranked = rerank(
            dishes(("carrot-tartare", 0, False), ("beet-salad", 50, True)),
            build_model(history, GUESTS),
            MAYA,
        )

        tartare = next(dish for dish in ranked if dish["id"] == "carrot-tartare")
        assert tartare["eligible"] is False
        assert tartare["score"] == 0

    def test_a_blocked_dish_never_outranks_a_sellable_one(self):
        # Even when it is by far the most ordered thing tonight.
        history = service([("carrot-tartare", 9)] * 3)
        ranked = rerank(
            dishes(("carrot-tartare", 0, False), ("beet-salad", 1, True)),
            build_model(history, GUESTS),
            MAYA,
        )
        assert [dish["id"] for dish in ranked] == ["beet-salad", "carrot-tartare"]

    def test_every_blocked_dish_sorts_below_every_sellable_one(self):
        history = service([("a", 5), ("b", 5)])
        ranked = rerank(
            dishes(("a", 0, False), ("c", 10, True), ("b", 0, False), ("d", 90, True)),
            build_model(history, GUESTS),
            MAYA,
        )
        eligibility = [dish["eligible"] for dish in ranked]
        assert eligibility == sorted(eligibility, reverse=True)

    def test_warnings_on_a_blocked_dish_are_left_intact(self):
        blocked = dishes(("carrot-tartare", 0, False))
        blocked[0]["warnings"] = ["Contains guest allergen: tree nuts"]
        ranked = rerank(blocked, build_model(service([("carrot-tartare", 3)]), GUESTS), MAYA)
        assert ranked[0]["warnings"] == ["Contains guest allergen: tree nuts"]

    def test_the_input_is_not_modified(self):
        original = dishes(("beet-salad", 50, True))
        rerank(original, build_model(service([("beet-salad", 3)]), GUESTS), MAYA)
        assert original[0]["score"] == 50
        assert "modelScore" not in original[0]


class TestFallback:
    def test_no_history_leaves_the_engine_ranking_alone(self):
        model = build_model(replay([]), GUESTS)
        assert model.weight == 0.0

        ranked = rerank(dishes(("a", 90, True), ("b", 50, True)), model, MAYA)
        assert [dish["id"] for dish in ranked] == ["a", "b"]
        assert ranked[0]["score"] == 90

    def test_a_thin_service_barely_moves_anything(self):
        # One ticket should not reorder a menu.
        model = build_model(service([("b", 1)]), GUESTS)
        ranked = rerank(dishes(("a", 90, True), ("b", 50, True)), model, MAYA)
        assert [dish["id"] for dish in ranked] == ["a", "b"]

    def test_the_model_is_capped_below_the_engine(self):
        # However much evidence accumulates, the engine stays the larger voice:
        # it knows about margin, pacing and what the guest actually asked for.
        model = build_model(service([("a", 3)] * 40), GUESTS)
        assert model.weight <= MAX_MODEL_WEIGHT
        assert model.weight < 0.5

    def test_reports_which_ranking_was_used(self):
        cold = build_ranking(dishes(("a", 10, True)), replay([]), GUESTS, MAYA)
        assert cold["rankedBy"] == "engine"
        assert cold["ticketsSeen"] == 0

        warm = build_ranking(dishes(("a", 10, True)), service([("a", 2)] * 4), GUESTS, MAYA)
        assert warm["rankedBy"] == "model"
        assert warm["ticketsSeen"] == 4


class TestLearning:
    def test_a_much_ordered_dish_climbs(self):
        # The untried dish keeps its engine score, so the ordered one has to
        # genuinely overtake it rather than win by everything else sinking.
        history = service([("underdog", 5)] * 6)
        ranked = rerank(
            dishes(("favourite", 55, True), ("underdog", 50, True)),
            build_model(history, GUESTS),
            MAYA,
        )
        assert [dish["id"] for dish in ranked] == ["underdog", "favourite"]
        assert ranked[1]["score"] == 55, "the untried dish is left where it was"

    def test_the_engine_wins_a_tie(self):
        # Equal blended scores keep the engine's order, which is the right
        # tiebreak: it is the ranking with more information behind it.
        history = service([("underdog", 5)] * 6)
        ranked = rerank(
            dishes(("favourite", 60, True), ("underdog", 50, True)),
            build_model(history, GUESTS),
            MAYA,
        )
        assert ranked[0]["score"] == ranked[1]["score"] == 60
        assert ranked[0]["id"] == "favourite"

    def test_explains_itself_when_it_moves_something(self):
        history = service([("underdog", 5)] * 6)
        ranked = rerank(dishes(("underdog", 50, True)), build_model(history, GUESTS), MAYA)
        assert any("Ordered 30 times tonight" in reason for reason in ranked[0]["reasons"])

    def test_its_reason_survives_a_full_reason_list(self):
        # The engine already supplies three reasons and the UI shows three.
        # Appending to a full list and truncating dropped the only reason that
        # explains why the dish moved.
        history = service([("a", 5)] * 6)
        full = dishes(("a", 50, True))
        full[0]["reasons"] = ["Engine one", "Engine two", "Engine three"]

        ranked = rerank(full, build_model(history, GUESTS), MAYA)

        assert len(ranked[0]["reasons"]) == 3
        assert ranked[0]["reasons"][-1] == "Ordered 30 times tonight"
        assert ranked[0]["reasons"][0] == "Engine one"

    def test_keeps_both_scores_so_the_shift_is_inspectable(self):
        history = service([("a", 4)] * 4)
        ranked = rerank(dishes(("a", 20, True)), build_model(history, GUESTS), MAYA)
        assert ranked[0]["engineScore"] == 20
        assert ranked[0]["modelScore"] > 0
        assert ranked[0]["score"] != 20

    def test_learns_per_trait_not_per_person(self):
        # A model keyed on who someone is would just memorise them and say
        # nothing about a guest who has not been in before.
        assert "diet:gluten-free" in guest_traits(MAYA)
        assert "likes:salmon" in guest_traits(MAYA)
        assert not any("maya" in trait.lower() for trait in guest_traits(MAYA))

    def test_a_trait_shared_with_past_guests_carries_over(self):
        # Jordan is vegan and has ordered nothing. Another vegan's orders are
        # the only signal available for him.
        history = service([("cauliflower", 4)] * 4, guest="guest-jordan")
        model = build_model(history, GUESTS)

        newcomer = {"id": "guest-new", "dietaryNeeds": ["vegan"], "likes": []}
        assert model.affinity("cauliflower", guest_traits(newcomer)) > 0
        assert model.affinity("ember-steak", guest_traits(newcomer)) == 0

    def test_a_dish_nobody_ordered_keeps_its_engine_score(self):
        # Absence of evidence is not evidence of unpopularity. Blending a zero
        # in would drag every untried dish down by the same amount, which reads
        # as the whole menu getting worse rather than as having no opinion.
        model = build_model(service([("popular", 4)] * 4), GUESTS)
        ranked = rerank(dishes(("untried", 93, True)), model, MAYA)

        assert ranked[0]["score"] == 93
        assert ranked[0]["modelScore"] == 0.0

    def test_relative_order_of_untried_dishes_is_untouched(self):
        model = build_model(service([("popular", 4)] * 4), GUESTS)
        ranked = rerank(dishes(("a", 93, True), ("b", 73, True), ("c", 71, True)), model, MAYA)
        assert [dish["score"] for dish in ranked] == [93, 73, 71]

    def test_an_unordered_dish_scores_zero_from_the_model(self):
        model = build_model(service([("a", 3)] * 4), GUESTS)
        assert model.popularity("never-ordered") == 0.0
        assert model.score("never-ordered", guest_traits(MAYA)) == 0.0


class TestEdges:
    def test_an_empty_menu_ranks_to_nothing(self):
        assert rerank([], build_model(replay([]), GUESTS), MAYA) == []

    def test_a_guest_the_model_has_never_seen_is_fine(self):
        model = build_model(service([("a", 3)] * 4), GUESTS)
        stranger = {"id": "guest-x", "dietaryNeeds": [], "likes": []}
        ranked = rerank(dishes(("a", 50, True)), model, stranger)
        assert ranked[0]["eligible"] is True

    def test_a_fire_from_an_unknown_guest_still_counts_for_popularity(self):
        # The guest may have been added and removed; the dish was still sold.
        history = service([("a", 2)] * 3, guest="guest-ghost")
        model = build_model(history, [])
        assert model.orders_by_dish["a"] == 6
        assert model.orders_by_trait == {}
