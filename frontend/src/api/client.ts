// Fetch client with in-memory access token and silent refresh (ADR-012).
//
// The access token lives only in JS memory (XSS cannot read the HttpOnly
// refresh cookie). Every request rides a bearer; a 401 triggers exactly one
// refresh-and-retry via the /auth/refresh cookie endpoint before surfacing
// the failure to callers.

const API_URL: string = import.meta.env.VITE_API_URL ?? "http://localhost:8080";

let accessToken: string | null = null;
let refreshInFlight: Promise<boolean> | null = null;

export const setAccessToken = (token: string | null): void => {
  accessToken = token;
};

export const getAccessToken = (): string | null => accessToken;

export class ApiError extends Error {
  status: number;
  code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.status = status;
    this.code = code;
  }
}

const request = async <T>(
  path: string,
  init: RequestInit,
  allowRetry: boolean,
): Promise<T> => {
  const headers = new Headers(init.headers);
  if (accessToken) headers.set("Authorization", `Bearer ${accessToken}`);
  if (init.body && !headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }

  const response = await fetch(`${API_URL}${path}`, {
    ...init,
    headers,
    credentials: "include", // refresh cookie rides along
  });

  if (response.status === 401 && allowRetry) {
    const refreshed = await refresh();
    if (refreshed) return request<T>(path, init, false);
  }

  if (response.status === 204) return undefined as T;

  const text = await response.text();
  const body = text ? JSON.parse(text) : null;
  if (!response.ok) {
    const message =
      body?.error?.message ?? body?.message ?? `request failed (${response.status})`;
    throw new ApiError(response.status, body?.error?.code ?? "", message);
  }
  return body as T;
};

/** One refresh at a time; concurrent 401s share the flight. */
export const refresh = (): Promise<boolean> => {
  refreshInFlight ??= (async () => {
    try {
      const body = await request<{ access_token: string }>(
        "/auth/refresh",
        { method: "POST" },
        false,
      );
      accessToken = body.access_token;
      return true;
    } catch {
      accessToken = null;
      return false;
    } finally {
      refreshInFlight = null;
    }
  })();
  return refreshInFlight;
};

export const api = {
  get: <T>(path: string) => request<T>(path, { method: "GET" }, true),
  post: <T>(path: string, body?: unknown) =>
    request<T>(
      path,
      { method: "POST", body: body === undefined ? undefined : JSON.stringify(body) },
      true,
    ),
  put: <T>(path: string, body?: unknown) =>
    request<T>(
      path,
      { method: "PUT", body: body === undefined ? undefined : JSON.stringify(body) },
      true,
    ),
};

export const sseUrl = (path: string): string =>
  accessToken ? `${API_URL}${path}?access_token=${encodeURIComponent(accessToken)}` : `${API_URL}${path}`;
