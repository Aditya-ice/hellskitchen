import { afterEach, describe, expect, it, vi } from "vitest";
import { apiFetch, clearSessionToken } from "./api-client";

describe("apiFetch session recovery", () => {
  afterEach(() => {
    clearSessionToken();
    vi.unstubAllGlobals();
  });

  it("mints a new token and retries once after a 401", async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(
        Response.json({ token: "11111111-1111-4111-8111-111111111111" }),
      )
      .mockResolvedValueOnce(Response.json({ error: "expired" }, { status: 401 }))
      .mockResolvedValueOnce(
        Response.json({ token: "22222222-2222-4222-8222-222222222222" }),
      )
      .mockResolvedValueOnce(Response.json({ ok: true }));
    vi.stubGlobal("fetch", fetchMock);

    await expect(apiFetch<{ ok: boolean }>("/v1/state")).resolves.toEqual({
      ok: true,
    });

    expect(fetchMock).toHaveBeenCalledTimes(4);
    expect(fetchMock.mock.calls[1][1]?.headers).toMatchObject(
      expect.any(Headers),
    );
    expect(
      (fetchMock.mock.calls[1][1]?.headers as Headers).get("Authorization"),
    ).toBe("Bearer 11111111-1111-4111-8111-111111111111");
    expect(
      (fetchMock.mock.calls[3][1]?.headers as Headers).get("Authorization"),
    ).toBe("Bearer 22222222-2222-4222-8222-222222222222");
  });

  it("does not retry more than once when the refreshed token is rejected", async () => {
    const fetchMock = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(
        Response.json({ token: "33333333-3333-4333-8333-333333333333" }),
      )
      .mockResolvedValueOnce(Response.json({ error: "expired" }, { status: 401 }))
      .mockResolvedValueOnce(
        Response.json({ token: "44444444-4444-4444-8444-444444444444" }),
      )
      .mockResolvedValueOnce(
        Response.json({ error: "still unauthorized" }, { status: 401 }),
      );
    vi.stubGlobal("fetch", fetchMock);

    await expect(apiFetch("/v1/state")).rejects.toThrow("still unauthorized");
    expect(fetchMock).toHaveBeenCalledTimes(4);
  });
});
