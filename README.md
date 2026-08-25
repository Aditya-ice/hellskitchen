# Ember POS

Ember POS is a guest-focused, AI-assisted front-of-house system built with a decoupled architecture (Next.js 16 frontend + standalone Hono/Node.js API backend + shared domain logic).

It helps a restaurant host move a party from arrival to the right table, then guides servers toward safe, relevant dishes with explainable recommendations.

---

## Architecture Overview

```
hellskitchen/
├── apps/
│   ├── web/               # Next.js 16 frontend (UI, guest concierge, voice capture)
│   └── api/               # Standalone Hono/Node.js backend (REST API, POS state, sponsor proxy)
├── packages/
│   └── shared/            # Shared domain types, demo catalog data, and recommendation engine
├── package.json           # npm workspaces root
└── README.md
```

```
┌────────────────────────────────┐            ┌────────────────────────────────┐
│   Next.js 16 Web (Vercel)      │            │   Hono/Node.js API (Render)    │
│   - POS Shell & Tabs           │            │   - Live POS State Store       │
│   - Voice Mic Streaming        │            │   - Seating & Order Validation │
│   - Stay22 Accommodation Map   │            │   - ElevenLabs & Tavily Proxy  │
└───────────────┬────────────────┘            └───────────────┬────────────────┘
                │                                             │
                │        REST / CORS (Bearer Demo Token)      │
                └─────────────────────────────────────────────┘
```

---

## Features

- **Arrivals & Seating:** Expected guest list, party sizes, check-in, walk-in creation, and smart table matching based on capacity, accessibility, server balance, and wait times.
- **Dietary-Aware Ordering:** Hard safety constraints for guest allergens, dietary compatibility filters (vegan, vegetarian, gluten-free), low-stock ingredient alerts, prep pacing, and profit margin balancing.
- **Guest Profiles & Activity Trail:** Real-time visit history, notes, and chronological activity logging.
- **ElevenLabs Voice Integration:** Server mints single-use `realtime_scribe` tokens; browser streams microphone input over WebSocket with typed fallback.
- **Tavily Culinary Context:** Web search for ingredient background and dish seasonality with live citations and allergy disclaimers.
- **Stay22 Concierge:** Interactive accommodation map for traveling guests centered on the venue.

---

## Local Development

### 1. Install dependencies

```bash
npm install
```

### 2. Configure environment variables

Copy `.env.example` into workspace apps:

```bash
cp apps/api/.env.example apps/api/.env
cp apps/web/.env.example apps/web/.env.local
```

### 3. Start development servers

Start both API (`http://localhost:4000`) and Web (`http://localhost:3000`):

```bash
npm run dev:api   # In terminal 1 (starts Hono API on port 4000)
npm run dev:web   # In terminal 2 (starts Next.js web on port 3000)
```

Open [http://localhost:3000](http://localhost:3000) and click **Open the live POS**.

---

## Quality Checks & Testing

Run all quality checks across the entire monorepo:

```bash
npm test          # Run Vitest test suite across all workspaces (15 passing tests)
npm run typecheck # TypeScript strict type verification
npm run lint      # ESLint code style & Next.js core vitals check
npm run build     # Production build of shared package, API, and Next.js bundle
```

---

## Deployment Guide (Free & Student Tiers)

### 1. Backend Deployment (Render Free Web Service or Heroku Eco)

- **Repository Root:** `/`
- **Root Directory / Build Filter:** `apps/api`
- **Build Command:** `npm install && npm run build --workspace=@hellskitchen/shared && npm run build --workspace=@hellskitchen/api`
- **Start Command:** `npm run start --workspace=@hellskitchen/api` (or `node apps/api/dist/index.js`)
- **Health Check Path:** `/health`
- **Environment Variables:**
  - `PORT=4000` (or injected by host)
  - `FRONTEND_ORIGIN=https://your-frontend-app.vercel.app` (for CORS)
  - `ELEVENLABS_API_KEY=...` (optional, server secret)
  - `TAVILY_API_KEY=...` (optional, server secret)

### 2. Frontend Deployment (Vercel Hobby)

- **Repository Root:** Import GitHub repo `Aditya-ice/hellskitchen`
- **Production Branch:** `main`
- **Root Directory:** `apps/web`
- **Framework Preset:** Next.js
- **Build Command:** `npm run build`
- **Environment Variables:**
  - `NEXT_PUBLIC_API_URL=https://your-api-service.onrender.com`
  - `NEXT_PUBLIC_STAY22_AID=...` (optional affiliate ID)
  - `NEXT_PUBLIC_RESTAURANT_VENUE="Fordham University, Lincoln Center, New York"`

---

## Polyglot Evolution Roadmap

For high-scale enterprise deployments, specific subsystems can be migrated beyond pure TypeScript:
- **Rust (Wasm / Native gRPC):** NP-hard floor constraint optimization & offline client solver.
- **Go / Elixir:** High-concurrency WebSocket & KDS ticket synchronization gateway.
- **Python (FastAPI + pgvector):** Guest culinary embeddings & predictive table-turn machine learning.
