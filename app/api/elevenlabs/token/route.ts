import { ElevenLabsClient } from "@elevenlabs/elevenlabs-js";
import {
  enforceRateLimit,
  requireDemoSession,
  requireSameOrigin,
} from "@/lib/api-guard";

export const dynamic = "force-dynamic";

export async function GET(request: Request) {
  const originError = requireSameOrigin(request);
  if (originError) return originError;
  const sessionError = requireDemoSession(request);
  if (sessionError) return sessionError;
  const rateLimitError = enforceRateLimit(request, "elevenlabs-token", 6, 60_000);
  if (rateLimitError) return rateLimitError;

  const apiKey = process.env.ELEVENLABS_API_KEY;
  if (!apiKey) {
    return Response.json(
      {
        error: "ElevenLabs is not configured. Use the typed demo input instead.",
        configured: false,
      },
      { status: 503 },
    );
  }

  try {
    const client = new ElevenLabsClient({ apiKey });
    const response = await client.tokens.singleUse.create("realtime_scribe");
    return Response.json({ token: response.token, configured: true });
  } catch (error) {
    console.error("Unable to create ElevenLabs token", error);
    return Response.json(
      { error: "Voice transcription is temporarily unavailable." },
      { status: 502 },
    );
  }
}
