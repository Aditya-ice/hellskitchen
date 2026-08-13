import { describe, expect, it } from "vitest";
import {
  createInitialPosState,
  reducePosState,
  type SharedAction,
  type SharedActionInput,
} from "@/components/pos-provider";

const action = (
  value: SharedActionInput,
): SharedAction =>
  ({
    ...value,
    id: "test-action",
    at: "2026-08-13T10:00:00.000Z",
  }) as SharedAction;

describe("POS state transitions", () => {
  it("rejects invalid seating and accepts a compatible waiting guest", () => {
    const initial = createInitialPosState();
    const expectedGuest = reducePosState(
      initial,
      action({ type: "seat-guest", guestId: "guest-jordan", tableId: "t7" }),
    );
    const occupiedTable = reducePosState(
      initial,
      action({ type: "seat-guest", guestId: "guest-maya", tableId: "t3" }),
    );
    const valid = reducePosState(
      initial,
      action({ type: "seat-guest", guestId: "guest-maya", tableId: "t2" }),
    );

    expect(expectedGuest).toBe(initial);
    expect(occupiedTable).toBe(initial);
    expect(valid.tables.find((table) => table.id === "t2")).toMatchObject({
      status: "occupied",
      seatedGuestId: "guest-maya",
    });
  });

  it("checks in expected guests only once", () => {
    const initial = createInitialPosState();
    const checkedIn = reducePosState(
      initial,
      action({ type: "check-in", guestId: "guest-jordan" }),
    );
    const repeated = reducePosState(
      checkedIn,
      action({ type: "check-in", guestId: "guest-jordan" }),
    );

    expect(checkedIn.guests.find((guest) => guest.id === "guest-jordan")?.status).toBe(
      "waiting",
    );
    expect(checkedIn.activity).toHaveLength(1);
    expect(repeated).toBe(checkedIn);
  });

  it("keeps sent orders immutable", () => {
    const initial = createInitialPosState();
    const state = {
      ...initial,
      orders: initial.orders.map((order) => ({
        ...order,
        status: "sent" as const,
        guestNotes: "Original note",
        lines: [{ menuItemId: "beet-salad", quantity: 1, notes: "" }],
      })),
    };

    const withAddedItem = reducePosState(
      state,
      action({
        type: "add-order-item",
        guestId: "guest-noah",
        menuItemId: "beet-salad",
      }),
    );
    const withRemovedItem = reducePosState(
      withAddedItem,
      action({
        type: "remove-order-item",
        guestId: "guest-noah",
        menuItemId: "beet-salad",
      }),
    );
    const withEditedNotes = reducePosState(
      withRemovedItem,
      action({
        type: "update-order-notes",
        guestId: "guest-noah",
        notes: "Changed",
      }),
    );

    expect(withEditedNotes.orders[0]).toMatchObject({
      status: "sent",
      guestNotes: "Original note",
      lines: [{ menuItemId: "beet-salad", quantity: 1 }],
    });
  });
});
