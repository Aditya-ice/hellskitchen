# Ember POS

Ember POS is a guest-focused, AI-assisted front-of-house prototype. It helps a host move a party from arrival to the right table, then helps a server build a safe and relevant order with explainable recommendations.

## What the demo does

- Signs staff in by PIN on a shared terminal, and records every action against
  whoever performed it.
- Manages expected guests, check-ins, and walk-ins.
- Scores tables using party size, accessibility, seating preference, wait time, and server load.
- Surfaces guest history, dietary needs, allergies, likes, and service notes.
- Ranks dishes using hard safety constraints, live ingredient availability, preferences, prep time, popularity, and balanced value.
- Consumes stock when a ticket is fired, so low-stock warnings fire during a service and a dish goes dark once its last portion is committed. The Larder panel books deliveries back in.
- Captures guest and order notes by ElevenLabs voice transcription or typed fallback.
- Uses Tavily for optional, source-linked dish background.
- Offers a lightweight Stay22 map for guests who need nearby accommodation.
- Keeps every open surface on one live floor, and keeps the service across restarts.
- Enforces the allergen and dietary rules on the server, not only in the browser.

The recommendation engine assists staff; it does not replace allergy verification or staff judgment.

## Architecture

The scoring rules and all state live in Rust, not in the browser. Every surface —
this web app, and the macOS app — is a thin client over the same server, so they
all see one floor.

```
crates/ember-core     domain model, decision engine, state reducer (pure)
crates/ember-store    SQLite: append-only action log + snapshot
crates/ember-server   axum: REST + SSE + static bundle + sponsor proxies
services/brain        optional Python service (floor agent). The POS runs without it.
src-tauri/            macOS app: embeds the server, adds the native surfaces
app/ components/ lib  Next.js UI — renders and dispatches, decides nothing
lib/generated/        TypeScript types generated from the Rust structs
```

The macOS app is not a second implementation. It embeds `ember-server`
in-process and points a webview at it, so the desktop and a browser tab run
identical code against one floor.

The action log is both the audit trail and the history the planned Python
services will learn from, so it is only ever appended to.

Stock is part of the state, not reference data: firing a ticket consumes it.
One unit of each listed ingredient per serving — the menu carries no per-dish
quantities, so that is the honest reading of the data available. Real recipe
quantities would replace `consumption()` in `crates/ember-core/src/reducer.rs`
and nothing else. Restocking is additive rather than "set to N", so two people
booking in a delivery at once add up instead of overwriting each other.

Hard safety rules — allergens, dietary conflicts, unavailable stock — live in
`ember-core` as a single function, `engine::dish_obstacle`. The ranking calls it
to decide what a guest may be shown, and the reducer calls it again before a
line reaches a check, so a dish that is blocked on screen cannot be ordered
through the API by a second terminal, a replayed action, or a direct POST. A
test walks every guest against every dish in the seed and asserts the two paths
agree.

Allergens and dietary conflicts are re-checked when the ticket is fired, because
a guest's allergy record can be corrected after the order was started. Stock is
deliberately not: the last portion going while a party was choosing is not a
reason to strand them.

## Model services

`services/brain` adds three things on top of the POS. All of them are optional,
and only the first needs credentials — the other two are arithmetic over the
action log.

**Forecasting** projects where stock is heading from what has actually been
consumed, and warns before a dish comes off the menu. **Reranking** reorders the
engine's suggestions using what has been ordered tonight. Neither is a fitted
model: with a service's worth of tickets there is nothing honest to fit, so both
are transparent estimators that report their own confidence and degrade to the
engine's own answer when the evidence is thin. `burn_per_hour` and the ranking
score are what a real model would replace; the interfaces would not change.

Two rules keep the reranker safe to bolt on. It only reorders dishes the engine
has already cleared, and `ember-server` verifies on every response that the
returned ranking has exactly the engine's eligibility — a reranker that tried to
unblock a dish is discarded whole. Allergy decisions are never a model's to make.

### Floor agent

The agent answers natural-language questions about the service happening
right now — "who has been waiting longest?", "what can I sell that uses up the
carrots?". Optional in the strongest sense: not running, running without
credentials, and failing mid-answer all produce a readable answer rather than
an error, and the POS behaves identically either way.

```bash
npm run brain    # http://127.0.0.1:4100
```

The agent needs `ANTHROPIC_API_KEY`; forecasting and reranking do not.

Then point the server at it with `EMBER_BRAIN_URL=http://127.0.0.1:4100`, and
the brain back at the server with `EMBER_URL`. Everything reaches it through
`ember-server`, so the model credentials never go near the browser.

The agent is read-only by construction — all five of its tools are queries, so
it can advise but never seat a party, fire an order or move stock. Allergy and
availability decisions arrive from `ember-core` already made: a blocked dish is
shown to the model as BLOCKED with its reason, and the system prompt forbids
working around one. `uv run --project services/brain python
services/brain/tools_preview.py` prints exactly what the model is shown, against
a live POS and without calling the model.

