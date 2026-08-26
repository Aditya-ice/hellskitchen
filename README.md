# Ember POS

Ember POS is a guest-focused, AI-assisted front-of-house prototype. It helps a host move a party from arrival to the right table, then helps a server build a safe and relevant order with explainable recommendations.

## What the demo does

- Manages expected guests, check-ins, and walk-ins.
- Scores tables using party size, accessibility, seating preference, wait time, and server load.
- Surfaces guest history, dietary needs, allergies, likes, and service notes.
- Ranks dishes using hard safety constraints, live ingredient availability, preferences, prep time, popularity, and balanced value.
- Captures guest and order notes by ElevenLabs voice transcription or typed fallback.
- Uses Tavily for optional, source-linked dish background.
- Offers a lightweight Stay22 map for guests who need nearby accommodation.
- Keeps every open surface on one live floor, and keeps the service across restarts.

The recommendation engine assists staff; it does not replace allergy verification or staff judgment.

## Architecture

The scoring rules and all state live in Rust, not in the browser. Every surface —
this web app, and the macOS app — is a thin client over the same server, so they
all see one floor.

```
crates/ember-core     domain model, decision engine, state reducer (pure)
crates/ember-store    SQLite: append-only action log + snapshot
crates/ember-server   axum: REST + SSE + static bundle + sponsor proxies
src-tauri/            macOS app: embeds the server, adds the native surfaces
app/ components/ lib  Next.js UI — renders and dispatches, decides nothing
lib/generated/        TypeScript types generated from the Rust structs
```

The macOS app is not a second implementation. It embeds `ember-server`
in-process and points a webview at it, so the desktop and a browser tab run
identical code against one floor.

The action log is both the audit trail and the history the planned Python
services will learn from, so it is only ever appended to.

Hard safety rules — allergens, dietary conflicts, unavailable stock — live in
`ember-core` and gate every recommendation. Nothing downstream may reverse that.

## Setup

Requires Node 20+ and a Rust toolchain.

```bash
npm install
cp .env.example .env.local
```

Run the server and the UI in two terminals:

```bash
npm run dev:server   # ember-server on :4000
npm run dev          # next dev on :3000, proxying /api/* to :4000
```

Open [http://localhost:3000](http://localhost:3000), then choose **Open the live POS**.

To run it the way it ships — one binary serving the built UI and the API:

```bash
npm start            # builds both, serves on :4000
```

Anything else on your network can then open `http://<your-ip>:4000` and share the
same floor.

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
- **⌘1–⌘4** to move between Arrivals, Floor, Order and Guest; **⌘R** to reset
  the demo service.

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
```

`cargo test` also regenerates `lib/generated/` from the Rust types, so the two
languages cannot drift.

## Loom demo script

1. Open **Arrivals** and select Maya Chen. Point out the tree-nut allergy, gluten-free need, anniversary note, and window/accessibility preferences.
2. Show the table recommendations. Explain why T2 scores highest, then seat Maya there.
3. Open a second window side by side and seat a party in one — it appears in the other immediately, with no reload.
4. Open **Order**. Show that unsafe or incompatible dishes are blocked, while available dishes are ranked with plain-language reasons.
5. Add the Golden Beet & Citrus and Cedar Salmon. Mention the live warning that carrots are running low.
6. Dictate an order note with ElevenLabs, or type it if no API key is configured, then send the order.
7. Open **Dish context** to show Tavily's source-linked web context and the allergy disclaimer.
8. Open **Guest concierge** to show the Stay22 accommodation map.
9. Return to **Guest** to show saved notes, the current check, and the activity trail.

Use **Reset demo** in the POS header to restore the seeded state before another recording.
