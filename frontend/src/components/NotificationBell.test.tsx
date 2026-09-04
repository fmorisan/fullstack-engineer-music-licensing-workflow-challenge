// Notification bell: unread badge, enriched inbox rows (movie name + song
// title), mark-read, and click-through to the role-appropriate decision
// page. Fetches are URL/method-routed; SSE is stubbed offline.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes, useLocation } from "react-router-dom";
import { NotificationBell } from "./NotificationBell";
import { AuthProvider } from "../auth/AuthContext";
import { setAccessToken } from "../api/client";
import type { Notification, User } from "../api/types";

const fetchMock = vi.fn();

const labelUser: User = {
  id: "u2",
  email: "warp-label@acme.example",
  display_name: "Warp",
  role: "LABEL",
  org_id: "org-1",
  created_at: "2026-01-01T00:00:00Z",
};

const notification = (id: string, read: boolean, fee = 120_000): Notification => ({
  id,
  recipient_user_id: null,
  recipient_org_id: "org-1",
  notification_type: "OFFER_RECEIVED",
  payload: {
    event_id: `evt-${id}`,
    kind: "CREATED",
    license: {
      license_id: "lic",
      movie_id: "m1",
      scene_number: 3,
      song_id: "s1",
      state: "OFFER",
      license_fee_cents: fee,
      studio_id: "st",
      label_id: "org-1",
    },
  },
  read_at: read ? "2026-01-01T00:00:05Z" : null,
  created_at: "2026-01-01T00:00:00Z",
});

const json = (status: number, body: unknown): Response =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });

type Handler = () => Response;
let routes: [string, string, Handler][] = [];
const on = (method: string, fragment: string, handler: Handler) => {
  routes.unshift([method, fragment, handler]); // later registrations win
};

const LocationProbe = () => {
  const location = useLocation();
  return <div data-testid="location">{location.pathname}</div>;
};

const mount = () =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <AuthProvider>
        <MemoryRouter initialEntries={["/"]}>
          <LocationProbe />
          <Routes>
            <Route path="/movies/:movieId/context" element={<div>context page</div>} />
            <Route path="/movies/:movieId" element={<div>movie page</div>} />
            <Route path="/" element={<NotificationBell />} />
          </Routes>
        </MemoryRouter>
      </AuthProvider>
    </QueryClientProvider>,
  );

beforeEach(() => {
  routes = [
    ["POST", "/auth/refresh", () => json(200, { access_token: "tok" })],
    ["GET", "/auth/me", () => json(200, labelUser)],
    ["GET", "/notifications/unread_count", () => json(200, 2)],
    ["GET", "/notifications", () => json(200, [notification("n1", false), notification("n2", true)])],
    ["GET", "/movies/m1", () =>
      json(200, {
        movie: {
          id: "m1",
          studio_id: "st",
          title: "Neon Pursuit",
          description: "",
          poster_key: null,
          created_at: "2026-01-01T00:00:00Z",
          updated_at: "2026-01-01T00:00:00Z",
        },
        scenes: [],
      })],
    ["GET", "/songs/s1", () =>
      json(200, {
        id: "s1",
        label_id: "org-1",
        title: "Nightcall",
        author: "Kavinsky",
        length_seconds: 252,
        box_art_key: null,
        audio_preview_key: null,
        created_at: "2026-01-01T00:00:00Z",
        updated_at: "2026-01-01T00:00:00Z",
      })],
    ["PUT", "/notifications/n1/read", () => json(200, {})],
    ["PUT", "/notifications/read-all", () => json(200, {})],
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
  vi.stubGlobal("fetch", fetchMock);
  vi.stubGlobal(
    "EventSource",
    class {
      onopen: (() => void) | null = null;
      onerror: (() => void) | null = null;
      close() {}
      addEventListener() {}
    },
  );
  setAccessToken("tok");
});

afterEach(() => vi.unstubAllGlobals());

const openInbox = async () => {
  await userEvent.click(await screen.findByRole("button", { name: /notifications/i }));
};

describe("NotificationBell", () => {
  it("shows the unread badge from the count endpoint", async () => {
    mount();
    expect(await screen.findByText("2")).toBeInTheDocument();
  });

  it("enriches rows with the song, scene, and movie under decision", async () => {
    mount();
    await openInbox();

    expect(await screen.findAllByText("offer received")).toHaveLength(2);
    // Both rows carry the enrichment (song · scene · movie).
    expect(
      await screen.findAllByText(/“Nightcall — Kavinsky” · scene 3 · Neon Pursuit/),
    ).toHaveLength(2);

    const rows = screen.getAllByText("offer received");
    expect(rows[0].closest(".notif")).toHaveClass("unread");
    expect(rows[1].closest(".notif")).toHaveClass("read");
  });

  it("clicking a row navigates to the label decision page and marks it read", async () => {
    mount();
    await openInbox();

    await userEvent.click((await screen.findAllByText("offer received"))[0].closest(".notif")!);

    await waitFor(() =>
      expect(screen.getByTestId("location")).toHaveTextContent("/movies/m1/context"),
    );
    expect(screen.getByText("context page")).toBeInTheDocument();
    await waitFor(() => {
      expect(
        fetchMock.mock.calls.some(
          ([url, init]) =>
            init?.method === "PUT" && String(url).includes("/notifications/n1/read"),
        ),
      ).toBe(true);
    });
  });

  it("marks everything read via mark-all", async () => {
    mount();
    await openInbox();
    await userEvent.click(await screen.findByRole("button", { name: "Mark all read" }));

    await waitFor(() => {
      expect(
        fetchMock.mock.calls.some(
          ([url, init]) =>
            init?.method === "PUT" && String(url).includes("/notifications/read-all"),
        ),
      ).toBe(true);
    });
  });

  it("empty inbox state", async () => {
    on("GET", "/notifications", () => json(200, []));
    mount();
    await openInbox();
    expect(await screen.findByText(/Nothing yet/i)).toBeInTheDocument();
  });
});
