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
- Persists state locally and syncs same-browser tabs with `BroadcastChannel`.

The recommendation engine assists staff; it does not replace allergy verification or staff judgment.

## Setup

```bash
npm install
cp .env.example .env.local
npm run dev
```

Open [http://localhost:3000](http://localhost:3000), then choose **Open the live POS**.

Environment variables:

- `ELEVENLABS_API_KEY`: server-side key used to mint short-lived Scribe tokens.
- `TAVILY_API_KEY`: server-side search key. The UI uses a seeded fallback without it.
- `NEXT_PUBLIC_STAY22_AID`: Stay22 affiliate ID used by the accommodation map.
- `NEXT_PUBLIC_RESTAURANT_VENUE`: address used to center the Stay22 map.

No secret key is sent to the browser. ElevenLabs receives only a short-lived single-use token.

## Quality checks

```bash
npm run lint
npm run typecheck
npm test
npm run build
```

## Loom demo script

1. Open **Arrivals** and select Maya Chen. Point out the tree-nut allergy, gluten-free need, anniversary note, and window/accessibility preferences.
2. Show the table recommendations. Explain why T2 scores highest, then seat Maya there.
3. Open **Order**. Show that unsafe or incompatible dishes are blocked, while available dishes are ranked with plain-language reasons.
4. Add the Golden Beet & Citrus and Cedar Salmon. Mention the live warning that carrots are running low.
5. Dictate an order note with ElevenLabs, or type it if no API key is configured, then send the order.
6. Open **Dish context** to show Tavily’s source-linked web context and the allergy disclaimer.
7. Open **Guest concierge** to show the Stay22 accommodation map.
8. Return to **Guest** to show saved notes, the current check, and the activity trail.

Use **Reset demo** in the POS header to restore the seeded state before another recording.
