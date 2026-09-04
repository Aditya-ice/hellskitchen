"use client";

import { useEffect } from "react";
import { RotateCcw, TriangleAlert } from "lucide-react";

/**
 * Last line of defence for a render that threw.
 *
 * Without this the app has no error boundary at all, so any unexpected shape in
 * server data takes the whole terminal to a white screen mid-service with
 * nothing to act on. This at least names the problem and offers a way back.
 */
export default function PosError({
  error,
  reset,
}: {
  error: Error & { digest?: string };
  reset: () => void;
}) {
  useEffect(() => {
    // Kept: a field failure with no console trail is a failure nobody can fix.
    console.error("Ember POS render error", error);
  }, [error]);

  return (
    <main className="grid min-h-screen place-items-center p-6">
      <div className="card max-w-md p-6 text-center">
        <TriangleAlert className="mx-auto size-8 text-critical" aria-hidden="true" />
        <h1 className="mt-4 text-lg font-black">This screen stopped responding</h1>
        <p className="mt-2 text-sm text-ink-muted">
          The service itself is unaffected — the floor, the open tickets and the
          stock all live on the server. Reloading this screen picks them back up.
        </p>
        {error.digest ? (
          <p className="mt-3 font-mono text-[11px] text-ink-muted">
            Reference: {error.digest}
          </p>
        ) : null}
        <button
          type="button"
          onClick={reset}
          className="mt-5 inline-flex items-center gap-2 rounded-full bg-navy px-4 py-2.5 text-xs font-black text-white"
        >
          <RotateCcw className="size-3.5" aria-hidden="true" /> Reload this screen
        </button>
      </div>
    </main>
  );
}
