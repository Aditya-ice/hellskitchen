import { Hono } from "hono";
import { cors } from "hono/cors";
import { ElevenLabsClient } from "@elevenlabs/elevenlabs-js";
import { tavily } from "@tavily/core";
import {
  fallbackDishContext,
  ingredients,
  menuItems,
  recommendDishes,
  recommendTables,
  restaurant,
  staff,
} from "@hellskitchen/shared";
import {
  enforceRateLimit,
  getClientIp,
  issueSessionToken,
  requireAuth,
} from "./auth.js";
import { store } from "./store.js";

export type Env = {
  Variables: {
    sessionToken: string;
  };
};

export function createServerApp() {
  const app = new Hono<Env>();

  const allowedOrigin = process.env.FRONTEND_ORIGIN || "*";

  app.use(
    "*",
    cors({
      origin: (origin) => {
        if (!origin || allowedOrigin === "*") return origin || "*";
        return origin === allowedOrigin ? origin : null;
      },
      allowMethods: ["GET", "POST", "PATCH", "DELETE", "OPTIONS"],
      allowHeaders: ["Content-Type", "Authorization"],
      exposeHeaders: ["Retry-After"],
      maxAge: 86400,
    }),
  );

  app.get("/health", (c) => {
    return c.json({ status: "healthy", timestamp: new Date().toISOString() });
  });

  app.post("/v1/session", (c) => {
    const ip = getClientIp(c);
    const rateCheck = enforceRateLimit(`session:${ip}`, 10, 60 * 60 * 1000);
    if (!rateCheck.allowed) {
      c.header("Retry-After", String(rateCheck.retryAfter));
      return c.json({ error: "Session creation rate limit exceeded." }, 429);
    }

    const token = issueSessionToken();
    return c.json({ token, expiresAt: new Date(Date.now() + 3600 * 1000).toISOString() });
  });

  app.get("/v1/catalog", (c) => {
    return c.json({
      restaurant,
      ingredients,
      menuItems,
      staff,
    });
  });

  const api = new Hono<Env>();
  api.use("*", requireAuth);

  api.get("/state", (c) => {
    return c.json(store.getState());
  });

  api.post("/demo/reset", (c) => {
    return c.json(store.reset());
  });

  api.post("/guests/:guestId/check-in", (c) => {
    const guestId = c.req.param("guestId");
    const updated = store.dispatch({ type: "check-in", guestId });
    return c.json(updated);
  });

  api.post("/guests/walk-ins", async (c) => {
    let body: { name?: string; partySize?: number };
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON body" }, 400);
    }

    if (!body.name || typeof body.name !== "string" || !body.name.trim()) {
      return c.json({ error: "Guest name is required" }, 400);
    }

    const partySize = Number(body.partySize) || 2;
    const actionId = crypto.randomUUID();
    const at = new Date().toISOString();

    const guest = {
      id: `guest-${actionId}`,
      name: body.name.trim(),
      partySize,
      reservationTime: null,
      arrivalTime: new Date(at).toLocaleTimeString([], {
        hour: "numeric",
        minute: "2-digit",
      }),
      status: "waiting" as const,
      allergies: [],
      dietaryNeeds: [],
      likes: [],
      dislikes: [],
      seatingPreferences: [],
      visitCount: 0,
      lastVisit: null,
      notes: "Walk-in guest",
    };

    const updated = store.dispatch({ type: "add-walk-in", guest });
    return c.json({ guest, state: updated });
  });

  api.patch("/guests/:guestId/notes", async (c) => {
    const guestId = c.req.param("guestId");
    let body: { notes?: string };
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON body" }, 400);
    }

    const updated = store.dispatch({
      type: "update-guest-notes",
      guestId,
      notes: typeof body.notes === "string" ? body.notes : "",
    });
    return c.json(updated);
  });

  api.post("/guests/:guestId/seat", async (c) => {
    const guestId = c.req.param("guestId");
    let body: { tableId?: string };
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON body" }, 400);
    }

    if (!body.tableId || typeof body.tableId !== "string") {
      return c.json({ error: "tableId is required" }, 400);
    }

    const stateBefore = store.getState();
    const updated = store.dispatch({
      type: "seat-guest",
      guestId,
      tableId: body.tableId,
    });

    if (updated === stateBefore) {
      return c.json(
        { error: "Table cannot seat this guest. Check table status, capacity, or accessibility." },
        400,
      );
    }

    return c.json(updated);
  });

  api.get("/guests/:guestId/recommendations", (c) => {
    const guestId = c.req.param("guestId");
    const state = store.getState();
    const guest = state.guests.find((g) => g.id === guestId);

    if (!guest) {
      return c.json({ error: "Guest not found" }, 404);
    }

    const tables = recommendTables(guest, state.tables);
    const dishes = recommendDishes(guest);

    return c.json({
      tables,
      dishes,
    });
  });

  api.post("/orders/:guestId/items", async (c) => {
    const guestId = c.req.param("guestId");
    let body: { menuItemId?: string };
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON body" }, 400);
    }

    if (!body.menuItemId || typeof body.menuItemId !== "string") {
      return c.json({ error: "menuItemId is required" }, 400);
    }

    const updated = store.dispatch({
      type: "add-order-item",
      guestId,
      menuItemId: body.menuItemId,
    });
    return c.json(updated);
  });

  api.delete("/orders/:guestId/items/:menuItemId", (c) => {
    const guestId = c.req.param("guestId");
    const menuItemId = c.req.param("menuItemId");

    const updated = store.dispatch({
      type: "remove-order-item",
      guestId,
      menuItemId,
    });
    return c.json(updated);
  });

  api.patch("/orders/:guestId/notes", async (c) => {
    const guestId = c.req.param("guestId");
    let body: { notes?: string };
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON body" }, 400);
    }

    const updated = store.dispatch({
      type: "update-order-notes",
      guestId,
      notes: typeof body.notes === "string" ? body.notes : "",
    });
    return c.json(updated);
  });

  api.post("/orders/:guestId/send", (c) => {
    const guestId = c.req.param("guestId");
    const stateBefore = store.getState();
    const updated = store.dispatch({
      type: "send-order",
      guestId,
    });

    if (updated === stateBefore) {
      return c.json(
        { error: "Cannot send order. Make sure order is in draft status and contains items." },
        400,
      );
    }

    return c.json(updated);
  });

  api.get("/integrations/elevenlabs/token", async (c) => {
    const ip = getClientIp(c);
    const sessionToken = c.get("sessionToken");
    const rateCheck = enforceRateLimit(`elevenlabs:${ip}:${sessionToken}`, 6, 60_000);

    if (!rateCheck.allowed) {
      c.header("Retry-After", String(rateCheck.retryAfter));
      return c.json({ error: "Too many voice requests. Try again shortly." }, 429);
    }

    const apiKey = process.env.ELEVENLABS_API_KEY;
    if (!apiKey) {
      return c.json(
        {
          error: "ElevenLabs is not configured. Use the typed demo input instead.",
          configured: false,
        },
        503,
      );
    }

    try {
      const client = new ElevenLabsClient({ apiKey });
      const response = await client.tokens.singleUse.create("realtime_scribe");
      return c.json({ token: response.token, configured: true });
    } catch (error) {
      console.error("Unable to create ElevenLabs token", error);
      return c.json(
        { error: "Voice transcription is temporarily unavailable." },
        502,
      );
    }
  });

  api.post("/integrations/tavily/search", async (c) => {
    const ip = getClientIp(c);
    const sessionToken = c.get("sessionToken");
    const rateCheck = enforceRateLimit(`tavily:${ip}:${sessionToken}`, 10, 60_000);

    if (!rateCheck.allowed) {
      c.header("Retry-After", String(rateCheck.retryAfter));
      return c.json({ error: "Too many search requests. Try again shortly." }, 429);
    }

    let body: { dishId?: string };
    try {
      body = await c.req.json();
    } catch {
      return c.json({ error: "Invalid JSON body." }, 400);
    }

    if (!body.dishId || typeof body.dishId !== "string" || body.dishId.length > 80) {
      return c.json({ error: "A valid dish ID is required." }, 400);
    }

    const dish = menuItems.find((item) => item.id === body.dishId);
    if (!dish) {
      return c.json({ error: "Dish not found." }, 404);
    }

    const apiKey = process.env.TAVILY_API_KEY;
    if (!apiKey) {
      return c.json({
        answer: fallbackDishContext,
        sources: [],
        isFallback: true,
      });
    }

    try {
      const client = tavily({ apiKey });
      const query = `Current culinary background, seasonality, and guest-friendly description for ${dish.name}: ${dish.description}. Do not provide medical or allergy safety claims.`;
      const result = await client.search(query, {
        searchDepth: "basic",
        maxResults: 3,
        includeAnswer: "basic",
        topic: "general",
      });

      return c.json({
        answer: result.answer ?? null,
        sources: (result.results ?? []).map((source) => ({
          title: source.title,
          url: source.url,
          content: source.content,
        })),
        isFallback: false,
      });
    } catch (error) {
      console.error("Tavily search failed", error);
      return c.json({
        answer: fallbackDishContext,
        sources: [],
        isFallback: true,
      });
    }
  });

  app.route("/v1", api);

  return app;
}
