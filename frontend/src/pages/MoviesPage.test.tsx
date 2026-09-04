// Creating a movie with a poster picked: the create-then-upload chain.
// Presigning needs an existing owner, so POST /movies runs first and the
// poster rides the standard pipeline; an upload failure downgrades to a
// warning while the movie itself survives.
//
// Fetches are routed by URL/method, not call order — the auth boot and the
// list query race on mount.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { AuthProvider } from "../auth/AuthContext";
import { MoviesPage } from "./MoviesPage";
import { setAccessToken } from "../api/client";
import type { Movie, User } from "../api/types";

const fetchMock = vi.fn();

const user: User = {
  id: "u1",
  email: "grace@acme.example",
  display_name: "Grace",
  role: "STUDIO",
  org_id: "org-1",
  created_at: "2026-01-01T00:00:00Z",
};

const makeMovie = (overrides: Partial<Movie> = {}): Movie => ({
  id: "m1",
  studio_id: "org-1",
  title: "Neon Pursuit",
  description: "A rain-soaked chase through a neon city.",
  poster_key: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  ...overrides,
});

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

type Handler = (init: RequestInit) => Response;
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
        return Promise.resolve(handler(init ?? {}));
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
        <MemoryRouter initialEntries={["/movies"]}>
          <Routes>
            <Route path="/movies" element={<MoviesPage />} />
          </Routes>
        </MemoryRouter>
      </AuthProvider>
    </QueryClientProvider>,
  );

const pickPoster = () => {
  const input = document.querySelector('input[type="file"]') as HTMLInputElement;
  fireEvent.change(input, {
    target: { files: [new File(["bytes"], "poster.png", { type: "image/png" })] },
  });
};

const fillForm = async () => {
  await waitFor(() => expect(screen.getByPlaceholderText("Neon Pursuit")).toBeEnabled());
  pickPoster();
  await userEvent.type(screen.getByPlaceholderText("Neon Pursuit"), "Neon Pursuit");
  await userEvent.click(screen.getByRole("button", { name: "Add movie" }));
};

describe("MoviesPage create-with-poster", () => {
  it("creates the movie, uploads the poster, and shows the thumbnail", async () => {
    const listed: Movie[] = [];
    on("GET", "8080/movies", () => json(200, [...listed]));
    on("POST", "8080/movies", () => {
      listed.push(makeMovie());
      return json(200, makeMovie());
    });
    on("PUT", "/movies/m1/poster", () =>
      json(200, {
        upload_url: "http://localhost:9000/movie-media/movies/m1/poster/abc?sig",
        object_key: "movies/m1/poster/abc",
        expires_in: 300,
      }),
    );
    on("PUT", "movie-media", () => new Response(null, { status: 200 }));
    on("PUT", "8080/movies/m1", () => {
      listed[0] = makeMovie({ poster_key: "movies/m1/poster/abc" });
      return json(200, listed[0]);
    });

    mount();
    await fillForm();

    await waitFor(() =>
      expect(screen.getByAltText("Neon Pursuit poster")).toBeInTheDocument(),
    );

    // The raw PUT skipped the bearer — the URL is the authorization.
    const upload = fetchMock.mock.calls.find(([url]) =>
      String(url).includes("movie-media/movies/m1/poster/abc?sig"),
    );
    expect(upload).toBeDefined();
    expect(upload![1].headers).toEqual({ "content-type": "image/png" });
  });

  it("downgrades a failed poster upload to a warning while keeping the movie", async () => {
    const listed: Movie[] = [];
    on("GET", "8080/movies", () => json(200, [...listed]));
    on("POST", "8080/movies", () => {
      listed.push(makeMovie());
      return json(200, makeMovie());
    });
    on("PUT", "/movies/m1/poster", () =>
      json(500, { error: { code: 500, message: "boom" } }),
    );

    mount();
    await fillForm();

    await waitFor(() =>
      expect(screen.getByText(/poster upload failed/)).toBeInTheDocument(),
    );
    expect(screen.getByRole("heading", { name: "Neon Pursuit" })).toBeInTheDocument();
  });
});
