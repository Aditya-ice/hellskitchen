import { describe, expect, it } from "vitest";
import type { Ingredient } from "@/lib/domain";
import { stockLevel, topUpQuantity } from "@/components/larder-panel";

function ingredient(onHand: number, par = 18): Ingredient {
  return {
    id: "carrot",
    name: "Carrots",
    aliases: ["carrot"],
    onHand,
    par,
    unit: "lb",
  };
}

describe("stock level", () => {
  it("matches the thresholds the engine scores against", () => {
    // These mirror recommend_dishes in crates/ember-core/src/engine.rs:
    // out at <= 0, low at onHand / par <= 0.25. If the engine's rule moves,
    // this panel would disagree with the menu it sits next to.
    expect(stockLevel(ingredient(0))).toBe("out");
    expect(stockLevel(ingredient(4.5))).toBe("low"); // exactly 0.25 of par
    expect(stockLevel(ingredient(4.6))).toBe("ok");
    expect(stockLevel(ingredient(18))).toBe("ok");
  });

  it("treats negative stock as out rather than low", () => {
    expect(stockLevel(ingredient(-1))).toBe("out");
  });

  it("does not divide by a zero par", () => {
    // An ingredient with no par set must not read as low through a NaN.
    expect(stockLevel(ingredient(5, 0))).toBe("ok");
  });
});

describe("top up quantity", () => {
  it("brings stock back to par", () => {
    expect(topUpQuantity(ingredient(3))).toBe(15);
    expect(topUpQuantity(ingredient(0))).toBe(18);
  });

  it("never asks for a non-positive delivery", () => {
    // The reducer refuses a quantity of zero or less, so a full ingredient
    // must still produce a valid action rather than a silently ignored one.
    expect(topUpQuantity(ingredient(18))).toBeGreaterThan(0);
    expect(topUpQuantity(ingredient(25))).toBeGreaterThan(0);
  });
});
