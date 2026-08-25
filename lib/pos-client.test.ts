import { afterEach, describe, expect, it, vi } from "vitest";
import { apiBase, apiUrl, newAction, newWalkIn } from "@/lib/pos-client";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.unstubAllEnvs();
});

describe("api base resolution", () => {
  it("defaults to same-origin so the exported bundle needs no configuration", () => {
    expect(apiBase()).toBe("");
    expect(apiUrl("/api/state")).toBe("/api/state");
  });

  it("prefers the runtime override the desktop shell injects", () => {
    // Tauri serves on an ephemeral port, so this cannot be baked in at build
    // time the way a NEXT_PUBLIC_ env var is.
    vi.stubGlobal("window", { __EMBER_API_BASE__: "http://127.0.0.1:51234" });
    expect(apiUrl("/api/state")).toBe("http://127.0.0.1:51234/api/state");
  });

  it("falls back to the build-time env var", () => {
    vi.stubGlobal("window", {});
    vi.stubEnv("NEXT_PUBLIC_EMBER_API", "http://localhost:4000");
    expect(apiUrl("/api/state")).toBe("http://localhost:4000/api/state");
  });

  it("does not produce a double slash", () => {
    vi.stubGlobal("window", { __EMBER_API_BASE__: "http://127.0.0.1:4000/" });
    expect(apiUrl("/api/state")).toBe("http://127.0.0.1:4000/api/state");
    expect(apiUrl("api/state")).toBe("http://127.0.0.1:4000/api/state");
  });
});

describe("action construction", () => {
  it("stamps an id and timestamp the server can dedupe on", () => {
    const action = newAction({ type: "send-order", guestId: "guest-maya" });

    expect(action).toMatchObject({ type: "send-order", guestId: "guest-maya" });
    expect(action.id).toMatch(/^[0-9a-f-]{36}$/);
    expect(Number.isNaN(Date.parse(action.at))).toBe(false);
  });

  it("gives every action a distinct id", () => {
    const first = newAction({ type: "reset" });
    const second = newAction({ type: "reset" });

    // Ids are the server's replay guard: a shared id would make the second
    // action a silent no-op.
    expect(first.id).not.toBe(second.id);
  });

  it("accepts a caller-supplied id for retries", () => {
    const action = newAction({ type: "reset" }, "fixed-id");
    expect(action.id).toBe("fixed-id");
  });
});

describe("walk-in guests", () => {
  it("arrives already waiting, with no dietary assumptions", () => {
    const guest = newWalkIn("Sam Reed", 3);

    expect(guest.name).toBe("Sam Reed");
    expect(guest.partySize).toBe(3);
    expect(guest.status).toBe("waiting");
    expect(guest.arrivalTime).not.toBeNull();
    expect(guest.reservationTime).toBeNull();

    // Nothing may be invented about a guest nobody has spoken to yet — an
    // assumed allergy or preference here would flow straight into scoring.
    expect(guest.allergies).toEqual([]);
    expect(guest.dietaryNeeds).toEqual([]);
    expect(guest.likes).toEqual([]);
    expect(guest.dislikes).toEqual([]);
    expect(guest.seatingPreferences).toEqual([]);
  });

  it("gives each walk-in a unique id", () => {
    expect(newWalkIn("A", 1).id).not.toBe(newWalkIn("A", 1).id);
  });
});
