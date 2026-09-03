import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { LicenseBoard } from "./LicenseBoard";
import type { License, Scene } from "../api/types";
import { setAccessToken } from "../api/client";

const fetchMock = vi.fn();

const scene: Scene = {
  movie_id: "m1",
  scene_number: 1,
  screen_time_seconds: 120,
  start_time_seconds: 0,
  end_time_seconds: 120,
  description: "The chase",
  capture_key: null,
};

const license = (state: License["state"], fee = 150_000): License => ({
  id: "lic-1",
  movie_id: "m1",
  scene_number: 1,
  song_id: "song-1",
  studio_id: "s",
  label_id: "l",
  state,
  license_fee_cents: fee,
  start_time_seconds: 0,
  end_time_seconds: 30,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
});

const songResponse = {
  id: "song-1",
  title: "Nightcall",
  author: "Kavinsky",
  label_id: "l",
  length_seconds: 252,
  box_art_key: null,
  audio_preview_key: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

interface MountOptions {
  state?: License["state"];
  perspective?: "studio" | "label";
}

const mount = ({ state = "OFFER", perspective = "studio" }: MountOptions = {}) =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>
        <LicenseBoard
          movieId="m1"
          scene={scene}
          licenses={[license(state)]}
          perspective={perspective}
        />
      </MemoryRouter>
    </QueryClientProvider>,
  );

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  setAccessToken("tok");
  fetchMock.mockImplementation(async (url: string) => {
    if (url.includes("/songs/song-1")) {
      return new Response(JSON.stringify(songResponse), { status: 200 });
    }
    return new Response(JSON.stringify({}), { status: 200 });
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  fetchMock.mockReset();
});

describe("LicenseBoard", () => {
  it("shows the song title and state badge", async () => {
    mount({ state: "OFFER", perspective: "label" });
    expect(await screen.findByText("Nightcall — Kavinsky")).toBeInTheDocument();
    expect(screen.getByText("Offer out")).toBeInTheDocument();
    expect(screen.getByText("$1500.00")).toBeInTheDocument();
  });

  it("gates actions by role and state (label on OFFER)", async () => {
    mount({ state: "OFFER", perspective: "label" });
    expect(screen.getByRole("button", { name: "Accept" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Counter" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Accept counter" })).not.toBeInTheDocument();
  });

  it("studio sees accept-counter on COUNTER_OFFER, not on OFFER", () => {
    mount({ state: "COUNTER_OFFER", perspective: "studio" });
    expect(screen.getByRole("button", { name: "Accept counter" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Accept" })).not.toBeInTheDocument();
  });

  it("terminal states offer no actions", () => {
    mount({ state: "ACCEPTED", perspective: "studio" });
    expect(screen.queryByRole("button", { name: /accept/i })).not.toBeInTheDocument();
    expect(screen.getByText("Accepted")).toBeInTheDocument();
  });

  it("countering sends the fee in cents", async () => {
    mount({ state: "OFFER", perspective: "label" });
    await userEvent.click(screen.getByRole("button", { name: "Counter" }));
    const feeInput = screen.getByRole("spinbutton");
    await userEvent.clear(feeInput);
    await userEvent.type(feeInput, "2200.50");
    await userEvent.click(screen.getByRole("button", { name: "Send" }));

    await waitFor(() => {
      const call = fetchMock.mock.calls.find(([url]) => String(url).includes("/licenses/lic-1"));
      expect(call).toBeDefined();
      const init = call![1] as RequestInit;
      expect(JSON.parse(String(init.body))).toEqual({
        action: "COUNTER_OFFER",
        license_fee_cents: 220050,
      });
    });
  });
});
