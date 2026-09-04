"use client";

import { useEffect } from "react";
import { CircleAlert, TriangleAlert, X } from "lucide-react";

import { usePos } from "@/components/pos-provider";

/** A refusal is read once and cleared; a failure stays until it is dismissed. */
const REFUSAL_TIMEOUT_MS = 7_000;

/**
 * The one place the POS tells you something did not happen.
 *
 * Before this existed, a refused action returned HTTP 200 with an unchanged
 * revision and the client dropped it on the floor: the button clicked, nothing
 * moved, and nobody was told why. During a service that reads as a broken
 * terminal, so people tap again — which is how a party ends up seated twice.
 *
 * Deliberately outside the header's `md:` cluster: a server on a phone is
 * exactly who needs to see this, and that cluster is hidden below 768px.
 */
export function PosNotice() {
  const pos = usePos();
  const { notice, dismissNotice } = pos;

  useEffect(() => {
    if (!notice || notice.kind !== "refused") return;
    const timer = window.setTimeout(dismissNotice, REFUSAL_TIMEOUT_MS);
    return () => window.clearTimeout(timer);
    // notice.id rather than notice: two identical refusals in a row must
    // restart the timer rather than let the first one's expiry clear the second.
  }, [notice?.id, notice?.kind, dismissNotice, notice]);

  const failed = notice?.kind === "failed";

  return (
    <div
      // The live region is always mounted. Screen readers do not reliably
      // announce content that arrives with the region itself.
      aria-live={failed ? "assertive" : "polite"}
      role={failed ? "alert" : "status"}
      className="pointer-events-none fixed inset-x-0 bottom-0 z-50 flex justify-center px-4 pb-[max(1rem,env(safe-area-inset-bottom))]"
    >
      {notice ? (
        <div
          key={notice.id}
          className={`pointer-events-auto flex w-full max-w-lg items-start gap-3 rounded-2xl border px-4 py-3 shadow-lg ${
            failed
              ? "border-critical/30 bg-critical text-white"
              : "border-line bg-navy text-white"
          }`}
        >
          {failed ? (
            <TriangleAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          ) : (
            <CircleAlert className="mt-0.5 size-4 shrink-0" aria-hidden="true" />
          )}
          <div className="min-w-0 flex-1">
            <p className="text-sm font-bold">{notice.message}</p>
            {failed ? (
              <p className="mt-0.5 text-xs text-white/70">
                The floor shown here may be behind until this reconnects.
              </p>
            ) : null}
          </div>
          <button
            type="button"
            onClick={dismissNotice}
            aria-label="Dismiss"
            className="-m-2 grid size-11 shrink-0 place-items-center rounded-full text-white/70 hover:text-white focus-visible:text-white"
          >
            <X className="size-4" aria-hidden="true" />
          </button>
        </div>
      ) : null}
    </div>
  );
}
