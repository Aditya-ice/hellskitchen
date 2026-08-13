import { tavily } from "@tavily/core";
import { fallbackDishContext, menuItems } from "@/data/demo";
import {
  enforceRateLimit,
  requireDemoSession,
  requireSameOrigin,
} from "@/lib/api-guard";

export async function POST(request: Request) {
  const originError = requireSameOrigin(request);
  if (originError) return originError;
  const sessionError = requireDemoSession(request);
  if (sessionError) return sessionError;
  const rateLimitError = enforceRateLimit(request, "tavily-search", 10, 60_000);
  if (rateLimitError) return rateLimitError;

  let body: { dishId?: unknown };
  try {
    body = (await request.json()) as { dishId?: unknown };
  } catch {
    return Response.json({ error: "Invalid JSON body." }, { status: 400 });
  }

  if (typeof body.dishId !== "string" || body.dishId.length > 80) {
    return Response.json({ error: "A valid dish ID is required." }, { status: 400 });
  }

  const dish = menuItems.find((item) => item.id === body.dishId);
  if (!dish) {
    return Response.json({ error: "Dish not found." }, { status: 404 });
  }

  const apiKey = process.env.TAVILY_API_KEY;
  if (!apiKey) {
    return Response.json({
      answer: fallbackDishContext,
      sources: [],
      isFallback: true,
    });
  }

  try {
    const client = tavily({ apiKey });
    const result = await client.search(
      `Current culinary background, seasonality, and guest-friendly description for: ${dish.name}: ${dish.description}. Do not provide medical or allergy safety claims.`,
      {
        searchDepth: "basic",
        maxResults: 3,
        includeAnswer: "basic",
        topic: "general",
      },
    );

    return Response.json({
      answer: result.answer ?? null,
      sources: result.results.map((source) => ({
        title: source.title,
        url: source.url,
        content: source.content,
      })),
      isFallback: false,
    });
  } catch (error) {
    console.error("Tavily search failed", error);
    return Response.json({
      answer: fallbackDishContext,
      sources: [],
      isFallback: true,
    });
  }
}
