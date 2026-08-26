"""The floor agent: natural-language questions about the live service.

Read-only by construction. Every tool is a query, so the worst a wrong answer
can do is mislead someone who can see the same screen — it cannot seat a party,
fire an order, or move stock.

The safety rules stay in Rust. `ember-core` decides which dishes a guest may be
sold, and those decisions arrive here already made: a blocked dish is labelled
BLOCKED with its reason. The agent's job is to read the ranking out loud, not
to reconsider it.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime, timezone
from typing import Any

from anthropic import AsyncAnthropic, beta_async_tool

from .floor import (
    FloorClient,
    describe_floor,
    describe_guest,
    describe_recommendations,
    describe_stock,
    describe_tickets,
)

MODEL = "claude-opus-5"

SYSTEM_PROMPT = """You are the floor agent for Ember & Ash, a restaurant. \
You answer questions from hosts and servers about the service happening right now.

Use the tools to read the live floor before answering. Never answer from memory \
or assumption — if you have not read it, look it up.

Rules that are not yours to bend:
- Allergy and dietary decisions are made by the POS engine, not by you. When a \
dish is marked BLOCKED, it cannot be sold to that guest, whatever the reason \
looks like. Never suggest a workaround, a substitution that removes an \
allergen, or that staff check whether it is "really" a problem.
- Never claim a dish is safe. You may report what the engine decided.
- You cannot change anything: no seating, no orders, no stock. If asked to do \
something, say who can do it and where in the POS.

Style: you are talking to someone mid-service. Lead with the answer. Two or \
three sentences unless asked for more. Use table labels and guest names, not \
internal ids. No preamble, no restating the question."""


@dataclass
class AgentAnswer:
    text: str
    tools_used: list[str] = field(default_factory=list)
    model: str = MODEL
    stop_reason: str | None = None


class FloorAgent:
    """Wraps the tool-runner loop over a read-only view of the POS."""

    def __init__(
        self,
        floor_client: FloorClient,
        anthropic_client: AsyncAnthropic | None = None,
        *,
        model: str = MODEL,
        effort: str = "medium",
    ) -> None:
        self.floor_client = floor_client
        self.model = model
        self.effort = effort
        # Constructed lazily so the service can start, and report itself
        # unconfigured, without credentials present.
        self._anthropic = anthropic_client

    @property
    def anthropic(self) -> AsyncAnthropic:
        if self._anthropic is None:
            self._anthropic = AsyncAnthropic()
        return self._anthropic

    # --- what the tools actually do -------------------------------------
    #
    # Kept separate from the decorators below so the behaviour can be tested
    # without going near the SDK. The decorated functions are glue.

    async def read_floor(self) -> str:
        return describe_floor(await self.floor_client.read())

    async def read_guest(self, name_or_id: str) -> str:
        floor = await self.floor_client.read()
        guest = floor.find_guest(name_or_id)
        if guest is None:
            known = ", ".join(g["name"] for g in floor.guests())
            return f"No guest matching {name_or_id!r}. Parties in tonight: {known}."
        return describe_guest(floor, guest)

    async def read_stock(self) -> str:
        return describe_stock(await self.floor_client.read())

    async def read_tickets(self, now: datetime | None = None) -> str:
        return describe_tickets(
            await self.floor_client.read(), now or datetime.now(timezone.utc)
        )

    async def read_ranking(self, name_or_id: str) -> str:
        floor = await self.floor_client.read()
        guest = floor.find_guest(name_or_id)
        if guest is None:
            return f"No guest matching {name_or_id!r}."
        payload = await self.floor_client.recommendations(guest["id"])
        return describe_recommendations(payload, floor)

    def _tools(self, used: list[str]) -> list[Any]:
        agent = self

        @beta_async_tool
        async def query_floor() -> str:
            """Read the whole floor: every party, their status and allergies, and every table.

            Use this for questions about who is here, who is waiting, what is free,
            or how the service is going.
            """
            used.append("query_floor")
            return await agent.read_floor()

        @beta_async_tool
        async def query_guest(name_or_id: str) -> str:
            """Read one guest in full: allergies, dietary needs, preferences, history and their current order.

            Args:
                name_or_id: The guest's name as staff would say it, or their id.
            """
            used.append("query_guest")
            return await agent.read_guest(name_or_id)

        @beta_async_tool
        async def query_stock() -> str:
            """Read ingredient stock, which dishes each ingredient is used in, and what is low or out."""
            used.append("query_stock")
            return await agent.read_stock()

        @beta_async_tool
        async def query_tickets() -> str:
            """Read the open kitchen tickets and how long each has been on the pass."""
            used.append("query_tickets")
            return await agent.read_tickets()

        @beta_async_tool
        async def rank_for_guest(name_or_id: str) -> str:
            """Read the engine's ranking of dishes and tables for one guest.

            This is the authority on what a guest may be sold. Dishes marked
            BLOCKED cannot be served to them.

            Args:
                name_or_id: The guest's name as staff would say it, or their id.
            """
            used.append("rank_for_guest")
            return await agent.read_ranking(name_or_id)

        return [query_floor, query_guest, query_stock, query_tickets, rank_for_guest]

    async def ask(self, question: str, max_tokens: int = 4096) -> AgentAnswer:
        used: list[str] = []
        runner = self.anthropic.beta.messages.tool_runner(
            model=self.model,
            max_tokens=max_tokens,
            system=SYSTEM_PROMPT,
            thinking={"type": "adaptive"},
            output_config={"effort": self.effort},
            tools=self._tools(used),
            messages=[{"role": "user", "content": question}],
        )

        final = None
        async for message in runner:
            final = message

        if final is None:
            return AgentAnswer(text="No answer was produced.", tools_used=used)

        # A safety decline is a real outcome, not an error: surface it plainly
        # rather than presenting an empty answer as if it were one.
        if final.stop_reason == "refusal":
            return AgentAnswer(
                text="I can't answer that one. Ask a colleague or check the POS directly.",
                tools_used=used,
                model=self.model,
                stop_reason=final.stop_reason,
            )

        text = "\n".join(
            block.text for block in final.content if getattr(block, "type", None) == "text"
        ).strip()

        return AgentAnswer(
            text=text or "No answer was produced.",
            tools_used=used,
            model=self.model,
            stop_reason=final.stop_reason,
        )
