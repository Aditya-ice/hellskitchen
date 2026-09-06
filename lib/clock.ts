"use client";

import { useSyncExternalStore } from "react";

/**
 * The service date, for the POS header.
 *
 * The bundle is statically exported, so anything derived from "now" is baked in
 * at build time and would be wrong by the time anyone opened it — the header
 * used to read a hardcoded "Sunday dinner · August 9" for exactly that reason.
 * `useSyncExternalStore` lets the client render today's date without a
 * hydration mismatch: the prerendered HTML carries no date at all.
 */

export function todayLabel(date: Date = new Date()): string {
  return date.toLocaleDateString(undefined, {
    weekday: "long",
    month: "long",
    day: "numeric",
  });
}

const noopSubscribe = () => () => {};
const getSnapshot = () => todayLabel();
// Empty on the server, so the static HTML commits to nothing.
const getServerSnapshot = () => "";

export function useTodayLabel(): string {
  return useSyncExternalStore(noopSubscribe, getSnapshot, getServerSnapshot);
}
