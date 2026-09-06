import { describe, expect, it } from "vitest";
import { todayLabel } from "@/lib/clock";

describe("service date label", () => {
  it("names the weekday and the date", () => {
    // A Wednesday.
    const label = todayLabel(new Date(2026, 7, 26));
    expect(label).toContain("Wednesday");
    expect(label).toContain("August");
    expect(label).toContain("26");
  });

  it("tracks the actual day rather than a build-time constant", () => {
    // The bug this replaced: the header read a fixed "Sunday dinner · August 9"
    // that was baked into the static export.
    const first = todayLabel(new Date(2026, 7, 26));
    const second = todayLabel(new Date(2026, 7, 27));
    expect(first).not.toBe(second);
  });
});
