"use client";

import { useCallback } from "react";

import { SignIn } from "@/components/sign-in";

/**
 * The terminal's front door.
 *
 * This used to be a marketing page whose hero card presented fabricated data as
 * if it were live: a named guest ("Maya Chen · Party of 4"), a table ("T2") and
 * a "98%" confidence score, none of which came from anywhere, above a
 * decorative "Dinner live" badge on a statically exported page. On a terminal
 * that is about to run a real service, an invented guest and an invented
 * confidence number are worse than no landing page at all.
 */
export default function Home() {
  const enter = useCallback(() => {
    window.location.replace("/pos");
  }, []);

  return <SignIn onSignedIn={enter} />;
}
