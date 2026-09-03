import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, api, getAccessToken, refresh, setAccessToken } from "./client";

const fetchMock = vi.fn();

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  setAccessToken("stale-token");
});

afterEach(() => {
  vi.unstubAllGlobals();
  fetchMock.mockReset();
});

const jsonResponse = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

describe("api client", () => {
  it("attaches the bearer token and parses JSON bodies", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(200, { hello: "world" }));
    const result = await api.get("/anything");
    expect(result).toEqual({ hello: "world" });
    const [, init] = fetchMock.mock.calls[0];
    expect(new Headers(init.headers).get("authorization")).toBe(
      "Bearer stale-token",
    );
    expect(init.credentials).toBe("include");
  });

  it("silently refreshes once on 401 and retries the request", async () => {
    fetchMock
      .mockResolvedValueOnce(jsonResponse(401, { error: { code: 401, message: "unauthorized" } }))
      .mockResolvedValueOnce(jsonResponse(200, { access_token: "fresh-token" }))
      .mockResolvedValueOnce(jsonResponse(200, { secret: "data" }));

    const result = await api.get("/needs-auth");
    expect(result).toEqual({ secret: "data" });
    expect(getAccessToken()).toBe("fresh-token");
    expect(fetchMock).toHaveBeenCalledTimes(3);

    // Retry carried the fresh token.
    const [, retryInit] = fetchMock.mock.calls[2];
    expect(new Headers(retryInit.headers).get("authorization")).toBe(
      "Bearer fresh-token",
    );
  });

  it("shares one refresh flight across concurrent 401s", async () => {
    fetchMock
      .mockResolvedValueOnce(jsonResponse(401, { error: { code: 401, message: "unauthorized" } }))
      .mockResolvedValueOnce(jsonResponse(401, { error: { code: 401, message: "unauthorized" } }))
      .mockResolvedValueOnce(jsonResponse(200, { access_token: "one-flight" }))
      .mockResolvedValueOnce(jsonResponse(200, { a: 1 }))
      .mockResolvedValueOnce(jsonResponse(200, { b: 2 }));

    const [a, b] = await Promise.all([api.get("/a"), api.get("/b")]);
    expect(a).toEqual({ a: 1 });
    expect(b).toEqual({ b: 2 });
    // Two failing calls + one refresh + two retries = 5; a second refresh
    // flight would make it 6.
    expect(fetchMock).toHaveBeenCalledTimes(5);
  });

  it("surfaces ApiError when the refresh also fails", async () => {
    setAccessToken(null);
    fetchMock
      .mockResolvedValueOnce(jsonResponse(401, { error: { code: 401, message: "unauthorized" } }))
      .mockResolvedValueOnce(jsonResponse(401, { error: { code: 401, message: "unauthorized" } }));

    await expect(api.get("/locked")).rejects.toBeInstanceOf(ApiError);
    expect(getAccessToken()).toBeNull();
  });

  it("serializes bodies as JSON with the right content type", async () => {
    fetchMock.mockResolvedValueOnce(jsonResponse(201, { ok: true }));
    await api.post("/things", { name: "x" });
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toContain("/things");
    expect(init.body).toBe(JSON.stringify({ name: "x" }));
    expect(new Headers(init.headers).get("content-type")).toBe("application/json");
  });

  it("refresh() resolves false without an endpoint", async () => {
    fetchMock.mockRejectedValueOnce(new TypeError("network down"));
    await expect(refresh()).resolves.toBe(false);
  });
});
