import { beforeEach, describe, expect, it } from "vitest";
import { createServerApp } from "./app";
import { store } from "./store";

describe("API server routes & auth", () => {
  let app: ReturnType<typeof createServerApp>;

  beforeEach(() => {
    store.reset();
    app = createServerApp();
  });

  it("returns health status without authentication", async () => {
    const res = await app.request("/health");
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body).toMatchObject({ status: "healthy" });
  });

  it("mints demo session tokens via POST /v1/session", async () => {
    const res = await app.request("/v1/session", { method: "POST" });
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.token).toBeDefined();
    expect(body.expiresAt).toBeDefined();
  });

  it("serves static restaurant catalog via GET /v1/catalog", async () => {
    const res = await app.request("/v1/catalog");
    expect(res.status).toBe(200);
    const body = await res.json();
    expect(body.restaurant).toBeDefined();
    expect(body.menuItems.length).toBeGreaterThan(0);
    expect(body.ingredients.length).toBeGreaterThan(0);
  });

  it("blocks state access without valid Bearer token", async () => {
    const noAuth = await app.request("/v1/state");
    expect(noAuth.status).toBe(401);

    const badAuth = await app.request("/v1/state", {
      headers: { Authorization: "Bearer invalid-token" },
    });
    expect(badAuth.status).toBe(401);
  });

  it("allows state retrieval and mutations with a valid session token", async () => {
    const sessionRes = await app.request("/v1/session", { method: "POST" });
    const { token } = await sessionRes.json();
    const headers = {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
    };

    const stateRes = await app.request("/v1/state", { headers });
    expect(stateRes.status).toBe(200);
    const state = await stateRes.json();
    expect(state.guests.length).toBeGreaterThan(0);

    // Check-in guest
    const checkInRes = await app.request("/v1/guests/guest-jordan/check-in", {
      method: "POST",
      headers,
    });
    expect(checkInRes.status).toBe(200);
    const afterCheckIn = await checkInRes.json();
    expect(afterCheckIn.guests.find((g: { id: string }) => g.id === "guest-jordan").status).toBe("waiting");

    // Recommendations
    const recsRes = await app.request("/v1/guests/guest-maya/recommendations", { headers });
    expect(recsRes.status).toBe(200);
    const recs = await recsRes.json();
    expect(recs.tables.length).toBeGreaterThan(0);
    expect(recs.dishes.length).toBeGreaterThan(0);

    // Seating validation
    const seatRes = await app.request("/v1/guests/guest-maya/seat", {
      method: "POST",
      headers,
      body: JSON.stringify({ tableId: "t2" }),
    });
    expect(seatRes.status).toBe(200);
    const afterSeat = await seatRes.json();
    expect(afterSeat.tables.find((t: { id: string }) => t.id === "t2").seatedGuestId).toBe("guest-maya");

    // Add order item
    const addItemRes = await app.request("/v1/orders/guest-maya/items", {
      method: "POST",
      headers,
      body: JSON.stringify({ menuItemId: "beet-salad" }),
    });
    expect(addItemRes.status).toBe(200);

    // Send order
    const sendOrderRes = await app.request("/v1/orders/guest-maya/send", {
      method: "POST",
      headers,
    });
    expect(sendOrderRes.status).toBe(200);
    const afterSend = await sendOrderRes.json();
    expect(afterSend.orders.find((o: { guestId: string }) => o.guestId === "guest-maya").status).toBe("sent");

    // Reset
    const resetRes = await app.request("/v1/demo/reset", {
      method: "POST",
      headers,
    });
    expect(resetRes.status).toBe(200);
  });
});
