import { tavily } from "@tavily/core";
import { fallbackDishContext } from "@/data/demo";

export async function POST(request: Request) {
  const body = (await request.json()) as { dish?: string; query?: string };
  const query = body.query?.trim() || body.dish?.trim();

  if (!query) {
    return Response.json({ error: "A dish or ingredient question is required." }, { status: 400 });
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
      `Current culinary background, seasonality, and guest-friendly description for: ${query}. Do not provide medical or allergy safety claims.`,
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
