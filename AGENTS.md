<!-- BEGIN:nextjs-agent-rules -->

# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` (resolved from this file's directory; in monorepos the `next` package may not be visible from the repo root) before writing any code. Heed deprecation notices.

This block is written and re-added by `next dev` — verify at `node_modules/next/dist/server/lib/generate-agent-files.js`. Removing it from a diff only re-creates the uncommitted change; committing it with your work keeps the tree clean.

<!-- END:nextjs-agent-rules -->

## Cursor Cloud specific instructions

- This repo is a single Next.js 16 + React 19 app ("Ember POS", package `hellskitchen`). It uses **npm** (`package-lock.json`). Standard commands live in `README.md` / `package.json` scripts: `npm run dev`, `npm run build`, `npm run lint`, `npm run typecheck`, `npm test`.
- Only one service needs to run for end-to-end testing: the Next.js dev server (`npm run dev`, http://localhost:3000). The live POS is at `/pos`. There is **no backend/database** — all state persists client-side via `localStorage` + `BroadcastChannel`, so a fresh browser/incognito starts from the seeded demo data. Use the "Reset demo" button in the POS header to restore seeded state.
- The third-party integrations (ElevenLabs voice, Tavily dish context, Stay22 map) are optional and degrade gracefully to typed input / seeded fallbacks / a placeholder map when their API keys are absent. No keys are required to develop, test, or build; set them in `.env.local` (copy from `.env.example`) only when exercising those specific integrations.
