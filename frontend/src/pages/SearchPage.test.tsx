// Pre-search suggestions: before any query, the page shows trending chips
// (by search volume) and the freshest catalog additions; picking a chip
// runs that search. Fetches are URL/method-routed (auth boot races).

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { AuthProvider } from "../auth/AuthContext";
import { SearchPage } from "./SearchPage";
import { setAccessToken } from "../api/client";
import type { User } from "../api/types";

const fetchMock = vi.fn();

const user: User = {
  id: "u1",
  email: "grace@acme.example",
  display_name: "Grace",
  role: "STUDIO",
  org_id: "org-1",
  created_at: "2026-01-01T00:00:00Z",
};

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

type Handler = () => Response;
let routes: [string, string, Handler][] = [];
const on = (method: string, fragment: string, handler: Handler) => {
  routes.push([method, fragment, handler]);
};

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  setAccessToken(null);
  routes = [
    ["POST", "/auth/refresh", () => json(200, { access_token: "tok" })],
    ["GET", "/auth/me", () => json(200, user)],
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
        <MemoryRouter initialEntries={["/search"]}>
          <Routes>
            <Route path="/search" element={<SearchPage />} />
          </Routes>
        </MemoryRouter>
      </AuthProvider>
    </QueryClientProvider>,
  );

describe("SearchPage pre-search suggestions", () => {
  it("shows trending chips and fresh additions before any query", async () => {
    on("GET", "/songs/newest", () =>
      json(200, {
        total: 1,
        hits: [
          {
            song_id: "s1",
            label_id: "org-2",
            title: "Nightcall",
            author: "Kavinsky",
            length_seconds: 252,
            box_art_key: "songs/s1/box_art/abc",
            audio_preview_key: "songs/s1/preview/abc",
            created_at: "2026-09-01T00:00:00Z",
          },
        ],
      }),
    );
    on("GET", "/songs/hot-queries", () =>
      json(200, [{ query: "nightcall", score: 7 }]),
    );

    mount();
    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "Nightcall" })).toBeInTheDocument(),
    );

    // Fresh card with its added date.
    expect(screen.getByRole("heading", { name: "Nightcall" })).toBeInTheDocument();
    const expected = new Date("2026-09-01T00:00:00Z").toLocaleDateString();
    expect(screen.getByText(`added ${expected}`, { exact: false })).toBeInTheDocument();
    // Album art and the preview player ride along.
    const art = screen.getByAltText("Nightcall box art");
    expect(art).toHaveAttribute(
      "src",
      "http://localhost:9000/song-media/songs/s1/box_art/abc",
    );
    const player = document.querySelector("audio");
    expect(player?.getAttribute("src")).toContain("/song-media/songs/s1/preview/abc");
    // Trending chip.
    expect(screen.getByRole("button", { name: "nightcall" })).toBeInTheDocument();
    // No search ran yet.
    const urls = fetchMock.mock.calls.map(([url]) => String(url));
    expect(urls.some((url) => url.includes("/songs/search?"))).toBe(false);
  });

  it("runs the search when a trending chip is picked", async () => {
    on("GET", "/songs/newest", () => json(200, { total: 0, hits: [] }));
    on("GET", "/songs/hot-queries", () =>
      json(200, [{ query: "nightcall", score: 7 }]),
    );
    let searched = false;
    on("GET", "/songs/search", () => {
      searched = true;
      return json(200, {
        total: 1,
        hits: [
          {
            song_id: "s1",
            label_id: "org-2",
            title: "Nightcall",
            author: "Kavinsky",
            length_seconds: 252,
            box_art_key: null,
            audio_preview_key: null,
          },
        ],
      });
    });

    mount();
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "nightcall" })).toBeInTheDocument(),
    );
    await userEvent.click(screen.getByRole("button", { name: "nightcall" }));

    // The input carries the query and the results replace the suggestions.
    await waitFor(() => expect(searched).toBe(true));
    expect(screen.queryByText("Fresh in the catalog")).not.toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Nightcall" })).toBeInTheDocument();
  });
});
