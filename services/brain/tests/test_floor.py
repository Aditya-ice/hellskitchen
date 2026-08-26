"""The formatters are what the model reads, so they are what these test."""

from datetime import datetime, timezone

from brain.floor import (
    Floor,
    describe_floor,
    describe_guest,
    describe_recommendations,
    describe_stock,
    describe_tickets,
)


def floor(orders: list | None = None) -> Floor:
    state = {
        "guests": [
            {
                "id": "guest-maya",
                "name": "Maya Chen",
                "partySize": 4,
                "status": "seated",
                "allergies": ["tree nuts"],
                "dietaryNeeds": ["gluten-free"],
                "likes": ["salmon"],
                "dislikes": ["mushroom"],
                "seatingPreferences": ["window"],
                "visitCount": 6,
                "lastVisit": "Jun 18",
                "reservationTime": "6:15 PM",
                "arrivalTime": "6:07 PM",
                "notes": "Anniversary.",
            },
            {
                "id": "guest-sam",
                "name": "Sam Reed",
                "partySize": 2,
                "status": "waiting",
                "allergies": [],
                "dietaryNeeds": [],
                "likes": [],
                "dislikes": [],
                "seatingPreferences": [],
                "visitCount": 0,
                "lastVisit": None,
                "reservationTime": None,
                "arrivalTime": "7:02 PM",
                "notes": "",
            },
        ],
        "tables": [
            {
                "id": "t2",
                "label": "T2",
                "capacity": 4,
                "area": "window",
                "status": "occupied",
                "accessible": True,
                "seatedGuestId": "guest-maya",
                "estimatedAvailableMinutes": 0,
            },
            {
                "id": "t5",
                "label": "T5",
                "capacity": 4,
                "area": "main",
                "status": "clearing",
                "accessible": False,
                "seatedGuestId": None,
                "estimatedAvailableMinutes": 8,
            },
        ],
        "orders": [
            {
                "id": "order-1",
                "guestId": "guest-maya",
                "tableId": "t2",
                "status": "sent",
                "lines": [{"menuItemId": "beet-salad", "quantity": 2, "notes": ""}],
                "guestNotes": "Candle with dessert.",
                "createdAt": "2026-08-26T18:00:00Z",
                "sentAt": "2026-08-26T18:10:00Z",
                "completedAt": None,
            }
        ]
        if orders is None
        else orders,
        "ingredients": [
            {"id": "carrot", "name": "Carrots", "onHand": 0, "par": 18, "unit": "lb", "aliases": []},
            {
                "id": "beet",
                "name": "Golden beets",
                "onHand": 3,
                "par": 12,
                "unit": "lb",
                "aliases": [],
            },
            {
                "id": "beef",
                "name": "Dry-aged beef",
                "onHand": 8,
                "par": 18,
                "unit": "portions",
                "aliases": [],
            },
        ],
    }
    return Floor(
        version=5,
        state=state,
        menu={
            "restaurant": {"serviceLabel": "Dinner service", "name": "Ember & Ash"},
            "menuItems": [
                {"id": "beet-salad", "name": "Golden Beet & Citrus", "ingredientIds": ["beet"]},
                {
                    "id": "salmon-carrot",
                    "name": "Cedar Salmon",
                    "ingredientIds": ["salmon", "carrot"],
                },
            ],
        },
        summary={"waitingGuests": 1, "openTables": 0, "averageWaitMinutes": 12},
    )


class TestFindGuest:
    def test_matches_the_name_staff_would_use(self):
        assert floor().find_guest("Maya Chen")["id"] == "guest-maya"

    def test_matches_a_partial_name(self):
        # Nobody types a full name mid-service.
        assert floor().find_guest("maya")["id"] == "guest-maya"

    def test_matches_an_id(self):
        assert floor().find_guest("guest-sam")["name"] == "Sam Reed"

    def test_returns_none_rather_than_guessing(self):
        assert floor().find_guest("Nobody At All") is None

    def test_an_empty_needle_matches_nothing(self):
        # Otherwise the substring pass would return whoever happens to be first.
        assert floor().find_guest("") is None


