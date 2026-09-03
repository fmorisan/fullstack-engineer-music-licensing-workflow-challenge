import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { NotificationBell } from "./NotificationBell";
import { setAccessToken } from "../api/client";
import type { Notification } from "../api/types";

const fetchMock = vi.fn();

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
      movie_id: "m",
      scene_number: 1,
      song_id: "s",
      state: "OFFER",
      license_fee_cents: fee,
      studio_id: "st",
      label_id: "org-1",
    },
  },
  read_at: read ? "2026-01-01T00:00:05Z" : null,
  created_at: "2026-01-01T00:00:00Z",
});

const mount = () =>
  render(
    <QueryClientProvider client={new QueryClient()}>
      <NotificationBell />
    </QueryClientProvider>,
  );

beforeEach(() => {
  fetchMock.mockReset();
  vi.stubGlobal("fetch", fetchMock);
  vi.stubGlobal("EventSource", class {
    onopen: (() => void) | null = null;
    onerror: (() => void) | null = null;
    close() {}
    addEventListener() {}
  });
  setAccessToken("tok");
});

afterEach(() => vi.unstubAllGlobals());

const json = (body: unknown): Response =>
  new Response(JSON.stringify(body), { status: 200 });

describe("NotificationBell", () => {
  it("shows the unread badge from the count endpoint", async () => {
    fetchMock.mockResolvedValueOnce(json(3));
    mount();
    await waitFor(() => expect(screen.getByText("3")).toBeInTheDocument());
  });

  it("opens the inbox on click with unread highlighting and mark-read", async () => {
    fetchMock
      .mockResolvedValueOnce(json(2))
      .mockResolvedValueOnce(json([notification("n1", false), notification("n2", true)]))
      .mockResolvedValueOnce(json({}));

    mount();
    await userEvent.click(screen.getByRole("button", { name: /notifications/i }));

    const rows = await screen.findAllByText("offer received");
    expect(rows).toHaveLength(2);
    const unread = rows[0].closest(".notif");
    expect(unread).toHaveClass("unread");

    // Clicking the unread row marks it read.
    await userEvent.click(unread!);
    await waitFor(() => {
      const markReadCall = fetchMock.mock.calls.find(([url]) =>
        String(url).includes("/notifications/n1/read"),
      );
      expect(markReadCall).toBeDefined();
    });
  });

  it("marks everything read via mark-all", async () => {
    fetchMock
      .mockResolvedValueOnce(json(1))
      .mockResolvedValueOnce(json([notification("n1", false)]))
      .mockResolvedValueOnce(json({}));

    mount();
    await userEvent.click(screen.getByRole("button", { name: /notifications/i }));
    await userEvent.click(await screen.findByRole("button", { name: "Mark all read" }));

    await waitFor(() => {
      expect(
        fetchMock.mock.calls.some(([url]) => String(url).includes("/notifications/read-all")),
      ).toBe(true);
    });
  });

  it("empty inbox state", async () => {
    fetchMock
      .mockResolvedValueOnce(json(0))
      .mockResolvedValueOnce(json([]));

    mount();
    await userEvent.click(screen.getByRole("button", { name: /notifications/i }));
    expect(await screen.findByText(/Nothing yet/i)).toBeInTheDocument();
  });
});
