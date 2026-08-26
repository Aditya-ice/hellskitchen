"use client";

import { KitchenDisplay } from "@/components/kitchen-display";
import { PosShell } from "@/components/pos-shell";
import { SiteHeader } from "@/components/site-header";
import { useSurface } from "@/lib/desktop";

/**
 * Picks which surface this window shows.
 *
 * The desktop app opens its Kitchen window at `/pos?view=kitchen`; everything
 * else gets the front-of-house workspace. Both read the same live state, so
 * this is purely a choice of view.
 */
export function PosSurface() {
  if (useSurface() === "kitchen") {
    return <KitchenDisplay />;
  }
  return (
    <>
      <SiteHeader active="pos" />
      <PosShell />
    </>
  );
}
