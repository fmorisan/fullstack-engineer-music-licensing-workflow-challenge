// MovieTimeline: proportional scene blocks (capture thumbnails as block
// backgrounds), license segments mapped from scene-relative windows into
// film time, state-colored overlays, and click-to-select.

import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MovieTimeline } from "./MovieTimeline";
import type { License, Scene } from "../api/types";

const scenes: Scene[] = [
  {
    movie_id: "m1",
    scene_number: 1,
    screen_time_seconds: 60,
    start_time_seconds: 0,
    end_time_seconds: 60,
    description: "The pursuit begins",
    capture_key: "movies/m1/scenes/1/capture/abc",
  },
  {
    movie_id: "m1",
    scene_number: 2,
    screen_time_seconds: 60,
    start_time_seconds: 60,
    end_time_seconds: 120,
    description: "Rooftop rain",
    capture_key: null,
  },
];

const license = (overrides: Partial<License> = {}): License => ({
  id: "l1",
  movie_id: "m1",
  scene_number: 1,
  song_id: "s1",
  studio_id: "org-1",
  label_id: "org-2",
  state: "ACCEPTED",
  license_fee_cents: 150000,
  start_time_seconds: 10,
  end_time_seconds: 40,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  ...overrides,
});

const mount = (licenses: License[]) =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <MovieTimeline
        scenes={scenes}
        licenses={licenses}
        selected={null}
        onSelectScene={() => {}}
      />
    </QueryClientProvider>,
  );

describe("MovieTimeline", () => {
  it("lays scene blocks proportionally and uses captures as backgrounds", () => {
    mount([]);
    const [first, second] = screen.getAllByRole("option");
    expect(first.style.left).toBe("0%");
    expect(first.style.width).toBe("50%");
    expect(first.style.backgroundImage).toContain("localhost:9000/movie-media/");
    expect(second.style.left).toBe("50%");
    expect(second.style.width).toBe("50%");
    expect(second.style.backgroundImage).toBe("");
  });

  it("maps scene-relative windows into film time", () => {
    mount([license()]);
    const segment = screen.getByTitle(/Accepted · \$1500\.00/);
    // Scene 1 starts at 0; window 10–40s lands at 8.33%..33.33% of 120s.
    expect(segment.style.left).toBe("8.333333333333332%");
    expect(segment.style.width).toBe("25%");
  });

  it("colors segments by state, renders the legend, and drops rejected", () => {
    mount([
      license({ id: "l1", state: "OFFER" }),
      license({ id: "l2", state: "COUNTER_OFFER", scene_number: 2, start_time_seconds: 0, end_time_seconds: 20 }),
      license({ id: "l3", state: "ACCEPTED" }),
      license({ id: "l4", state: "REJECTED", start_time_seconds: 45, end_time_seconds: 60 }),
    ]);
    expect(screen.getAllByTitle(/Offer out/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByTitle(/Countered/)).toBeInTheDocument();
    expect(screen.getAllByTitle(/Accepted/).length).toBeGreaterThanOrEqual(1);
    // Rejected negotiations are dead — no segment, no legend entry.
    expect(screen.queryByTitle(/Rejected/)).not.toBeInTheDocument();
    for (const label of ["Offer out", "Countered", "Accepted"]) {
      expect(screen.getByText(label, { selector: ".tl-legend span" })).toBeInTheDocument();
    }
    expect(
      screen.queryByText("Rejected", { selector: ".tl-legend span" }),
    ).not.toBeInTheDocument();
  });

  it("reports scene selection", async () => {
    const onSelectScene = vi.fn();
    render(
      <QueryClientProvider client={new QueryClient()}>
        <MovieTimeline
          scenes={scenes}
          licenses={[]}
          selected={null}
          onSelectScene={onSelectScene}
        />
      </QueryClientProvider>,
    );
    await userEvent.click(screen.getByRole("option", { name: "S2" }));
    expect(onSelectScene).toHaveBeenCalledWith(2);
  });
});