class TestDescribeFloor:
    def test_names_every_party_and_table(self):
        text = describe_floor(floor())
        assert "Maya Chen" in text
        assert "Sam Reed" in text
        assert "T2" in text and "T5" in text

    def test_shouts_about_allergies(self):
        # Upper case on purpose: this is the line that must not be skimmed past.
        assert "ALLERGIES: tree nuts" in describe_floor(floor())

    def test_reports_who_is_at_which_table(self):
        assert "occupied by Maya Chen" in describe_floor(floor())

    def test_reports_when_a_clearing_table_frees_up(self):
        assert "ready in ~8 min" in describe_floor(floor())

    def test_writes_whole_numbers_without_a_decimal_point(self):
        # The summary arrives as JSON floats; "Average wait: 12.0 min" reads
        # like a machine talking.
        text = describe_floor(floor())
        assert "Average wait: 12 min" in text
        assert "12.0" not in text


class TestDescribeGuest:
    def test_leads_with_allergies_and_diet(self):
        text = describe_guest(floor(), floor().find_guest("maya"))
        assert "Allergies: tree nuts" in text
        assert "Dietary needs: gluten-free" in text

    def test_says_none_recorded_rather_than_omitting(self):
        # An absent line reads as either "no allergies" or "never asked".
        # Saying so removes the ambiguity.
        text = describe_guest(floor(), floor().find_guest("Sam Reed"))
        assert "Allergies: none recorded" in text

    def test_includes_the_open_order_with_dish_names(self):
        text = describe_guest(floor(), floor().find_guest("maya"))
        assert "2x Golden Beet & Citrus" in text
        assert "Candle with dessert." in text

    def test_says_when_there_is_no_order(self):
        assert "No order open." in describe_guest(floor(), floor().find_guest("Sam Reed"))


class TestDescribeStock:
    def test_flags_out_and_low_against_the_engine_thresholds(self):
        text = describe_stock(floor())
        assert "Carrots: 0 of 18 lb [OUT OF STOCK]" in text
        assert "Golden beets: 3 of 12 lb [LOW]" in text
        assert "[ok]" in text

    def test_says_which_dishes_an_ingredient_blocks(self):
        # "What can I sell that uses up the carrots" needs this link.
        assert "used in Cedar Salmon" in describe_stock(floor())


class TestDescribeTickets:
    def test_reports_age_from_when_the_ticket_was_fired(self):
        text = describe_tickets(floor(), datetime(2026, 8, 26, 18, 34, tzinfo=timezone.utc))
        assert "24 min old" in text
        assert "T2 (Maya Chen)" in text

    def test_says_so_when_the_pass_is_clear(self):
        assert "pass is clear" in describe_tickets(floor(orders=[]))

    def test_survives_an_unparseable_timestamp(self):
        broken = floor()
        orders = [{**broken.orders()[0], "sentAt": "not-a-date"}]
        assert "T2" in describe_tickets(floor(orders=orders))


PAYLOAD = {
    "guestId": "guest-maya",
    "dishes": [
        {
            "id": "beet-salad",
            "eligible": True,
            "score": 93,
            "reasons": ["Meets gluten-free"],
            "warnings": [],
        },
        {
            "id": "salmon-carrot",
            "eligible": False,
            "score": 0,
            "reasons": [],
            "warnings": ["Carrots is unavailable"],
        },
    ],
    "tables": [
        {"id": "t2", "eligible": True, "score": 100, "reasons": ["Exact fit"], "warnings": []},
        {
            "id": "t5",
            "eligible": False,
            "score": 0,
            "reasons": [],
            "warnings": ["Does not meet accessibility need"],
        },
    ],
    "estimateWait": 0,
    "orderTotal": 34,
}


class TestDescribeRecommendations:
    def test_lists_blocked_dishes_rather_than_hiding_them(self):
        # Filtering them out would let the model say "everything is fine" when
        # something specifically is not.
        text = describe_recommendations(PAYLOAD, floor())
        assert "Cedar Salmon: BLOCKED — Carrots is unavailable" in text

    def test_marks_sellable_dishes_with_the_engine_score(self):
        assert "Golden Beet & Citrus: SELLABLE, score 93" in describe_recommendations(
            PAYLOAD, floor()
        )

    def test_uses_dish_and_table_names_not_ids(self):
        text = describe_recommendations(PAYLOAD, floor())
        assert "salmon-carrot" not in text
        assert "T5: not suitable" in text

    def test_an_allergen_block_reaches_the_model_verbatim(self):
        payload = {
            **PAYLOAD,
            "dishes": [
                {
                    "id": "beet-salad",
                    "eligible": False,
                    "score": 0,
                    "reasons": [],
                    "warnings": ["Contains guest allergen: tree nuts"],
                }
            ],
        }
        text = describe_recommendations(payload, floor())
        assert "BLOCKED — Contains guest allergen: tree nuts" in text
