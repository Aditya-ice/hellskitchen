"use client";

import { ChefHat, Wifi, WifiOff } from "lucide-react";

import { usePos } from "@/components/pos-provider";

/**
 * The bar above the floor.
 *
 * It used to carry a green "Dinner live" badge that was a literal — not
 * conditional on connectivity, the service, or anything else — sitting a few
 * pixels above the header's real "Offline" pill. On a dropped connection the
 * screen said both at once. It also carried an "Open POS" link that rendered on
 * the POS and navigated to the page you were already on.
 *
 * What is left is the one thing a bar like this is worth having: whether this
 * terminal is actually connected to the floor.
 */
export function SiteHeader() {
  const pos = usePos();

  return (
    <header className="border-b border-line bg-white/85 backdrop-blur">
      <div className="mx-auto flex h-16 max-w-[1440px] items-center justify-between px-4 sm:px-6">
        <span className="flex items-center gap-3">
          <span className="grid size-9 place-items-center rounded-xl bg-navy text-white">
            <ChefHat className="size-5" aria-hidden="true" />
          </span>
          <span>
            <span className="block text-sm font-black tracking-tight">
              {pos.restaurant.name || "Ember POS"}
            </span>
            <span className="block text-[10px] font-bold uppercase tracking-[0.16em] text-ink-muted">
              {pos.restaurant.serviceLabel || "Front of house"}
            </span>
          </span>
        </span>

        {/* Announced, because losing the floor mid-service is exactly the kind
            of thing a screen reader user must not have to notice visually. */}
        <p
          role="status"
          aria-live="polite"
          className={`flex items-center gap-1.5 text-xs font-bold ${
            pos.connected ? "text-success" : "text-[#8a5b06]"
          }`}
        >
          {pos.connected ? (
            <>
              <Wifi className="size-3.5" aria-hidden="true" /> Connected
            </>
          ) : (
            <>
              <WifiOff className="size-3.5" aria-hidden="true" /> Reconnecting…
            </>
          )}
        </p>
      </div>
    </header>
  );
}
