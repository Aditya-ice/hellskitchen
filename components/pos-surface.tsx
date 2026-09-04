"use client";

import { KitchenDisplay } from "@/components/kitchen-display";
import { PosNotice } from "@/components/pos-notice";
import { PosShell } from "@/components/pos-shell";
import { SiteHeader } from "@/components/site-header";
import { useSurface } from "@/lib/desktop";

/**
 * Picks which surface this window shows.
 *
 * The desktop app opens its Kitchen window at `/pos?view=kitchen`; everything
 * else gets the front-of-house workspace. Both read the same live state, so
 * this is purely a choice of view.
 *
 * `PosNotice` sits on both: the pass bumps tickets, and a refused bump needs
 * saying just as much as a refused seating does.
 */
export function PosSurface() {
  if (useSurface() === "kitchen") {
    return (
      <>
        <KitchenDisplay />
        <PosNotice />
      </>
    );
  }
  return (
    <>
      <SiteHeader active="pos" />
      <PosShell />
      <PosNotice />
    </>
  );
}
