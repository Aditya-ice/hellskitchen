import { describe, expect, it } from "vitest";
import {
  demoSessionCookie,
  enforceRateLimit,
  requireDemoSession,
  requireSameOrigin,
} from "@/lib/api-guard";

function request(headers: Record<string, string> = {}) {
  return new Request("http://localhost:3000/api/test", { headers });
}

describe("sponsor API guards", () => {
  it("rejects cross-site requests", () => {
    const response = requireSameOrigin(
      request({
        origin: "https://attacker.example",
        "sec-fetch-site": "cross-site",
      }),
    );

    expect(response?.status).toBe(403);
  });

  it("requires a valid demo session cookie", () => {
    expect(requireDemoSession(request())?.status).toBe(401);
    expect(
      requireDemoSession(
        request({
          cookie: "ember_demo_session=123e4567-e89b-12d3-a456-426614174000",
        }),
      ),
    ).toBeNull();
  });

  it("limits repeated requests for the same session", () => {
    const guardedRequest = request({
      cookie: "ember_demo_session=123e4567-e89b-12d3-a456-426614174001",
      "x-forwarded-for": "203.0.113.8",
    });
    const scope = `test-${crypto.randomUUID()}`;

    expect(enforceRateLimit(guardedRequest, scope, 2, 60_000)).toBeNull();
    expect(enforceRateLimit(guardedRequest, scope, 2, 60_000)).toBeNull();
    expect(enforceRateLimit(guardedRequest, scope, 2, 60_000)?.status).toBe(429);
  });

  it("creates a strict HTTP-only session cookie", () => {
    const cookie = demoSessionCookie("123e4567-e89b-12d3-a456-426614174000");
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("SameSite=Strict");
    expect(cookie).toContain("Max-Age=3600");
  });
});
