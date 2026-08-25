/**
 * Parity harness for the TypeScript → Rust engine port.
 *
 * Dumps the TypeScript engine's full output for every seeded guest so it can be
 * diffed against `cargo run -p ember-core --example dump`. Both sides must agree
 * exactly — scores, reason strings, warning strings, and ordering.
 *
 * Skipped unless PARITY_OUT is set, so it stays out of the way of `npm test`:
 *
 *   PARITY_OUT=/tmp/ts.json npx vitest run scripts/parity-dump.test.ts
 *   cargo run -q -p ember-core --example dump > /tmp/rs.json
 *   node scripts/parity-diff.mjs /tmp/ts.json /tmp/rs.json
 *
 * This file goes away with `lib/decision-engine.ts` in Phase 3.
 */
import { writeFileSync } from "node:fs";
import { it } from "vitest";
import { demoGuests, demoTables } from "@/data/demo";
import {
  estimateWait,
  recommendDishes,
  recommendTables,
} from "@/lib/decision-engine";

const out = process.env.PARITY_OUT;

it.skipIf(!out)("dumps the TypeScript engine output for parity diffing", () => {
  const dump = demoGuests.map((guest) => ({
    guestId: guest.id,
    estimateWait: estimateWait(guest, demoTables),
    tables: recommendTables(guest, demoTables),
    dishes: recommendDishes(guest),
  }));
  writeFileSync(out!, JSON.stringify(dump, null, 2));
});
