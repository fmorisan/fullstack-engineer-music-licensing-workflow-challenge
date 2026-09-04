// The label's movie context: full timeline with every license on the
// movie, and boards where only the label's own rows carry actions.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { AuthProvider } from "../auth/AuthContext";
import { LabelMovieContextPage } from "./LabelMovieContextPage";
import { setAccessToken } from "../api/client";
import type { License, User } from "../api/types";

const fetchMock = vi.fn();

const labelUser: User = {
  id: "u2",
  email: "warp-label@acme.example",
  display_name: "Warp",
  role: "LABEL",
  org_id: "org-label",
  created_at: "2026-01-01T00:00:00Z",
};

const license = (overrides: Partial<License>): License => ({
  id: "l1",
  movie_id: "m1",
  scene_number: 1,
  song_id: "s1",
  studio_id: "org-studio",
  label_id: "org-label",
  state: "OFFER",
  license_fee_cents: 150000,
  start_time_seconds: 10,
  end_time_seconds: 40,
  created_at: "2026-01-02T00:00:00Z",
  updated_at: "2026-01-02T00:00:00Z",
  ...overrides,
});

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

type Handler = () => Response;
let routes: [string, string, Handler][] = [];

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  setAccessToken(null);
  routes = [
    ["POST", "/auth/refresh", () => json(200, { access_token: "tok" })],
    ["GET", "/auth/me", () => json(200, labelUser)],
    ["GET", "/licenses/movies/m1", () =>
      json(200, [
        license({ id: "own", song_id: "s1", label_id: "org-label" }),
        license({
          id: "foreign",
          song_id: "s2",
          label_id: "org-other",
          state: "ACCEPTED",
        }),
      ])],
    ["GET", "/movies/m1", () =>
      json(200, {
        movie: {
          id: "m1",
          studio_id: "org-studio",
          title: "Neon Pursuit",
          description: "A chase.",
          poster_key: null,
          created_at: "2026-01-01T00:00:00Z",
          updated_at: "2026-01-01T00:00:00Z",
        },
        scenes: [
          {
            movie_id: "m1",
            scene_number: 1,
            screen_time_seconds: 60,
            start_time_seconds: 0,
            end_time_seconds: 60,
            description: "The decision scene",
            capture_key: null,
          },
        ],
      })],
    ["GET", "/songs/s1", () =>
      json(200, {
        id: "s1",
        label_id: "org-label",
        title: "Nightcall",
        author: "Kavinsky",
        length_seconds: 252,
        box_art_key: null,
        audio_preview_key: null,
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      })],
    ["GET", "/songs/s2", () =>
      json(200, {
        id: "s2",
        label_id: "org-other",
        title: "Turbo Killer",
        author: "Carpenter Brut",
        length_seconds: 200,
        box_art_key: null,
        audio_preview_key: null,
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      })],
  ];
  fetchMock.mockImplementation((url: RequestInfo | URL, init?: RequestInit) => {
    const target = String(url);
    for (const [method, fragment, handler] of routes) {
      if (init?.method === method && target.includes(fragment)) {
        return Promise.resolve(handler());
      }
    }
    return Promise.reject(new Error(`unmocked fetch: ${init?.method} ${target}`));
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  fetchMock.mockReset();
});

const mount = () =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AuthProvider>
        <MemoryRouter initialEntries={["/movies/m1/context"]}>
          <Routes>
            <Route path="/movies/:movieId/context" element={<LabelMovieContextPage />} />
          </Routes>
        </MemoryRouter>
      </AuthProvider>
    </QueryClientProvider>,
  );

describe("LabelMovieContextPage", () => {
  it("shows the movie, every license on the timeline, and actable own rows only", async () => {
    mount();

    await waitFor(() => expect(screen.getByText("Neon Pursuit")).toBeInTheDocument());

    // Both licenses appear on the timeline and in the scene board.
    await waitFor(() =>
      expect(
        screen.getAllByTitle(/Nightcall — Kavinsky · Offer out/).length,
      ).toBeGreaterThanOrEqual(1),
    );
    expect(
      screen.getAllByTitle(/Turbo Killer — Carpenter Brut · Accepted/).length,
    ).toBeGreaterThanOrEqual(1);

    const ownRow = screen.getByText("Nightcall — Kavinsky").closest("tr")!;
    expect(within(ownRow).getByRole("button", { name: "Accept" })).toBeInTheDocument();

    const foreignRow = screen.getByText("Turbo Killer — Carpenter Brut").closest("tr")!;
    expect(within(foreignRow).queryByRole("button")).toBeNull();
    expect(within(foreignRow).getByText(/another label's license/)).toBeInTheDocument();
  });
});
