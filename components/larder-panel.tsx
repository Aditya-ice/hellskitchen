"use client";

import { Clock3, PackagePlus, TriangleAlert } from "lucide-react";
import { usePos } from "@/components/pos-provider";
import type { Ingredient } from "@/lib/domain";

/**
 * Live stock, and the one control that puts it back.
 *
 * Only shows what needs attention. A full larder is not information a server
 * needs mid-service, so when nothing is low this collapses to a single line.
 */

/** Matches the thresholds the engine scores against, in `engine.rs`. */
export function stockLevel(ingredient: Ingredient): "out" | "low" | "ok" {
  if (ingredient.onHand <= 0) return "out";
  if (ingredient.par > 0 && ingredient.onHand / ingredient.par <= 0.25) return "low";
  return "ok";
}

/** Amount needed to bring an ingredient back up to par. */
export function topUpQuantity(ingredient: Ingredient): number {
  return Math.max(ingredient.par - ingredient.onHand, 1);
}

export function LarderPanel() {
  const pos = usePos();

  // Only shown when the forecaster has enough evidence to be worth acting on.
  // A projection built from one ticket would be a guess wearing a number.
  const risks =
    pos.forecast.available && pos.forecast.actionable
      ? (pos.forecast.stockoutRisks ?? [])
      : [];
  const riskFor = (ingredientId: string) =>
    risks.find((risk) => risk.ingredientId === ingredientId);

  const needsAttention = pos.ingredients
    .map((ingredient) => ({ ingredient, level: stockLevel(ingredient) }))
    .filter(({ level }) => level !== "ok")
    // Out before low, so the thing blocking a sale is at the top.
    .sort((a, b) => {
      if (a.level !== b.level) return a.level === "out" ? -1 : 1;
      return a.ingredient.name.localeCompare(b.ingredient.name);
    });

  return (
    <div className="border-t border-line p-4">
      <div className="flex items-center justify-between gap-2">
        <p className="text-xs font-black">Larder</p>
        {needsAttention.length > 0 && (
          <span className="rounded-full bg-warning/12 px-2 py-0.5 text-[10px] font-black text-[#8a5b06]">
            {needsAttention.length} low
          </span>
        )}
      </div>

      {risks.length > 0 && (
        <ul className="mt-2 space-y-2">
          {risks
            .filter((risk) => !needsAttention.some((item) => item.ingredient.id === risk.ingredientId))
            .map((risk) => (
              <li
                key={risk.ingredientId}
                className="rounded-xl border border-warning/40 bg-warning/8 p-3"
              >
                <p className="flex items-center gap-1.5 text-xs font-black text-[#8a5b06]">
                  <Clock3 className="size-3" />
                  {risk.name} runs out in ~{risk.minutesToZero}m
                </p>
                {risk.blocks.length > 0 && (
                  <p className="mt-1 text-[10px] leading-4 text-ink-muted">
                    Takes {risk.blocks.join(", ")} off the menu
                  </p>
                )}
              </li>
            ))}
        </ul>
      )}

      {needsAttention.length === 0 && risks.length === 0 ? (
        <p className="mt-2 text-[11px] leading-4 text-ink-muted">
          Everything is stocked.
        </p>
      ) : needsAttention.length === 0 ? null : (
        <ul className="mt-2 space-y-2">
          {needsAttention.map(({ ingredient, level }) => (
            <li
              key={ingredient.id}
              className={`rounded-xl border p-3 ${
                level === "out"
                  ? "border-critical/20 bg-critical/5"
                  : "border-line bg-white"
              }`}
            >
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0">
                  <p className="truncate text-xs font-black">{ingredient.name}</p>
                  <p
                    className={`mt-0.5 text-[10px] font-bold ${
                      level === "out" ? "text-critical" : "text-ink-muted"
                    }`}
                  >
                    {level === "out" ? (
                      <span className="inline-flex items-center gap-1">
                        <TriangleAlert className="size-3" /> Out of stock
                      </span>
                    ) : (
                      `${ingredient.onHand} of ${ingredient.par} ${ingredient.unit}`
                    )}
                  </p>
                  {riskFor(ingredient.id)?.minutesToZero != null && level !== "out" && (
                    <p className="mt-0.5 text-[10px] font-bold text-[#8a5b06]">
                      Out in ~{riskFor(ingredient.id)!.minutesToZero}m at this rate
                    </p>
                  )}
                </div>
                <button
                  type="button"
                  onClick={() =>
                    pos.restockIngredient(ingredient.id, topUpQuantity(ingredient))
                  }
                  title={`Book in ${topUpQuantity(ingredient)} ${ingredient.unit}`}
                  className="flex shrink-0 items-center gap-1 rounded-full border border-line bg-white px-2.5 py-1.5 text-[10px] font-black hover:border-accent hover:text-accent"
                >
                  <PackagePlus className="size-3" />
                  Restock
                </button>
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
