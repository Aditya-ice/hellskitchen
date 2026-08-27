"""Read-only view of the POS, and the text the model is shown.

Everything here is derived from `ember-server`. The agent has no state of its
own and no way to change anything: it reads these views and answers in prose.
That is deliberate — the floor is the host's to run, and an agent that could
seat a party or fire an order would be a much bigger claim than this makes.

The formatters are the interesting part. They are what the model actually
reads, so what they include and — more importantly — what they refuse to
soften is the whole safety story. A dish the engine has ruled ineligible is
always rendered as blocked, with its reason attached.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any

import httpx


def _amount(value: float) -> str:
    """Renders 18.0 as "18" and 4.5 as "4.5"."""
    return str(int(value)) if float(value).is_integer() else str(value)


def _rows(source: dict[str, Any], key: str) -> list[dict[str, Any]]:
    """Read a list of JSON objects out of an untyped payload.

    Everything the client returns is `response.json()`, so the boundary is
    genuinely `Any`. This narrows it in one place, and treats a payload whose
    shape disagrees with the contract as absent rather than raising deep inside
    a formatter.
    """
    value = source.get(key, [])
    return value if isinstance(value, list) else []


def _object(payload: Any) -> dict[str, Any]:
    """Narrow a decoded JSON body to an object, as every endpoint here returns."""
    return payload if isinstance(payload, dict) else {}


@dataclass(frozen=True)
class Floor:
    """A consistent read of the service."""

    version: int
    state: dict[str, Any]
    menu: dict[str, Any]
    summary: dict[str, Any]

    def guests(self) -> list[dict[str, Any]]:
        return _rows(self.state, "guests")

    def tables(self) -> list[dict[str, Any]]:
        return _rows(self.state, "tables")

    def orders(self) -> list[dict[str, Any]]:
        return _rows(self.state, "orders")

    def ingredients(self) -> list[dict[str, Any]]:
        return _rows(self.state, "ingredients")

    def menu_items(self) -> list[dict[str, Any]]:
        return _rows(self.menu, "menuItems")

    def dish_name(self, menu_item_id: str) -> str:
        for item in self.menu_items():
            if item["id"] == menu_item_id:
                return str(item["name"])
        return menu_item_id

    def table_label(self, table_id: str | None) -> str:
        for table in self.tables():
            if table["id"] == table_id:
                return str(table["label"])
        return "—"

    def find_guest(self, needle: str) -> dict[str, Any] | None:
        """Matches on id first, then on name, case-insensitively.

        Staff refer to guests by name, so an agent asked about "Maya" must not
        need the internal id.
        """
        wanted = needle.strip().lower()
        for guest in self.guests():
            if guest["id"].lower() == wanted:
                return guest
        for guest in self.guests():
            if guest["name"].lower() == wanted:
                return guest
        for guest in self.guests():
            if wanted and wanted in guest["name"].lower():
                return guest
        return None


# --- formatters -----------------------------------------------------------


def describe_floor(floor: Floor) -> str:
    lines = [
        f"Service: {floor.menu.get('restaurant', {}).get('serviceLabel', 'Service')}",
        f"Waiting parties: {_amount(floor.summary.get('waitingGuests', 0))}",
        f"Open tables: {_amount(floor.summary.get('openTables', 0))}",
        f"Average wait: {_amount(floor.summary.get('averageWaitMinutes', 0))} min",
        "",
        "Parties:",
    ]
    for guest in floor.guests():
        seated_at = next(
            (t["label"] for t in floor.tables() if t.get("seatedGuestId") == guest["id"]),
            None,
        )
        details = [
            f"party of {guest['partySize']}",
            f"status {guest['status']}",
        ]
        if seated_at:
            details.append(f"at {seated_at}")
        if guest.get("reservationTime"):
            details.append(f"booked {guest['reservationTime']}")
        if guest.get("arrivalTime"):
            details.append(f"arrived {guest['arrivalTime']}")
        if guest.get("allergies"):
            details.append("ALLERGIES: " + ", ".join(guest["allergies"]))
        if guest.get("dietaryNeeds"):
            details.append("diet: " + ", ".join(guest["dietaryNeeds"]))
        lines.append(f"- {guest['name']} ({guest['id']}): " + "; ".join(details))

    lines.append("")
    lines.append("Tables:")
    for table in floor.tables():
        occupant = next(
            (g["name"] for g in floor.guests() if g["id"] == table.get("seatedGuestId")),
            None,
        )
        detail = (
            f"- {table['label']}: seats {table['capacity']}, {table['area']}, {table['status']}"
        )
        if table.get("accessible"):
            detail += ", accessible"
        if occupant:
            detail += f", occupied by {occupant}"
        if table["status"] == "clearing":
            detail += f", ready in ~{_amount(table['estimatedAvailableMinutes'])} min"
        lines.append(detail)

    return "\n".join(lines)


def describe_guest(floor: Floor, guest: dict[str, Any]) -> str:
    lines = [
        f"{guest['name']} ({guest['id']})",
        f"Party of {guest['partySize']}, status {guest['status']}",
    ]
    # Allergies first and unabbreviated. This is the one thing in the whole
    # view that must never be summarised away.
    lines.append(
        "Allergies: "
        + (", ".join(guest["allergies"]) if guest.get("allergies") else "none recorded")
    )
    lines.append(
        "Dietary needs: "
        + (", ".join(guest["dietaryNeeds"]) if guest.get("dietaryNeeds") else "none recorded")
    )
    if guest.get("likes"):
        lines.append("Likes: " + ", ".join(guest["likes"]))
    if guest.get("dislikes"):
        lines.append("Dislikes: " + ", ".join(guest["dislikes"]))
    if guest.get("seatingPreferences"):
        lines.append("Seating preferences: " + ", ".join(guest["seatingPreferences"]))
    lines.append(f"Previous visits: {guest.get('visitCount', 0)}")
    if guest.get("lastVisit"):
        lines.append(f"Last visit: {guest['lastVisit']}")
    if guest.get("notes"):
        lines.append(f"Service notes: {guest['notes']}")

    order = next((o for o in floor.orders() if o["guestId"] == guest["id"]), None)
    if order:
        lines.append("")
        lines.append(
            f"Order {order['id']}: {order['status']}, {floor.table_label(order.get('tableId'))}"
        )
        for line in order.get("lines", []):
            lines.append(f"  {line['quantity']}x {floor.dish_name(line['menuItemId'])}")
        if order.get("guestNotes"):
            lines.append(f"  Notes: {order['guestNotes']}")
    else:
        lines.append("")
        lines.append("No order open.")

    return "\n".join(lines)


def describe_stock(floor: Floor) -> str:
    lines = ["Stock on hand (a dish cannot be sold once an ingredient reaches zero):"]
    for ingredient in floor.ingredients():
        on_hand = float(ingredient["onHand"])
        par = float(ingredient["par"])
        if on_hand <= 0:
            level = "OUT OF STOCK"
        elif par > 0 and on_hand / par <= 0.25:
            level = "LOW"
        else:
            level = "ok"
        uses = [
            item["name"]
            for item in floor.menu_items()
            if ingredient["id"] in item.get("ingredientIds", [])
        ]
        line = (
            f"- {ingredient['name']}: {_amount(on_hand)} of {_amount(par)} "
            f"{ingredient['unit']} [{level}]"
        )
        if uses:
            line += " — used in " + ", ".join(uses)
        lines.append(line)
    return "\n".join(lines)


def describe_tickets(floor: Floor, now: datetime | None = None) -> str:
    moment = now or datetime.now(UTC)
    open_tickets = [o for o in floor.orders() if o["status"] == "sent"]
    if not open_tickets:
        return "No open tickets: the pass is clear."

    rows = []
    for order in open_tickets:
        age = ""
        if order.get("sentAt"):
            try:
                sent = datetime.fromisoformat(order["sentAt"].replace("Z", "+00:00"))
                minutes = max(0, int((moment - sent).total_seconds() // 60))
                age = f", {minutes} min old"
            except ValueError:
                age = ""
        guest = next(
            (g["name"] for g in floor.guests() if g["id"] == order["guestId"]), "unknown party"
        )
        items = ", ".join(
            f"{line['quantity']}x {floor.dish_name(line['menuItemId'])}"
            for line in order.get("lines", [])
        )
        rows.append(
            f"- {floor.table_label(order.get('tableId'))} ({guest}){age}: {items or 'no items'}"
        )
    return f"{len(open_tickets)} open ticket(s):\n" + "\n".join(rows)


def describe_recommendations(payload: dict[str, Any], floor: Floor) -> str:
    """Renders the engine's ranking, blocks included.

    Blocked dishes are listed rather than filtered out, with their reason. The
    model is not asked to decide what is safe — it is shown what was decided.
    """
    lines = [f"Engine ranking for {payload.get('guestId')}:", "", "Dishes:"]
    for dish in payload.get("dishes", []):
        name = floor.dish_name(dish["id"])
        if dish["eligible"]:
            reasons = "; ".join(dish.get("reasons", [])) or "no specific reason"
            line = f"- {name}: SELLABLE, score {_amount(dish['score'])} — {reasons}"
            if dish.get("warnings"):
                line += " | caution: " + "; ".join(dish["warnings"])
        else:
            line = f"- {name}: BLOCKED — " + "; ".join(dish.get("warnings", ["not available"]))
        lines.append(line)

    lines.extend(["", "Tables:"])
    for table in payload.get("tables", []):
        label = floor.table_label(table["id"])
        if table["eligible"]:
            lines.append(
                f"- {label}: available, score {_amount(table['score'])} — "
                + ("; ".join(table.get("reasons", [])) or "no specific reason")
            )
        else:
            lines.append(
                f"- {label}: not suitable — " + "; ".join(table.get("warnings", ["unsuitable"]))
            )

    lines.extend(
        [
            "",
            f"Estimated wait for this party: {_amount(payload.get('estimateWait', 0))} min",
            f"Current check total: ${_amount(payload.get('orderTotal', 0))}",
        ]
    )
    return "\n".join(lines)


# --- client ---------------------------------------------------------------


class FloorClient:
    """Reads the POS over HTTP. Never writes."""

    def __init__(self, base_url: str, client: httpx.AsyncClient | None = None) -> None:
        self.base_url = base_url.rstrip("/")
        self._client = client

    async def _get(self, path: str) -> dict[str, Any]:
        if self._client is not None:
            response = await self._client.get(f"{self.base_url}{path}")
            response.raise_for_status()
            return _object(response.json())
        async with httpx.AsyncClient(timeout=10.0) as client:
            response = await client.get(f"{self.base_url}{path}")
            response.raise_for_status()
            return _object(response.json())

    async def read(self) -> Floor:
        revision = await self._get("/api/state")
        return Floor(
            version=int(revision.get("version", -1)),
            state=_object(revision.get("state", {})),
            menu=await self._get("/api/menu"),
            summary=await self._get("/api/summary"),
        )

    async def recommendations(self, guest_id: str, rerank: bool = True) -> dict[str, Any]:
        """The engine's ranking.

        `rerank=False` asks ember-server for the engine's own ordering without
        calling back into this service — which is what stops `/rank` recursing
        when it fetches its own input.
        """
        suffix = "" if rerank else "?rerank=false"
        return await self._get(f"/api/recommendations/{guest_id}{suffix}")

    async def action_log(self, since: int = 0, limit: int = 2000) -> list[dict[str, Any]]:
        """The append-only log: what the forecaster and reranker learn from."""
        payload = await self._get(f"/api/actions/log?since={since}&limit={limit}")
        return _rows(payload, "entries")
