"use client";

/**
 * Transport layer between the UI and `ember-server`.
 *
 * All POS state now lives in the Rust server: the browser reads a snapshot,
 * posts actions, and subscribes to changes over SSE. Nothing here reduces or
 * scores anything — that is the server's job, so every surface (this app, the
 * desktop shell, a phone on the LAN) sees exactly the same floor.
 */

import type { Action } from "@/lib/generated/Action";
import type { GuestProfile } from "@/lib/generated/GuestProfile";
import type { MenuItem } from "@/lib/generated/MenuItem";
import type { PosState } from "@/lib/generated/PosState";
import type { Recommendation } from "@/lib/generated/Recommendation";
import type { Restaurant } from "@/lib/generated/Restaurant";
import type { StaffMember } from "@/lib/generated/StaffMember";

declare global {
  interface Window {
    /** Injected by the desktop shell, which serves on an ephemeral port. */
    __EMBER_API_BASE__?: string;
  }
}

/**
 * Empty means same-origin, which is the case both for `npm run dev` (proxied)
 * and for the server serving the exported bundle itself.
 */
export function apiBase(): string {
  const runtime =
    typeof window !== "undefined" ? window.__EMBER_API_BASE__ : undefined;
  const configured = runtime ?? process.env.NEXT_PUBLIC_EMBER_API ?? "";
  return configured.replace(/\/+$/, "");
}

export function apiUrl(path: string): string {
  return `${apiBase()}${path.startsWith("/") ? path : `/${path}`}`;
}

export interface Revision {
  version: number;
  state: PosState;
}

/** Static reference data. Live stock arrives with the state, not here. */
export interface MenuPayload {
  restaurant: Restaurant;
  menuItems: MenuItem[];
  staff: StaffMember[];
}

export interface RecommendationPayload {
  guestId: string;
  version: number;
  tables: Recommendation[];
  dishes: Recommendation[];
  estimateWait: number;
  orderTotal: number;
}

/** Floor-wide numbers for the header. The average wait needs the engine. */
export interface FloorSummary {
  version: number;
  waitingGuests: number;
  openTables: number;
  averageWaitMinutes: number;
}

export interface ActionOutcome extends Revision {
  outcome: "changed" | "rejected" | "duplicate";
}

async function readJson<T>(response: Response, what: string): Promise<T> {
  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as {
      error?: string;
    } | null;
    throw new Error(body?.error ?? `Could not ${what} (HTTP ${response.status}).`);
  }
  return (await response.json()) as T;
}

export async function fetchState(signal?: AbortSignal): Promise<Revision> {
  const response = await fetch(apiUrl("/api/state"), {
    credentials: "include",
    signal,
  });
  return readJson<Revision>(response, "load the current service");
}

export async function fetchMenu(signal?: AbortSignal): Promise<MenuPayload> {
  const response = await fetch(apiUrl("/api/menu"), {
    credentials: "include",
    signal,
  });
  return readJson<MenuPayload>(response, "load the menu");
}

export async function fetchRecommendations(
  guestId: string,
  signal?: AbortSignal,
): Promise<RecommendationPayload> {
  const response = await fetch(
    apiUrl(`/api/recommendations/${encodeURIComponent(guestId)}`),
    { credentials: "include", signal },
  );
  return readJson<RecommendationPayload>(response, "score this guest");
}

export async function fetchSummary(signal?: AbortSignal): Promise<FloorSummary> {
  const response = await fetch(apiUrl("/api/summary"), {
    credentials: "include",
    signal,
  });
  return readJson<FloorSummary>(response, "read the floor summary");
}

export async function postAction(action: Action): Promise<ActionOutcome> {
  const response = await fetch(apiUrl("/api/actions"), {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(action),
  });
  return readJson<ActionOutcome>(response, "record that change");
}

/**
 * Subscribes to state changes. The server replays the current revision on
 * connect, so a caller does not need to fetch first. Returns an unsubscribe.
 */
export function subscribeToState(handlers: {
  onRevision: (revision: Revision) => void;
  onConnectedChange?: (connected: boolean) => void;
}): () => void {
  if (typeof EventSource === "undefined") return () => {};

  const source = new EventSource(apiUrl("/api/stream"), {
    withCredentials: true,
  });

  source.addEventListener("state", (event) => {
    try {
      handlers.onRevision(JSON.parse((event as MessageEvent<string>).data));
      handlers.onConnectedChange?.(true);
    } catch {
      // A malformed frame is not worth tearing the stream down for; the next
      // revision supersedes it anyway.
    }
  });
  source.onopen = () => handlers.onConnectedChange?.(true);
  // EventSource reconnects on its own; this only reflects the current state.
  source.onerror = () => handlers.onConnectedChange?.(false);

  return () => source.close();
}

/**
 * `Omit` does not distribute over a union: `Omit<Action, "id" | "at">` collapses
 * to the properties every member shares, which is just `type`. Distributing it
 * first keeps each variant's own fields, so a malformed action is a type error.
 */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown
  ? Omit<T, K>
  : never;

export type ActionInput = DistributiveOmit<Action, "id" | "at">;

/** Builds an action with the identity the server dedupes on. */
export function newAction(
  kind: ActionInput,
  id: string = crypto.randomUUID(),
): Action {
  return { ...kind, id, at: new Date().toISOString() } as Action;
}

export function newWalkIn(name: string, partySize: number): GuestProfile {
  return {
    id: `guest-${crypto.randomUUID()}`,
    name,
    partySize,
    reservationTime: null,
    arrivalTime: new Date().toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
    }),
    status: "waiting",
    allergies: [],
    dietaryNeeds: [],
    likes: [],
    dislikes: [],
    seatingPreferences: [],
    visitCount: 0,
    lastVisit: null,
    notes: "Walk-in guest",
  };
}

// --- demo session ---------------------------------------------------------

let sessionPromise: Promise<void> | null = null;

/**
 * Sponsor routes require a session cookie. Requested once per page load and
 * retried on failure.
 */
export function ensureDemoSession(): Promise<void> {
  if (!sessionPromise) {
    sessionPromise = fetch(apiUrl("/api/demo-session"), {
      method: "POST",
      credentials: "include",
    }).then(async (response) => {
      if (!response.ok) {
        const body = (await response.json().catch(() => null)) as {
          error?: string;
        } | null;
        throw new Error(body?.error ?? "Unable to start the demo session.");
      }
    });
    sessionPromise.catch(() => {
      sessionPromise = null;
    });
  }
  return sessionPromise;
}
