"""Agent behaviour, without going near the network.

The tool bodies are exercised directly against a stub POS. `ask()` is
exercised against a stub Anthropic client, so these tests need no credentials
and make no requests.
"""

from dataclasses import dataclass
from datetime import datetime, timezone

import pytest

from brain.agent import SYSTEM_PROMPT, FloorAgent
from tests.test_floor import PAYLOAD, floor


class StubFloorClient:
    """Stands in for ember-server."""

    def __init__(self, the_floor=None, payload=None):
        self.the_floor = the_floor or floor()
        self.payload = payload or PAYLOAD
        self.reads = 0

    async def read(self):
        self.reads += 1
        return self.the_floor

    async def recommendations(self, guest_id: str):
        return {**self.payload, "guestId": guest_id}


def agent(client=None) -> FloorAgent:
    return FloorAgent(client or StubFloorClient(), anthropic_client=object())


class TestTools:
    async def test_read_floor_describes_the_service(self):
        text = await agent().read_floor()
        assert "Maya Chen" in text
        assert "T2" in text

    async def test_read_guest_finds_someone_by_first_name(self):
        assert "Maya Chen" in await agent().read_guest("maya")

    async def test_an_unknown_guest_gets_the_guest_list_rather_than_a_dead_end(self):
        # The model can then correct itself in the same turn instead of
        # inventing somebody.
        text = await agent().read_guest("Wilhelmina")
        assert "No guest matching" in text
        assert "Maya Chen" in text

    async def test_read_stock_reports_what_is_out(self):
        assert "OUT OF STOCK" in await agent().read_stock()

    async def test_read_tickets_reports_age(self):
        text = await agent().read_tickets(datetime(2026, 8, 26, 18, 34, tzinfo=timezone.utc))
        assert "24 min old" in text

    async def test_read_ranking_passes_blocks_through(self):
        text = await agent().read_ranking("maya")
        assert "BLOCKED" in text

    async def test_read_ranking_refuses_to_guess_at_an_unknown_guest(self):
        assert "No guest matching" in await agent().read_ranking("Nobody")

    async def test_every_tool_reads_live_rather_than_caching(self):
        # A cached floor would let the agent answer with a service that has
        # since moved on.
        client = StubFloorClient()
        subject = agent(client)
        await subject.read_floor()
        await subject.read_stock()
        await subject.read_tickets()
        assert client.reads == 3


class TestToolSchema:
    def test_exposes_exactly_the_read_only_tools(self):
        names = {tool.name for tool in agent()._tools([])}
        assert names == {
            "query_floor",
            "query_guest",
            "query_stock",
            "query_tickets",
            "rank_for_guest",
        }

    def test_no_tool_can_change_the_service(self):
        # The agent advises; it never acts. If a write tool is ever added this
        # should be a deliberate decision, not an accident.
        forbidden = ("seat", "order", "send", "restock", "complete", "reset", "walk")
        for tool in agent()._tools([]):
            assert not any(word in tool.name for word in forbidden), tool.name

    def test_records_which_tools_were_used(self):
        used: list[str] = []
        tools = agent()._tools(used)
        assert used == []
        assert len(tools) == 5


class TestSystemPrompt:
    def test_hands_allergy_decisions_to_the_engine(self):
        assert "not by you" in SYSTEM_PROMPT
        assert "BLOCKED" in SYSTEM_PROMPT

    def test_forbids_working_around_an_allergen(self):
        assert "workaround" in SYSTEM_PROMPT
        assert "substitution that removes an allergen" in SYSTEM_PROMPT

    def test_forbids_claiming_a_dish_is_safe(self):
        assert "Never claim a dish is safe" in SYSTEM_PROMPT

    def test_states_that_it_cannot_change_anything(self):
        assert "You cannot change anything" in SYSTEM_PROMPT


# --- ask() ---------------------------------------------------------------


@dataclass
class Block:
    type: str
    text: str = ""


@dataclass
class Message:
    content: list
    stop_reason: str = "end_turn"


class StubRunner:
    def __init__(self, messages):
        self.messages = messages

    def __aiter__(self):
        async def gen():
            for message in self.messages:
                yield message

        return gen()


class StubAnthropic:
    def __init__(self, messages):
        self.messages_to_yield = messages
        self.kwargs = None
        outer = self

        class Messages:
            def tool_runner(self, **kwargs):
                outer.kwargs = kwargs
                return StubRunner(outer.messages_to_yield)

        class Beta:
            messages = Messages()

        self.beta = Beta()


def agent_with(messages) -> FloorAgent:
    return FloorAgent(StubFloorClient(), anthropic_client=StubAnthropic(messages))


class TestAsk:
    async def test_returns_the_final_text(self):
        subject = agent_with([Message([Block("text", "Priya has waited longest, 47 minutes.")])])
        answer = await subject.ask("who has waited longest?")
        assert answer.text == "Priya has waited longest, 47 minutes."

    async def test_uses_the_last_message_not_the_first(self):
        # Earlier iterations are tool-call turns; the answer is the last one.
        subject = agent_with(
            [
                Message([Block("text", "Let me check.")], stop_reason="tool_use"),
                Message([Block("text", "T9 is free.")]),
            ]
        )
        assert (await subject.ask("anything free?")).text == "T9 is free."

    async def test_asks_for_adaptive_thinking_on_the_pinned_model(self):
        subject = agent_with([Message([Block("text", "ok")])])
        await subject.ask("hello")
        kwargs = subject.anthropic.kwargs
        assert kwargs["model"] == "claude-opus-5"
        assert kwargs["thinking"] == {"type": "adaptive"}
        assert kwargs["output_config"] == {"effort": "medium"}
        assert kwargs["system"] is SYSTEM_PROMPT

    async def test_a_refusal_is_reported_plainly(self):
        # An empty answer presented as an answer would be worse than saying so.
        subject = agent_with([Message([], stop_reason="refusal")])
        answer = await subject.ask("something declined")
        assert "can't answer that one" in answer.text
        assert answer.stop_reason == "refusal"

    async def test_an_empty_response_does_not_look_like_an_answer(self):
        subject = agent_with([Message([])])
        assert (await subject.ask("hello")).text == "No answer was produced."

    async def test_a_runner_that_yields_nothing_does_not_crash(self):
        subject = agent_with([])
        assert (await subject.ask("hello")).text == "No answer was produced."

    async def test_ignores_non_text_blocks(self):
        subject = agent_with(
            [Message([Block("thinking"), Block("text", "Two tables free."), Block("tool_use")])]
        )
        assert (await subject.ask("q")).text == "Two tables free."
