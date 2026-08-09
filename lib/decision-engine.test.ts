import { describe, expect, it } from "vitest";
import { demoGuests, demoOrders, demoTables } from "@/data/demo";
import {
  estimateWait,
  orderTotal,
  recommendDishes,
  recommendTables,
} from "@/lib/decision-engine";

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

describe("orders", () => {
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
