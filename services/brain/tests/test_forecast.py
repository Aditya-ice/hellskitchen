"""Forecasting from the action log.

The thing most worth testing is not the arithmetic — it is that the forecast
refuses to sound more certain than the data allows, and that it does not bury
a real risk under irrelevant ones.
"""

from datetime import timedelta

from brain.forecast import (
    FAIR_EVIDENCE_FIRES,
    build_forecast,
    confidence_from,
    consumed_units,
    forecast_covers,
    forecast_stockouts,
)
from brain.history import replay
from tests.test_history import T0, add, entry, fire

MENU = [
    {"id": "salmon-carrot", "name": "Cedar Salmon", "ingredientIds": ["salmon", "carrot"]},
    {"id": "beet-salad", "name": "Golden Beet & Citrus", "ingredientIds": ["beet"]},
    {"id": "roasted-roots", "name": "Ember-Roasted Roots", "ingredientIds": ["carrot", "beet"]},
]


def ingredients(carrot=10.0, beet=20.0, beef=8.0):
    return [
        {"id": "carrot", "name": "Carrots", "onHand": carrot, "par": 18, "unit": "lb"},
        {"id": "beet", "name": "Golden beets", "onHand": beet, "par": 12, "unit": "lb"},
        {"id": "beef", "name": "Dry-aged beef", "onHand": beef, "par": 18, "unit": "portions"},
    ]


def service(tickets: int, item: str = "salmon-carrot", per_ticket: int = 1):
    """A log with `tickets` fires, one every ten minutes."""
    log = []
    minute = 0
    for index in range(tickets):
        guest = f"guest-{index}"
        for _ in range(per_ticket):
            minute += 1
            log.append(add(item, minute, guest=guest))
        minute += 1
        log.append(entry("seat-guest", minute, guestId=guest, tableId="t1"))
        minute += 1
        log.append(fire(minute, guest=guest))
        minute += 7
    return replay(log)


class TestConsumption:
    def test_counts_one_unit_of_each_ingredient_per_serving(self):
        # Mirrors consumption() in the Rust reducer. If that gains real recipe
        # quantities and this does not follow, every burn rate here is wrong.
        history = service(tickets=1, per_ticket=3)
        assert consumed_units(history, MENU) == {"salmon": 3.0, "carrot": 3.0}

    def test_adds_up_across_dishes_that_share_an_ingredient(self):
        log = [add("salmon-carrot", 1), add("roasted-roots", 2), fire(3)]
        assert consumed_units(replay(log), MENU)["carrot"] == 2.0

    def test_an_empty_history_consumes_nothing(self):
        assert consumed_units(replay([]), MENU) == {}


class TestConfidence:
    def test_no_tickets_is_no_confidence(self):
        result = confidence_from(replay([]), T0)
        assert result.level == "none"
        assert not result.actionable

    def test_a_single_ticket_is_low_confidence(self):
        result = confidence_from(service(tickets=1), T0 + timedelta(hours=1))
        assert result.level == "low"
        assert "1 ticket" in result.reason
        assert not result.actionable

    def test_a_short_service_is_low_however_many_tickets(self):
        # Five tickets in four minutes says nothing about the next hour.
        history = service(tickets=5)
        result = confidence_from(history, (history.first_at or T0) + timedelta(minutes=4))
        assert result.level == "low"
        assert "minutes of service" in result.reason

    def test_enough_evidence_reads_fair(self):
        history = service(tickets=FAIR_EVIDENCE_FIRES + 1)
        result = confidence_from(history, (history.first_at or T0) + timedelta(hours=1))
        assert result.level == "fair"
        assert result.actionable

    def test_never_claims_more_than_fair(self):
        # There is no amount of data in a demo service that justifies "high",
        # and a label nobody can earn should not exist.
        history = service(tickets=50)
        assert confidence_from(history, T0 + timedelta(hours=8)).level == "fair"


