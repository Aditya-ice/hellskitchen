import { describe, expect, it } from "vitest";
import type { Order, OrderStatus } from "@/lib/domain";
import { isLockedOrder, isOpenOrder, orderStageLabel } from "@/lib/orders";

function order(status: OrderStatus, lines: Order["lines"] = []): Order {
  return {
    id: "order-1",
    guestId: "guest-maya",
    tableId: "t2",
    status,
    lines,
    guestNotes: "",
    createdAt: "2026-08-13T10:00:00.000Z",
    sentAt: status === "draft" ? null : "2026-08-13T10:05:00.000Z",
    completedAt: status === "completed" ? "2026-08-13T10:30:00.000Z" : null,
  };
}

describe("which orders accept edits", () => {
  it("only a draft is open", () => {
    expect(isOpenOrder(order("draft"))).toBe(true);
    expect(isOpenOrder(order("sent"))).toBe(false);
    expect(isOpenOrder(order("completed"))).toBe(false);
  });

  it("a completed ticket is locked, not re-openable", () => {
    // The check this replaced was `status === "sent"`, which would have called
    // a completed ticket editable and lit up every order control again.
    expect(isLockedOrder(order("completed"))).toBe(true);
  });

  it("a guest with no order yet has nothing to lock", () => {
    // Distinct from `!isOpenOrder`: controls must not grey out just because a
    // party has not been seated.
    expect(isOpenOrder(undefined)).toBe(false);
    expect(isLockedOrder(undefined)).toBe(false);
  });

  it("locks exactly the statuses that are not open", () => {
    const statuses: OrderStatus[] = ["draft", "sent", "completed"];
    for (const status of statuses) {
      expect(isLockedOrder(order(status))).toBe(!isOpenOrder(order(status)));
    }
  });

  it("refuses to guess at a status it does not know", () => {
    // If OrderStatus ever grows a fourth value, TypeScript fails the build at
    // the switch. This covers the runtime case where untyped data reaches it —
    // failing loudly beats silently treating an unknown state as editable.
    const rogue = { ...order("draft"), status: "voided" as OrderStatus };
    expect(() => isOpenOrder(rogue)).toThrow(/Unhandled order status/);
  });
});

describe("order stage label", () => {
  it("names each stage", () => {
    expect(orderStageLabel(order("draft"))).toBe("Send order");
    expect(orderStageLabel(order("sent"))).toBe("Sent to kitchen");
    expect(orderStageLabel(order("completed"))).toBe("Away from the pass");
    expect(orderStageLabel(undefined)).toBe("Send order");
  });
});
