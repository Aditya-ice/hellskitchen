"""Prints exactly what each tool would hand the model, against a live POS.

No model call and no credentials: this exercises the read path and the
formatters against a real `ember-server`, which is where a wrong answer would
actually come from.

    uv run --project services/brain python services/brain/tools_preview.py
"""

import asyncio
import os
import sys

from brain.agent import FloorAgent
from brain.floor import FloorClient

EMBER_URL = os.environ.get("EMBER_URL", "http://127.0.0.1:4000")


async def main() -> int:
    agent = FloorAgent(FloorClient(EMBER_URL), anthropic_client=object())
    guest = sys.argv[1] if len(sys.argv) > 1 else "maya"

    for title, view in [
        ("query_floor", await agent.read_floor()),
        (f"query_guest({guest!r})", await agent.read_guest(guest)),
        ("query_stock", await agent.read_stock()),
        ("query_tickets", await agent.read_tickets()),
        (f"rank_for_guest({guest!r})", await agent.read_ranking(guest)),
    ]:
        print(f"\n{'=' * 70}\n{title}\n{'=' * 70}")
        print(view)
    return 0


if __name__ == "__main__":
    raise SystemExit(asyncio.run(main()))
