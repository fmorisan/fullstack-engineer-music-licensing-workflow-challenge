// Publishing a song with media: over-cap clips are refused at pick time
// (before the song or the bytes leave the browser), and picked media rides
// the create-then-upload chain. Fetches are routed by URL/method because
// the auth boot and the catalog query race on mount.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { AuthProvider } from "../auth/AuthContext";
import { CatalogPage } from "./CatalogPage";
import { setAccessToken } from "../api/client";
import type { Song, User } from "../api/types";

const fetchMock = vi.fn();

const user: User = {
  id: "u2",
  email: "warp-label@acme.example",
  display_name: "Warp",
  role: "LABEL",
  org_id: "org-2",
  created_at: "2026-01-01T00:00:00Z",
};

const song: Song = {
  id: "s1",
  label_id: "org-2",
  title: "Nightcall",
  author: "Kavinsky",
  length_seconds: 195,
  box_art_key: null,
  audio_preview_key: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

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
        <MemoryRouter initialEntries={["/catalog"]}>
          <Routes>
            <Route path="/catalog" element={<CatalogPage />} />
          </Routes>
        </MemoryRouter>
      </AuthProvider>
    </QueryClientProvider>,
  );

/** Stub <audio> metadata with a fixed duration, as in media.test.ts. */
const withFakeAudio = (duration: number) => {
  const element = {
    duration,
    onloadedmetadata: null as (() => void) | null,
    onerror: null as (() => void) | null,
    preload: "",
  };
  Object.defineProperty(element, "src", {
    set() {
      queueMicrotask(() => element.onloadedmetadata?.());
    },
  });
  vi.spyOn(document, "createElement").mockImplementation((tag: string) =>
    tag === "audio"
      ? (element as unknown as HTMLElement)
      : (document.createElementNS("http://www.w3.org/1999/xhtml", tag) as HTMLElement),
  );
  vi.stubGlobal("URL", {
    ...URL,
    createObjectURL: vi.fn().mockReturnValue("blob:x"),
    revokeObjectURL: vi.fn(),
  });
};

const inputs = () =>
  Array.from(document.querySelectorAll('input[type="file"]')) as HTMLInputElement[];

const fillForm = async () => {
  await waitFor(() => expect(screen.getByPlaceholderText("Nightcall")).toBeEnabled());
  await userEvent.type(screen.getByPlaceholderText("Nightcall"), "Nightcall");
  await userEvent.type(screen.getByPlaceholderText("Kavinsky"), "Kavinsky");
};

describe("CatalogPage create-with-media", () => {
  it("refuses over-cap preview clips at pick time", async () => {
    withFakeAudio(31);
    const listed: Song[] = [];
    on("GET", "8080/songs", () => json(200, [...listed]));
    on("POST", "8080/songs", () => {
      listed.push(song);
      return json(200, song);
    });

    mount();
    await fillForm();

    const clip = new File(["bytes"], "clip.mp3", { type: "audio/mpeg" });
    fireEvent.change(inputs()[1], { target: { files: [clip] } }); // preview picker

    await waitFor(() => expect(screen.getByText(/capped at 30s/)).toBeInTheDocument());
    await userEvent.click(screen.getByRole("button", { name: "Publish song" }));

    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "Nightcall" })).toBeInTheDocument(),
    );
    const urls = fetchMock.mock.calls.map(([url]) => String(url));
    expect(urls.some((url) => url.includes("/audio_preview"))).toBe(false);
  });

  it("chains box art and preview uploads after publishing", async () => {
    withFakeAudio(12);
    const listed: Song[] = [];
    on("GET", "8080/songs", () => json(200, [...listed]));
    on("POST", "8080/songs", () => {
      listed.push(song);
      return json(200, song);
    });
    on("PUT", "/songs/s1/box_art", () =>
      json(200, {
        upload_url: "http://localhost:9000/song-media/songs/s1/box_art/abc?sig",
        object_key: "songs/s1/box_art/abc",
        expires_in: 300,
      }),
    );
    on("PUT", "/songs/s1/audio_preview", () =>
      json(200, {
        upload_url: "http://localhost:9000/song-media/songs/s1/preview/abc?sig",
        object_key: "songs/s1/preview/abc",
        expires_in: 300,
      }),
    );
    on("PUT", "song-media", () => new Response(null, { status: 200 }));
    on("PUT", "8080/songs/s1", () => json(200, song));

    mount();
    await fillForm();

    fireEvent.change(inputs()[0], {
      target: { files: [new File(["bytes"], "art.png", { type: "image/png" })] },
    });
    fireEvent.change(inputs()[1], {
      target: { files: [new File(["bytes"], "clip.mp3", { type: "audio/mpeg" })] },
    });
    await userEvent.click(screen.getByRole("button", { name: "Publish song" }));

    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "Nightcall" })).toBeInTheDocument(),
    );
    const urls = fetchMock.mock.calls.map(([url]) => String(url));
    expect(urls.some((url) => url.includes("/songs/s1/box_art"))).toBe(true);
    expect(urls.some((url) => url.includes("/songs/s1/audio_preview"))).toBe(true);
  });
});
