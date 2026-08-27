"""Replaying the action log.

The log records that an order was fired but not what was on it, so these check
that the reconstruction is exact — a forecaster fed the wrong lines is worse
than no forecaster.
"""

from datetime import UTC, datetime, timedelta

from brain.history import History, replay

T0 = datetime(2026, 8, 26, 18, 0, tzinfo=UTC)


def entry(kind: str, minutes: int = 0, **fields):
    at = (T0 + timedelta(minutes=minutes)).isoformat().replace("+00:00", "Z")
    return {"seq": minutes, "action": {"id": f"a{minutes}", "at": at, "type": kind, **fields}}


def add(item: str, minutes: int = 0, guest: str = "guest-maya"):
    return entry("add-order-item", minutes, guestId=guest, menuItemId=item)


def fire(minutes: int = 0, guest: str = "guest-maya"):
    return entry("send-order", minutes, guestId=guest)


class TestReplayingTickets:
    def test_reconstructs_what_was_on_a_ticket(self):
        history = replay([add("beet-salad", 1), add("salmon-carrot", 2), fire(3)])

        assert len(history.fires) == 1
        assert dict(history.fires[0].lines) == {"beet-salad": 1, "salmon-carrot": 1}
        assert history.fires[0].guest_id == "guest-maya"

    def test_counts_repeats_as_quantity(self):
        history = replay([add("beet-salad", 1), add("beet-salad", 2), fire(3)])
        assert dict(history.fires[0].lines) == {"beet-salad": 2}

    def test_honours_removals_before_the_fire(self):
        history = replay(
            [
                add("beet-salad", 1),
                add("beet-salad", 2),
                entry("remove-order-item", 3, guestId="guest-maya", menuItemId="beet-salad"),
                fire(4),
            ]
        )
        assert dict(history.fires[0].lines) == {"beet-salad": 1}

    def test_a_line_removed_to_zero_never_reaches_the_kitchen(self):
        history = replay(
            [
                add("beet-salad", 1),
                entry("remove-order-item", 2, guestId="guest-maya", menuItemId="beet-salad"),
                add("herb-chicken", 3),
                fire(4),
            ]
        )
        assert dict(history.fires[0].lines) == {"herb-chicken": 1}

    def test_an_empty_order_is_not_a_fire(self):
        # The reducer refuses to send an empty order, so one in the log would
        # mean the replay had drifted from the reducer.
        assert replay([fire(1)]).fires == []

    def test_keeps_parties_separate(self):
        history = replay(
            [
                add("beet-salad", 1, guest="guest-maya"),
                add("ember-steak", 2, guest="guest-noah"),
                fire(3, guest="guest-maya"),
                fire(4, guest="guest-noah"),
            ]
        )
        assert len(history.fires) == 2
        assert dict(history.fires[0].lines) == {"beet-salad": 1}
        assert dict(history.fires[1].lines) == {"ember-steak": 1}

    def test_a_second_ticket_does_not_inherit_the_first(self):
        history = replay([add("beet-salad", 1), fire(2), add("ember-steak", 3), fire(4)])
        assert dict(history.fires[0].lines) == {"beet-salad": 1}
        assert dict(history.fires[1].lines) == {"ember-steak": 1}


class TestReset:
    def test_a_reset_discards_everything_before_it(self):
        # A reset service shares nothing with the one before it. Carrying
        # totals across would make the first forecast after a reset wrong in a
        # way nobody would notice.
        history = replay(
            [
                add("beet-salad", 1),
                fire(2),
                entry("reset", 3),
                add("ember-steak", 4),
                fire(5),
            ]
        )
        assert len(history.fires) == 1
        assert dict(history.fires[0].lines) == {"ember-steak": 1}

    def test_a_reset_discards_a_draft_in_progress(self):
        history = replay([add("beet-salad", 1), entry("reset", 2), fire(3)])
        assert history.fires == []


class TestOtherEvents:
    def test_records_seatings_and_restocks(self):
        history = replay(
            [
                entry("seat-guest", 1, guestId="guest-maya", tableId="t2"),
                entry("restock-ingredient", 2, ingredientId="carrot", quantity=15),
            ]
        )
        assert history.seatings[0].guest_id == "guest-maya"
        assert history.restocks[0].ingredient_id == "carrot"
        assert history.restocks[0].quantity == 15.0

    def test_ignores_events_it_has_no_use_for(self):
        history = replay(
            [
                entry("update-guest-notes", 1, guestId="guest-maya", notes="hi"),
                add("beet-salad", 2),
                fire(3),
            ]
        )
        assert len(history.fires) == 1

    def test_survives_a_malformed_timestamp(self):
        broken = {"action": {"id": "x", "at": "not-a-date", "type": "seat-guest", "guestId": "g"}}
        history = replay([broken])
        assert history.seatings == []
        assert history.first_at is None


class TestAggregates:
    def test_totals_dishes_across_tickets(self):
        history = replay(
            [
                add("beet-salad", 1),
                add("beet-salad", 2),
                fire(3),
                add("beet-salad", 4, guest="guest-noah"),
                add("ember-steak", 5, guest="guest-noah"),
                fire(6, guest="guest-noah"),
            ]
        )
        assert history.dishes_ordered() == {"beet-salad": 3, "ember-steak": 1}

    def test_splits_dishes_by_guest(self):
        history = replay([add("beet-salad", 1), fire(2)])
        assert history.dishes_by_guest() == {"guest-maya": {"beet-salad": 1}}

    def test_span_is_measured_to_now_not_to_the_last_event(self):
        # Otherwise a quiet half hour would make the burn rate look higher than
        # it is, right when someone is deciding whether to reorder.
        history = replay([add("beet-salad", 0), fire(10)])
        span = history.span(now=T0 + timedelta(minutes=60))
        assert span == timedelta(minutes=60)

    def test_an_empty_log_has_no_span(self):
        assert replay([]).span(now=T0) == timedelta(0)
        assert History().span(now=T0) == timedelta(0)
