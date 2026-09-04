// Media upload client: presign → PUT bytes → record-the-key sequence,
// partial-update bodies (title/author/length ride along), and the
// client-side 30s preview cap.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setAccessToken } from "./client";
import {
  MAX_PREVIEW_SECONDS,
  mediaUrl,
  uploadAudioPreview,
  uploadBoxArt,
  uploadMoviePoster,
} from "./media";
import type { Movie, Song } from "./types";

const fetchMock = vi.fn();

const movie: Movie = {
  id: "m1",
  studio_id: "org-1",
  title: "Neon Horizon",
  description: "A chase through a rainy megacity.",
  poster_key: null,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
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

const grant = {
  upload_url: "http://localhost:9000/movie-media/movies/m1/poster/abc",
  object_key: "movies/m1/poster/abc",
  expires_in: 300,
};

const file = (type: string) => new File(["bytes"], "art.png", { type });

beforeEach(() => {
  vi.stubGlobal("fetch", fetchMock);
  setAccessToken("tok");
});

afterEach(() => {
  vi.unstubAllGlobals();
  fetchMock.mockReset();
});

describe("mediaUrl", () => {
  it("points at the right bucket", () => {
    expect(mediaUrl("movie", "k")).toBe("http://localhost:9000/movie-media/k");
    expect(mediaUrl("song", "k")).toBe("http://localhost:9000/song-media/k");
  });
});

describe("uploadMoviePoster", () => {
  it("presigns, PUTs the bytes raw, and re-sends overwriting fields", async () => {
    fetchMock
      .mockResolvedValueOnce(json(200, grant)) // presign
      .mockResolvedValueOnce(new Response(null, { status: 200 })) // PUT bytes
      .mockResolvedValueOnce(json(200, { ...movie, poster_key: grant.object_key }));

    const updated = await uploadMoviePoster(movie, file("image/png"));
    expect(updated.poster_key).toBe(grant.object_key);

    const presign = fetchMock.mock.calls[0];
    expect(presign[0]).toBe("http://localhost:8080/movies/m1/poster");
    expect(presign[1].method).toBe("PUT");
    // Presign body is required even when empty.
    expect(presign[1].body).toBe("{}");

    const upload = fetchMock.mock.calls[1];
    expect(upload[0]).toBe(grant.upload_url);
    expect(upload[1].method).toBe("PUT");
    expect(upload[1].body).toBeInstanceOf(File);
    // The presigned URL is its own authorization — no bearer leaks to MinIO.
    expect(upload[1].headers).toEqual({ "content-type": "image/png" });

    const record = fetchMock.mock.calls[2];
    const body = JSON.parse(record[1].body);
    expect(body).toEqual({
      title: "Neon Horizon",
      description: "A chase through a rainy megacity.",
      poster_key: grant.object_key,
    });
  });
});

describe("uploadBoxArt", () => {
  it("re-sends title/author/length with the new key", async () => {
    const songGrant = { ...grant, object_key: "songs/s1/box_art/abc" };
    fetchMock
      .mockResolvedValueOnce(json(200, songGrant))
      .mockResolvedValueOnce(new Response(null, { status: 200 }))
      .mockResolvedValueOnce(json(200, { ...song, box_art_key: songGrant.object_key }));

    const updated = await uploadBoxArt(song, file("image/png"));
    expect(updated.box_art_key).toBe(songGrant.object_key);

    const body = JSON.parse(fetchMock.mock.calls[2][1].body);
    expect(body).toEqual({
      title: "Nightcall",
      author: "Kavinsky",
      length_seconds: 195,
      box_art_key: songGrant.object_key,
    });
  });
});

describe("uploadAudioPreview", () => {
  const withFakeAudio = (duration: number) => {
    const element: {
      duration: number;
      onloadedmetadata: (() => void) | null;
      onerror: (() => void) | null;
      src: string;
      preload: string;
    } = { duration, onloadedmetadata: null, onerror: null, src: "", preload: "" };
    Object.defineProperty(element, "src", {
      set() {
        queueMicrotask(() => element.onloadedmetadata?.());
      },
    });
    const created = vi
      .spyOn(document, "createElement")
      .mockImplementation((tag: string) =>
        tag === "audio"
          ? (element as unknown as HTMLElement)
          : document.createElementNS("http://www.w3.org/1999/xhtml", tag) as HTMLElement,
      );
    vi.stubGlobal("URL", {
      ...URL,
      createObjectURL: vi.fn().mockReturnValue("blob:x"),
      revokeObjectURL: vi.fn(),
    });
    return created;
  };

  it("rejects clips over the cap before any network call", async () => {
    withFakeAudio(MAX_PREVIEW_SECONDS + 15);
    await expect(
      uploadAudioPreview(song, file("audio/mpeg")),
    ).rejects.toThrow(/capped at 30s/);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("presigns with the measured duration and records the key", async () => {
    withFakeAudio(12.4);
    const clipGrant = { ...grant, object_key: "songs/s1/preview/abc" };
    fetchMock
      .mockResolvedValueOnce(json(200, clipGrant))
      .mockResolvedValueOnce(new Response(null, { status: 200 }))
      .mockResolvedValueOnce(
        json(200, { ...song, audio_preview_key: clipGrant.object_key }),
      );

    const updated = await uploadAudioPreview(song, file("audio/mpeg"));
    expect(updated.audio_preview_key).toBe(clipGrant.object_key);

    const presign = JSON.parse(fetchMock.mock.calls[0][1].body);
    expect(presign).toEqual({ duration_seconds: 12 });
    const body = JSON.parse(fetchMock.mock.calls[2][1].body);
    expect(body.audio_preview_key).toBe(clipGrant.object_key);
  });
});
