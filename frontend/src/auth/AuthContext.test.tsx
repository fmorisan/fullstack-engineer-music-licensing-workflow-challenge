import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { AuthProvider, useAuth } from "./AuthContext";
import { setAccessToken } from "../api/client";
import type { User } from "../api/types";

const fetchMock = vi.fn();

const studioUser: User = {
  id: "u1",
  email: "grace@acme.example",
  display_name: "Grace",
  role: "STUDIO",
  org_id: "org-1",
  created_at: "2026-01-01T00:00:00Z",
};

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  setAccessToken(null);
});

afterEach(() => {
  vi.unstubAllGlobals();
  fetchMock.mockReset();
});

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

const Probe = () => {
  const { user, ready } = useAuth();
  return <div data-testid="probe">{ready ? (user?.display_name ?? "anonymous") : "loading"}</div>;
};

const mount = () =>
  render(
    <AuthProvider>
      <MemoryRouter>
        <Probe />
      </MemoryRouter>
    </AuthProvider>,
  );

describe("AuthContext", () => {
  it("restores the session via silent refresh on boot", async () => {
    fetchMock
      .mockResolvedValueOnce(json(200, { access_token: "tok-1" }))
      .mockResolvedValueOnce(json(200, studioUser));

    mount();
    expect(screen.getByTestId("probe")).toHaveTextContent("loading");
    await waitFor(() =>
      expect(screen.getByTestId("probe")).toHaveTextContent("Grace"),
    );
    expect(fetchMock.mock.calls[0][0]).toContain("/auth/refresh");
    expect(fetchMock.mock.calls[1][0]).toContain("/auth/me");
  });

  it("stays anonymous when no refresh cookie exists", async () => {
    fetchMock.mockResolvedValueOnce(json(401, { error: { code: 401, message: "unauthorized" } }));

    mount();
    await waitFor(() =>
      expect(screen.getByTestId("probe")).toHaveTextContent("anonymous"),
    );
  });

  it("login stores the access token and user", async () => {
    const LoginProbe = () => {
      const { login } = useAuth();
      return (
        <button type="button" onClick={() => login("a@b.c", "password-1")}>
          login
        </button>
      );
    };
    fetchMock
      // Boot silent refresh: no cookie yet.
      .mockResolvedValueOnce(json(401, { error: { code: 401, message: "unauthorized" } }))
      .mockResolvedValueOnce(json(200, { user: studioUser, access_token: "tok-2" }));

    render(
      <AuthProvider>
        <MemoryRouter>
          <LoginProbe />
        </MemoryRouter>
      </AuthProvider>,
    );
    const button = screen.getByRole("button", { name: "login" });
    // Let the boot silent-refresh finish before clicking.
    await waitFor(() => expect(fetchMock).toHaveBeenCalledTimes(1));
    await userEvent.click(button);
    await waitFor(() =>
      expect(fetchMock.mock.calls[1][0]).toContain("/auth/login"),
    );
  });
});
