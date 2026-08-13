import {
  demoSessionCookie,
  enforceRateLimit,
  getDemoSession,
  requireSameOrigin,
} from "@/lib/api-guard";

export async function POST(request: Request) {
  const originError = requireSameOrigin(request);
  if (originError) return originError;

  const existing = getDemoSession(request);
  if (existing) return Response.json({ ready: true });

  const rateLimitError = enforceRateLimit(request, "demo-session", 8, 60 * 60 * 1000);
  if (rateLimitError) return rateLimitError;

  const sessionId = crypto.randomUUID();
  return Response.json(
    { ready: true },
    { headers: { "Set-Cookie": demoSessionCookie(sessionId) } },
  );
}
