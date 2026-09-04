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
import type { Rejection } from "@/lib/generated/Rejection";
import type { Restaurant } from "@/lib/generated/Restaurant";
import type { StaffMember } from "@/lib/generated/StaffMember";
import type { StaffRole } from "@/lib/generated/StaffRole";

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
  /** Which ranking this actually is — the brain may be absent or unhelpful. */
  rankedBy: "engine" | "model";
}

export interface StockoutRisk {
  ingredientId: string;
  name: string;
  onHand: number;
  unit: string;
  burnPerHour: number;
  minutesToZero: number | null;
  /** Dishes that come off the menu when this runs out. */
  blocks: string[];
}

export interface Forecast {
  available: boolean;
  confidence?: "none" | "low" | "fair";
  confidenceReason?: string;
  actionable?: boolean;
  stockoutRisks?: StockoutRisk[];
}

/** Never rejects on an absent brain: an unavailable forecast is a valid answer. */
export async function fetchForecast(signal?: AbortSignal): Promise<Forecast> {
  const response = await fetch(apiUrl("/api/forecast"), {
    credentials: "include",
    signal,
  });
  if (!response.ok) return { available: false };
  return (await response.json()) as Forecast;
}

/** Floor-wide numbers for the header. The average wait needs the engine. */
export interface FloorSummary {
  version: number;
  waitingGuests: number;
  openTables: number;
  averageWaitMinutes: number;
}

export interface ActionOutcome extends Revision {
  /**
   * `changed` moved the floor. `unchanged` was allowed but altered nothing.
   * `duplicate` was already applied — a retry, not a second seating.
   * `rejected` was refused by a guard, and `reason` says which.
   */
  outcome: "changed" | "unchanged" | "rejected" | "duplicate";
  /** Set only when `outcome` is `rejected`. Switch on this, not the message. */
  reason?: Rejection;
  /** Server-supplied fallback text, for a reason this client has no copy for. */
  reasonMessage?: string;
}

async function readJson<T>(response: Response, what: string): Promise<T> {
  if (response.status === 401) throw new NotAuthenticatedError();
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

export interface AgentAnswer {
  answer: string;
  toolsUsed: string[];
  model: string | null;
  /** False when the service is absent, or running without model credentials. */
  configured: boolean;
}

/**
 * Asks the optional floor agent a question.
 *
 * Always resolves: `ember-server` reports a missing or failing agent as an
 * answer rather than an error, because the POS does not depend on it.
 */
export async function askFloorAgent(
  question: string,
  signal?: AbortSignal,
): Promise<AgentAnswer> {
  const response = await fetch(apiUrl("/api/agent/ask"), {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ question }),
    signal,
  });
  return readJson<AgentAnswer>(response, "ask the floor agent");
}

export async function postAction(
  action: ActionInput & ActionRequest,
): Promise<ActionOutcome> {
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
 * A random id that also works over plain http.
 *
 * `crypto.randomUUID` is only available in a secure context, so on a phone
 * reaching the POS over `http://192.168.x.x` -- the surface this module's
 * header advertises -- it throws on every single action. `getRandomValues` has
 * no such restriction.
 */
function randomId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    try {
      return crypto.randomUUID();
    } catch {
      // Falls through to the bytes below.
    }
  }
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

/**
 * `Omit` does not distribute over a union: `Omit<Action, "id" | "at">` collapses
 * to the properties every member shares, which is just `type`. Distributing it
 * first keeps each variant's own fields, so a malformed action is a type error.
 */
type DistributiveOmit<T, K extends PropertyKey> = T extends unknown
  ? Omit<T, K>
  : never;

export type ActionInput = DistributiveOmit<Action, "id" | "at" | "actor">;

/**
 * What a client sends: the action, plus the id the server dedupes on.
 *
 * No `at` and no `actor`. The server stamps both — the time from its own clock,
 * the actor from the session — so neither is ours to claim.
 */
export interface ActionRequest {
  id: string;
}

/** Builds an action request with the identity the server dedupes on. */
export function newAction(
  kind: ActionInput,
  id: string = randomId(),
): ActionInput & ActionRequest {
  return { ...kind, id };
}

export function newWalkIn(name: string, partySize: number): GuestProfile {
  return {
    id: `guest-${randomId()}`,
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

// --- identity -------------------------------------------------------------

export interface Identity {
  staffId: string;
  name: string;
  role: StaffRole;
  terminalId: string;
}

export interface AuthState {
  authenticated: boolean;
  identity?: Identity;
  /** True on a terminal where nobody has a PIN yet, so there is nothing to sign in to. */
  needsSetup?: boolean;
}

/**
 * Thrown when the server says this terminal is not signed in.
 *
 * A distinct type so the provider can drop straight to the sign-in screen
 * rather than showing "something went wrong" for what is really just a session
 * that timed out between tables.
 */
export class NotAuthenticatedError extends Error {
  constructor() {
    super("This terminal is not signed in.");
    this.name = "NotAuthenticatedError";
  }
}

export async function fetchIdentity(signal?: AbortSignal): Promise<AuthState> {
  const response = await fetch(apiUrl("/api/auth/me"), {
    credentials: "include",
    signal,
  });
  return readJson<AuthState>(response, "check who is signed in");
}

export async function login(
  staffId: string,
  pin: string,
  terminalId: string,
): Promise<Identity | undefined> {
  const response = await fetch(apiUrl("/api/auth/login"), {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ staffId, pin, terminalId }),
  });
  const body = await readJson<{ identity?: Identity }>(response, "sign in");
  return body.identity;
}

export async function logout(): Promise<void> {
  await fetch(apiUrl("/api/auth/logout"), {
    method: "POST",
    credentials: "include",
  });
}

/** First run only: the server refuses this once any PIN exists. */
export async function setupFirstManager(
  staffId: string,
  pin: string,
): Promise<void> {
  const response = await fetch(apiUrl("/api/auth/setup"), {
    method: "POST",
    credentials: "include",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ staffId, pin }),
  });
  await readJson<unknown>(response, "set the first PIN");
}
