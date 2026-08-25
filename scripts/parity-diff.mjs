/**
 * Diffs the TypeScript and Rust engine dumps. See scripts/parity-dump.test.ts.
 *
 * Usage: node scripts/parity-diff.mjs <ts.json> <rs.json>
 * Exits non-zero on any difference.
 */
import { readFileSync } from "node:fs";

const [tsPath, rsPath] = process.argv.slice(2);
if (!tsPath || !rsPath) {
  console.error("usage: node scripts/parity-diff.mjs <ts.json> <rs.json>");
  process.exit(2);
}

// Two representational differences are not real differences: Rust emits 100.0
// where JSON.stringify emits 100, and serde_json::Value orders keys
// alphabetically while the TypeScript object literal keeps insertion order.
// Sort keys so the comparison is about values only.
const normalize = (value) =>
  Array.isArray(value)
    ? value.map(normalize)
    : value && typeof value === "object"
      ? Object.fromEntries(
          Object.entries(value)
            .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0))
            .map(([key, item]) => [key, normalize(item)]),
        )
      : value;

const load = (path) => normalize(JSON.parse(readFileSync(path, "utf8")));
const ts = load(tsPath);
const rs = load(rsPath);

let differences = 0;

const report = (label, left, right) => {
  differences += 1;
  console.error(`\n${label} differs`);
  console.error(`  ts: ${JSON.stringify(left)}`);
  console.error(`  rs: ${JSON.stringify(right)}`);
};

for (const [index, guest] of ts.entries()) {
  const other = rs[index];
  if (!other) {
    report(`[${index}]`, guest.guestId, undefined);
    continue;
  }
  if (guest.estimateWait !== other.estimateWait) {
    report(`${guest.guestId}.estimateWait`, guest.estimateWait, other.estimateWait);
  }
  // Compare recommendation-by-recommendation so a single changed score points
  // at one dish rather than dumping the whole ranking.
  for (const key of ["tables", "dishes"]) {
    const left = guest[key];
    const right = other[key] ?? [];
    if (left.length !== right.length) {
      report(`${guest.guestId}.${key}.length`, left.length, right.length);
    }
    for (let i = 0; i < Math.max(left.length, right.length); i += 1) {
      if (JSON.stringify(left[i]) === JSON.stringify(right[i])) continue;
      report(`${guest.guestId}.${key}[${i}] (${left[i]?.id ?? right[i]?.id})`, left[i], right[i]);
    }
  }
}

if (differences > 0) {
  console.error(
    `\n${differences} differing ${differences === 1 ? "entry" : "entries"}`,
  );
  process.exit(1);
}

const scored = ts.reduce(
  (total, guest) => total + guest.tables.length + guest.dishes.length,
  0,
);
console.log(
  `identical — ${ts.length} guests, ${scored} scored recommendations match exactly`,
);
