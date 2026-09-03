// Regression: the login submit MUST navigate away once authentication
// succeeds (the original bug: state updated, URL never changed), and
// /login must bounce already-authenticated visits.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  MemoryRouter,
  Navigate,
  Route,
  Routes,
  useLocation,
} from "react-router-dom";
import { AuthProvider } from "../auth/AuthContext";
import { AnonymousOnly } from "../components/AnonymousOnly";
import { LoginPage } from "./LoginPage";
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

/** Records the current path so assertions can prove navigation. */
const LocationProbe = () => {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
};

const mount = (initial: string) =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AuthProvider>
        <MemoryRouter initialEntries={[initial]}>
          <LocationProbe />
          <Routes>
            <Route
              path="/login"
              element={
                <AnonymousOnly>
                  <LoginPage />
                </AnonymousOnly>
              }
            />
            <Route path="/" element={<Navigate to="/movies" replace />} />
            <Route path="/movies" element={<div>movies page</div>} />
          </Routes>
        </MemoryRouter>
      </AuthProvider>
    </QueryClientProvider>,
  );

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  setAccessToken(null);
});

afterEach(() => {
  vi.unstubAllGlobals();
  fetchMock.mockReset();
});

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

describe("LoginPage navigation (regression)", () => {
  it("navigates to the app after a successful login", async () => {
    fetchMock
      // Boot silent refresh: no cookie.
      .mockResolvedValueOnce(json(401, { error: { code: 401, message: "unauthorized" } }))
      // The login itself.
      .mockResolvedValueOnce(json(200, { user: studioUser, access_token: "tok" }));

    mount("/login");

    // Boot settles (guard flips to children) once the form is reachable.
    await waitFor(() => expect(screen.getByLabelText(/email/i)).toBeInTheDocument());
    await userEvent.type(screen.getByLabelText(/email/i), "grace@acme.example");
    await userEvent.type(screen.getByLabelText(/password/i), "nw-derulo-99");
    const submit = screen
      .getAllByRole("button", { name: "Sign in" })
      .find((button) => button.getAttribute("type") === "submit")!;
    await userEvent.click(submit);

    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent("/movies"),
    );
    expect(await screen.findByText("movies page")).toBeInTheDocument();
  });

  it("shows the error inline and stays put on failed login", async () => {
    fetchMock
      .mockResolvedValueOnce(json(401, { error: { code: 401, message: "unauthorized" } }))
      .mockResolvedValueOnce(json(401, { error: { code: 401, message: "invalid credentials" } }));

    mount("/login");
    await waitFor(() => expect(screen.getByLabelText(/email/i)).toBeInTheDocument());
    await userEvent.type(screen.getByLabelText(/email/i), "grace@acme.example");
    await userEvent.type(screen.getByLabelText(/password/i), "wrong-pass-99");
    const submit = screen
      .getAllByRole("button", { name: "Sign in" })
      .find((button) => button.getAttribute("type") === "submit")!;
    await userEvent.click(submit);

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("invalid credentials"),
    );
    expect(screen.getByTestId("location")).toHaveTextContent("/login");
  });
});

describe("AnonymousOnly guard", () => {
  it("bounces authenticated users to /", async () => {
    fetchMock
      .mockResolvedValueOnce(json(200, { access_token: "tok" }))
      .mockResolvedValueOnce(json(200, studioUser));

    mount("/login");
    // Guard waits for boot, then redirects; Home (not mounted here) would
    // route onward — assert we left /login.
    await waitFor(() =>
      expect(screen.getByTestId("location")).not.toHaveTextContent("/login"),
    );
  });
});