Not yet wired to the desktop app: the brain needs a stable URL for the POS, and
the desktop binds an ephemeral port. That is part of the sidecar packaging work.

## Setup

Requires Node 20+ and a Rust toolchain.

```bash
npm install
cp .env.example .env.local
```

`EMBER_DB` is required — the server refuses to start without it rather than
keeping the whole service in memory and losing it on the next restart. Set
`EMBER_EPHEMERAL=1` if an in-memory run is genuinely what you want.

Run the server and the UI in two terminals:

```bash
npm run dev:server   # ember-server on :4000
npm run dev          # next dev on :3000, proxying /api/* to :4000
```

To run it the way it ships — one binary serving the built UI and the API:

```bash
npm start            # builds both, serves on :4000
```

Anything else on your network can then open `http://<your-ip>:4000` and share the
same floor.

### Signing in

Nothing is reachable without a staff session: the floor, the menu and the event
stream all carry guest names, allergies and dietary needs, and every action is
recorded against whoever performed it.

On a terminal where nobody has a PIN yet, the server prints a one-time setup
code at startup:

```
WARN ember_server: FIRST RUN — nobody has a PIN yet. Setup code for the first
manager: a17683212c700296…
```

Open the app, enter that code with a manager's staff id (`manager-1` in the
seeded roster) and a 4–12 digit PIN. The code is required because this route
cannot ask who you are — there is nobody to be yet — so without it whoever
reached the port first would become the manager. It is spent as soon as the
first PIN is set. `EMBER_SETUP_TOKEN` pins it to a known value when provisioning
from a script.

After that, a manager adds everyone else from **Staff PINs** in the header.
Five wrong attempts lock an account for five minutes; a manager resetting the
PIN clears the lockout, and also signs that person out of every terminal.

Sessions expire after 30 minutes idle. Serve over https and set
`EMBER_SECURE_COOKIES=1` in a venue — otherwise the session cookie travels in
the clear and anyone on the same network can lift it.

## macOS app

```bash
npm run desktop        # run it from source
npm run desktop:build  # build target/release/bundle/macos/Ember POS.app
```

The app carries its own copy of the UI and its own server, so it needs nothing
else running. Its service lives in
`~/Library/Application Support/com.emberpos.desktop/ember.db` and survives
reinstalling the app.

What the native shell adds over a browser tab:

- **Menu bar** — live ticket count and the age of the oldest ticket.
- **Kitchen Display** (⌘K) — a ticket rail for a second screen above the pass.
  Tickets turn amber at 10 minutes and red at 20, and are bumped from the rail
  when the food goes out. Bumping is the one thing the kitchen owns; it clears
  the ticket and the menu-bar count, and says nothing about the table — the
  party is still sitting there eating.
- **Notifications** — when an order is fired, and when a party with recorded
  allergies is seated.
- **⌘1–⌘4** to move between Arrivals, Floor, Order and Guest.

`bundle.targets` is `["app"]`. Adding `"dmg"` also works, but `bundle_dmg.sh`
drives Finder through AppleScript and needs Automation permission granted to
whichever terminal runs the build.

Environment variables are documented in `.env.example`. Without
`ELEVENLABS_API_KEY` the voice input falls back to typing; without
`TAVILY_API_KEY` dish context returns seeded text. Neither key reaches the
browser.

## Quality checks

```bash
npm run lint
npm run typecheck
npm test          # UI client layer
npm run test:rust # engine, reducer, store, server
npm run test:e2e  # a service end to end, in a real browser

cd services/brain && uv run pytest   # floor agent
```

`cargo test` also regenerates `lib/generated/` from the Rust types, so the two
languages cannot drift.

## Loom demo script

1. Start the server, take the setup code from its first log line, and set the
   first manager PIN. Point out that this is the only thing standing between a
   fresh venue machine and whoever else is on its network.
2. Open **Arrivals** and select Maya Chen. Point out the tree-nut allergy,
   gluten-free need, anniversary note, and window/accessibility preferences.
3. Show the table recommendations. Explain why T2 scores highest, then seat Maya
   there.
4. Open a second window side by side and seat a party in one — it appears in the
   other immediately, with no reload.
5. Open **Order**. The Charred Carrot Tartare is blocked for her and says why.
   That rule is enforced in `ember-core`, not in the browser: a second terminal
   posting the same order directly is refused too.
6. Add the Golden Beet & Citrus and Cedar Salmon. Mention the live warning that
   carrots are running low.
7. Dictate an order note with ElevenLabs, or type it if no API key is
   configured, then send the order.
8. Open **Dish context** to show Tavily's source-linked web context and the
   allergy disclaimer.
9. Return to **Guest** to show saved notes, the current check, and the activity
   trail — every entry attributed to whoever is signed in.
