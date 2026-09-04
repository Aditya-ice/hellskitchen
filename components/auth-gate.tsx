"use client";

import { useCallback } from "react";

import { SignIn } from "@/components/sign-in";
import { usePos } from "@/components/pos-provider";

/**
 * Keeps the floor behind a sign-in.
 *
 * Two ways in: arriving without a session at all, and having one expire
 * mid-service. The second is the one worth designing for — a terminal left on
 * the pass idles out, and whoever picks it up should get a keypad, not a screen
 * full of stale tables that refuses every tap.
 *
 * The provider owns the answer, because it is the thing actually talking to the
 * server and so learns about a dead session first.
 */
export function AuthGate({ children }: { children: React.ReactNode }) {
  const pos = usePos();
  const { reload } = pos;

  const onSignedIn = useCallback(() => reload(), [reload]);

  // `null` means "not known yet". Showing the floor is right there: it is empty
  // until the first revision anyway, and flashing a keypad at a signed-in
  // terminal on every reload would be worse.
  if (pos.authenticated === false) {
    return (
      <SignIn
        onSignedIn={onSignedIn}
        reason={
          pos.sessionEnded
            ? "This terminal was signed out after being idle. Sign in to carry on."
            : undefined
        }
      />
    );
  }

  return <>{children}</>;
}
