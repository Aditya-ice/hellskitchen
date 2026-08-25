import { describe, expect, it } from "vitest";
import { demoGuests, demoOrders, demoTables } from "./demo";
import {
  canSeatGuestAtTable,
  createInitialPosState,
  estimateWait,
  orderTotal,
  recommendDishes,
  recommendTables,
  reducePosState,
} from "./decision-engine";
import type { SharedAction, SharedActionInput } from "./domain";

const action = (value: SharedActionInput): SharedAction =>
  ({
    ...value,
    id: "test-action",
    at: "2026-08-13T10:00:00.000Z",
  }) as SharedAction;

describe("table recommendations", () => {
  it("ranks the exact-fit accessible window table first for Maya", () => {
    const maya = demoGuests.find((guest) => guest.id === "guest-maya")!;
    const recommendations = recommendTables(maya, demoTables);

    expect(recommendations[0]).toMatchObject({
      id: "t2",
      eligible: true,
      score: 100,
    });
    expect(recommendations[0].reasons).toContain("Matches window preference");
  });

  it("enforces capacity and accessibility as hard constraints", () => {
    const priya = demoGuests.find((guest) => guest.id === "guest-priya")!;
    const recommendations = recommendTables(priya, demoTables);

    expect(recommendations.find((item) => item.id === "t5")?.eligible).toBe(false);
    expect(recommendations.find((item) => item.id === "t1")?.eligible).toBe(false);
  });

  it("returns no wait when the top table is already available", () => {
    const maya = demoGuests.find((guest) => guest.id === "guest-maya")!;
    expect(estimateWait(maya, demoTables)).toBe(0);
  });

  it("only allows a checked-in guest at an available compatible table", () => {
    const maya = demoGuests.find((guest) => guest.id === "guest-maya")!;
    const jordan = demoGuests.find((guest) => guest.id === "guest-jordan")!;
    const accessibleTable = demoTables.find((table) => table.id === "t2")!;
    const occupiedTable = demoTables.find((table) => table.id === "t3")!;

    expect(canSeatGuestAtTable(maya, accessibleTable)).toBe(true);
    expect(canSeatGuestAtTable(jordan, accessibleTable)).toBe(false);
    expect(canSeatGuestAtTable(maya, occupiedTable)).toBe(false);
  });
});

describe("dish recommendations", () => {
  it("blocks explicit allergens and unmet dietary requirements", () => {
    const maya = demoGuests.find((guest) => guest.id === "guest-maya")!;
    const recommendations = recommendDishes(maya);

    const tartare = recommendations.find((item) => item.id === "carrot-tartare")!;
    const farro = recommendations.find((item) => item.id === "mushroom-farro")!;

    expect(tartare.eligible).toBe(false);
    expect(tartare.warnings).toContain("Contains guest allergen: tree nuts");
    expect(farro.eligible).toBe(false);
    expect(farro.warnings).toContain("Does not meet gluten-free");
  });

  it("keeps only vegan-compatible dishes eligible for a vegan guest", () => {
    const jordan = demoGuests.find((guest) => guest.id === "guest-jordan")!;
    const recommendations = recommendDishes(jordan);

    expect(recommendations.find((item) => item.id === "cauliflower")?.eligible).toBe(true);
    expect(recommendations.find((item) => item.id === "herb-chicken")?.eligible).toBe(false);
  });
});

describe("orders & totals", () => {
  it("calculates totals from quantity and menu price", () => {
    const order = {
      ...demoOrders[0],
      lines: [
        { menuItemId: "beet-salad", quantity: 2, notes: "" },
        { menuItemId: "chocolate-torte", quantity: 1, notes: "" },
      ],
    };

    expect(orderTotal(order)).toBe(48);
  });
});

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
