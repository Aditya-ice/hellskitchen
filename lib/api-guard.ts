const SESSION_COOKIE = "ember_demo_session";
const SESSION_PATTERN = /^[0-9a-f-]{36}$/i;
const buckets = new Map<string, number[]>();

function cookieValue(request: Request, name: string) {
  const cookie = request.headers.get("cookie") ?? "";
  return cookie
    .split(";")
    .map((part) => part.trim().split("="))
    .find(([key]) => key === name)?.[1];
}

function clientIp(request: Request) {
  return (
    request.headers.get("x-forwarded-for")?.split(",")[0]?.trim() ||
    request.headers.get("x-real-ip") ||
    "local"
  );
}

export function getDemoSession(request: Request) {
  const value = cookieValue(request, SESSION_COOKIE);
  return value && SESSION_PATTERN.test(value) ? value : null;
}

export function requireSameOrigin(request: Request) {
  const fetchSite = request.headers.get("sec-fetch-site");
  if (fetchSite === "cross-site") {
    return Response.json({ error: "Cross-site requests are not allowed." }, { status: 403 });
  }

  const origin = request.headers.get("origin");
  if (origin) {
    try {
      if (new URL(origin).host !== new URL(request.url).host) {
        return Response.json({ error: "Invalid request origin." }, { status: 403 });
      }
    } catch {
      return Response.json({ error: "Invalid request origin." }, { status: 403 });
    }
  }
  return null;
}

export function requireDemoSession(request: Request) {
  if (!getDemoSession(request)) {
    return Response.json(
      { error: "Start a demo session before using sponsor integrations." },
      { status: 401 },
    );
  }
  return null;
}

export function enforceRateLimit(
  request: Request,
  scope: string,
  limit: number,
  windowMs: number,
) {
  const identity = `${clientIp(request)}:${getDemoSession(request) ?? "anonymous"}`;
  const key = `${scope}:${identity}`;
  const now = Date.now();
  const active = (buckets.get(key) ?? []).filter((timestamp) => now - timestamp < windowMs);

  if (active.length >= limit) {
    const retryAfter = Math.max(1, Math.ceil((windowMs - (now - active[0])) / 1000));
    return Response.json(
      { error: "Too many requests. Please try again shortly." },
      { status: 429, headers: { "Retry-After": String(retryAfter) } },
    );
  }

  active.push(now);
  buckets.set(key, active);
  return null;
}

export function demoSessionCookie(value: string) {
  const secure = process.env.NODE_ENV === "production" ? "; Secure" : "";
  return `${SESSION_COOKIE}=${value}; Path=/; HttpOnly; SameSite=Strict; Max-Age=3600${secure}`;
}
