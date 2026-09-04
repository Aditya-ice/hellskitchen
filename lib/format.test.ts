import { describe, expect, it } from "vitest";

import { formatMinutes, formatMoney, plural } from "@/lib/format";

describe("formatMoney", () => {
  it("renders integer minor units as money", () => {
    expect(formatMoney(1800, "USD", "en-US")).toBe("$18.00");
    // The old code interpolated the raw number, so this rendered as "$24.5".
    expect(formatMoney(2450, "USD", "en-US")).toBe("$24.50");
    expect(formatMoney(0, "USD", "en-US")).toBe("$0.00");
  });

  it("respects a currency with no minor unit", () => {
    // Dividing by 100 unconditionally would show this a hundredfold too small.
    expect(formatMoney(1800, "JPY", "en-US")).toBe("¥1,800");
  });

  it("does not throw on a currency the runtime does not know", () => {
    expect(formatMoney(1800, "XYZ", "en-US")).toContain("XYZ");
  });
});

describe("formatMinutes", () => {
  it("reads as minutes below an hour", () => {
    expect(formatMinutes(0)).toBe("0m");
    expect(formatMinutes(42)).toBe("42m");
  });

  it("switches to hours once minutes stop being readable", () => {
    expect(formatMinutes(60)).toBe("1h");
    expect(formatMinutes(94)).toBe("1h 34m");
  });

  it("never shows a negative age", () => {
    // Clock skew between a terminal and the server should not print "-3m".
    expect(formatMinutes(-3)).toBe("0m");
  });
});

describe("plural", () => {
  it("does not say 1 items", () => {
    expect(plural(1, "visit")).toBe("1 visit");
    expect(plural(2, "visit")).toBe("2 visits");
    expect(plural(1, "guest")).toBe("1 guest");
  });
});