class TestStockoutRisks:
    def test_projects_when_an_ingredient_runs_out(self):
        # Four salmon dishes in one hour burns four carrots an hour; ten on
        # hand is two and a half hours.
        history = service(tickets=4)
        now = (history.first_at or T0) + timedelta(hours=1)
        risks = forecast_stockouts(history, ingredients(carrot=2.0), MENU, now, horizon_minutes=90)

        carrot = next(risk for risk in risks if risk.ingredient_id == "carrot")
        assert carrot.burn_per_hour == 4.0
        assert 25 <= (carrot.minutes_to_zero or 0) <= 35

    def test_ignores_ingredients_nobody_is_using(self):
        # Beef is low but untouched. Reporting it would bury the real risk.
        history = service(tickets=4)
        now = (history.first_at or T0) + timedelta(hours=1)
        risks = forecast_stockouts(history, ingredients(beef=0.5), MENU, now)

        assert all(risk.ingredient_id != "beef" for risk in risks)

    def test_ignores_what_will_comfortably_last(self):
        history = service(tickets=4)
        now = (history.first_at or T0) + timedelta(hours=1)
        risks = forecast_stockouts(
            history, ingredients(carrot=500.0), MENU, now, horizon_minutes=90
        )
        assert risks == []

    def test_reports_the_soonest_first(self):
        log = []
        minute = 0
        for index in range(4):
            minute += 1
            log.append(add("salmon-carrot", minute, guest=f"g{index}"))
            minute += 1
            log.append(add("beet-salad", minute, guest=f"g{index}"))
            minute += 1
            log.append(fire(minute, guest=f"g{index}"))
        history = replay(log)
        now = (history.first_at or T0) + timedelta(hours=1)

        risks = forecast_stockouts(
            history, ingredients(carrot=1.0, beet=3.0), MENU, now, horizon_minutes=240
        )
        assert [risk.ingredient_id for risk in risks] == ["carrot", "beet"]

    def test_says_which_dishes_a_shortage_takes_down(self):
        history = service(tickets=4)
        now = (history.first_at or T0) + timedelta(hours=1)
        risks = forecast_stockouts(history, ingredients(carrot=1.0), MENU, now)

        carrot = next(risk for risk in risks if risk.ingredient_id == "carrot")
        assert carrot.blocks == ["Cedar Salmon", "Ember-Roasted Roots"]

    def test_an_empty_history_forecasts_nothing(self):
        assert forecast_stockouts(replay([]), ingredients(), MENU, T0) == []


class TestCovers:
    def test_counts_what_actually_happened(self):
        history = service(tickets=3, per_ticket=2)
        covers = forecast_covers(history, (history.first_at or T0) + timedelta(hours=1))

        assert covers.parties_seated == 3
        assert covers.servings_fired == 6
        assert covers.parties_per_hour == 3.0
        assert covers.projected_servings_next_hour == 6.0

    def test_an_empty_history_projects_zero_rather_than_dividing_by_it(self):
        covers = forecast_covers(replay([]), T0)
        assert covers.parties_per_hour == 0.0
        assert covers.projected_servings_next_hour == 0.0


class TestBuildForecast:
    def test_carries_its_own_confidence_and_method(self):
        history = service(tickets=4)
        now = (history.first_at or T0) + timedelta(hours=1)
        result = build_forecast(history, ingredients(carrot=2.0), MENU, now)

        assert result["confidence"] == "fair"
        assert result["actionable"] is True
        assert "burn rate" in result["method"]
        assert result["stockoutRisks"][0]["ingredientId"] == "carrot"
        assert result["stockoutRisks"][0]["blocks"]

    def test_a_thin_service_is_reported_as_not_actionable(self):
        # The numbers are still returned — they are just labelled as something
        # not to act on, rather than withheld or dressed up.
        history = service(tickets=1)
        result = build_forecast(
            history, ingredients(carrot=1.0), MENU, (history.first_at or T0) + timedelta(hours=1)
        )
        assert result["confidence"] == "low"
        assert result["actionable"] is False

    def test_an_untouched_service_forecasts_nothing_and_says_why(self):
        result = build_forecast(replay([]), ingredients(), MENU, T0)
        assert result["confidence"] == "none"
        assert result["stockoutRisks"] == []
        assert "no tickets" in result["confidenceReason"]
