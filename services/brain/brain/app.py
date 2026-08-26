"""HTTP surface for the brain service.

Deliberately small. `ember-server` is the only caller, it reaches this over
loopback, and the POS runs perfectly well when this process is not there — so
the contract is one health check and one question endpoint.
"""

from __future__ import annotations

import os

from fastapi import FastAPI
from pydantic import BaseModel, Field

from .agent import MODEL, FloorAgent
from .floor import FloorClient
from .forecast import build_forecast
from .history import replay
from .rank import build_ranking

EMBER_URL = os.environ.get("EMBER_URL", "http://127.0.0.1:4000")
EFFORT = os.environ.get("EMBER_BRAIN_EFFORT", "medium")

app = FastAPI(title="Ember POS brain", version="0.1.0")


def _floor_client() -> FloorClient:
    return FloorClient(EMBER_URL)


def _agent() -> FloorAgent:
    return FloorAgent(_floor_client(), effort=EFFORT)


def _has_credentials() -> bool:
    """Whether the Anthropic SDK will find a credential.

    An unset ANTHROPIC_API_KEY does not mean there is none — the SDK also reads
    ANTHROPIC_AUTH_TOKEN and an `ant auth login` profile — so this checks for a
    profile on disk too rather than assuming.
    """
    if os.environ.get("ANTHROPIC_API_KEY") or os.environ.get("ANTHROPIC_AUTH_TOKEN"):
        return True
    profile = os.path.expanduser("~/.config/anthropic")
    return os.path.isdir(profile) and bool(os.listdir(profile))


class RankRequest(BaseModel):
    guest_id: str = Field(min_length=1, max_length=200, alias="guestId")
    #: The engine's ranking. Supplied by ember-server so this does not have to
    #: fetch it back — which would recurse, because that endpoint calls here.
    dishes: list[dict] | None = None

    model_config = {"populate_by_name": True}


class Question(BaseModel):
    question: str = Field(min_length=1, max_length=2000)


class Answer(BaseModel):
    answer: str
    tools_used: list[str] = []
    model: str = MODEL
    configured: bool = True


@app.get("/health")
async def health() -> dict[str, object]:
    return {
        "ok": True,
        "service": "ember-brain",
        "model": MODEL,
        "effort": EFFORT,
        "ember_url": EMBER_URL,
        "configured": _has_credentials(),
    }


@app.get("/forecast")
async def forecast(horizon_minutes: float = 90.0) -> dict[str, object]:
    """Where stock is heading, and how busy the next hour looks.

    Needs no model credentials: this is arithmetic over the action log, so it
    works whether or not the agent is configured.
    """
    client = _floor_client()
    try:
        floor = await client.read()
        history = replay(await client.action_log())
    except Exception as error:  # noqa: BLE001 — an optional service must not shout
        print(f"forecast failed: {type(error).__name__}: {error}")
        return {"available": False, "reason": "Could not read the service."}

    result = build_forecast(
        history,
        floor.ingredients(),
        floor.menu_items(),
        horizon_minutes=max(15.0, min(horizon_minutes, 480.0)),
    )
    return {"available": True, **result}


@app.post("/rank")
async def rank(body: RankRequest) -> dict[str, object]:
    """Reranks the engine's suggestions for one guest using tonight's orders.

    Also needs no credentials. Eligibility is never touched — see rank.py.
    """
    client = _floor_client()
    try:
        floor = await client.read()
        guest = floor.find_guest(body.guest_id)
        if guest is None:
            return {"available": False, "reason": f"No guest matching {body.guest_id!r}."}
        dishes = body.dishes
        if dishes is None:
            # Standalone use. `rerank=false` stops ember-server calling back
            # into this endpoint and looping.
            payload = await client.recommendations(guest["id"], rerank=False)
            dishes = payload.get("dishes", [])
        history = replay(await client.action_log())
    except Exception as error:  # noqa: BLE001
        print(f"rank failed: {type(error).__name__}: {error}")
        return {"available": False, "reason": "Could not read the service."}

    return {"available": True, **build_ranking(dishes, history, floor.guests(), guest)}


@app.post("/ask", response_model=Answer)
async def ask(body: Question) -> Answer:
    if not _has_credentials():
        return Answer(
            answer=(
                "The floor agent is not configured. Set ANTHROPIC_API_KEY for the "
                "brain service, or run `ant auth login`."
            ),
            configured=False,
        )

    try:
        result = await _agent().ask(body.question)
    except Exception as error:  # noqa: BLE001 — the POS must survive any failure here
        # The agent is an enhancement. A failure here is reported as an answer
        # the staff can act on, never as an error that interrupts service.
        print(f"floor agent failed: {type(error).__name__}: {error}")
        return Answer(
            answer="The floor agent is unavailable right now. The POS is unaffected.",
            configured=True,
        )

    return Answer(answer=result.text, tools_used=result.tools_used, model=result.model)
