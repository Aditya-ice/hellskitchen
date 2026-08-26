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

EMBER_URL = os.environ.get("EMBER_URL", "http://127.0.0.1:4000")
EFFORT = os.environ.get("EMBER_BRAIN_EFFORT", "medium")

app = FastAPI(title="Ember POS brain", version="0.1.0")


def _agent() -> FloorAgent:
    return FloorAgent(FloorClient(EMBER_URL), effort=EFFORT)


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
