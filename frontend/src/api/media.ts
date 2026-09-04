// Pre-signed media uploads (ADR-009): presign through the gateway, PUT the
// bytes straight to object storage, then record the object key on the
// entity through its regular update endpoint.
//
// The presign endpoints take a required (possibly empty) JSON body and sign
// the content type only when one is provided — we deliberately presign
// without it so the browser can send the file's own type unsigned.

import { api } from "./client";
import type { Movie, Scene, Song } from "./types";

const MEDIA_BASE: string =
  import.meta.env.VITE_MEDIA_URL ?? "http://localhost:9000";

const BUCKET_MOVIE = "movie-media";
const BUCKET_SONG = "song-media";

interface PresignedUpload {
  upload_url: string;
  object_key: string;
  expires_in: number;
}

/** Public URL for a stored object (buckets are public-read locally). */
export const mediaUrl = (bucket: "movie" | "song", objectKey: string): string =>
  `${MEDIA_BASE}/${bucket === "movie" ? BUCKET_MOVIE : BUCKET_SONG}/${objectKey}`;

const presign = (path: string, body: Record<string, unknown> = {}) =>
  api.put<PresignedUpload>(path, body);

/** PUT raw bytes to a pre-signed URL — no Authorization header (the URL IS
 * the authorization). */
const putBytes = async (uploadUrl: string, file: File): Promise<void> => {
  const response = await fetch(uploadUrl, {
    method: "PUT",
    body: file,
    headers: { "content-type": file.type || "application/octet-stream" },
  });
  if (!response.ok) {
    throw new Error(`upload failed (${response.status})`);
  }
};

// ── Movies ───────────────────────────────────────────────────────────────────
// `PUT /movies/:id` overwrites title/description (no COALESCE), so the
// current values ride along; `poster_key` itself COALESCEs.

export const uploadMoviePoster = async (
  movie: Movie,
  file: File,
): Promise<Movie> => {
  const grant = await presign(`/movies/${movie.id}/poster`);
  await putBytes(grant.upload_url, file);
  return api.put<Movie>(`/movies/${movie.id}`, {
    title: movie.title,
    description: movie.description,
    poster_key: grant.object_key,
  });
};

export const uploadSceneCapture = async (
  movieId: string,
  scene: Scene,
  file: File,
): Promise<Scene> => {
  const grant = await presign(
    `/movies/${movieId}/scenes/${scene.scene_number}/capture`,
  );
  await putBytes(grant.upload_url, file);
  return api.put<Scene>(`/movies/${movieId}/scenes/${scene.scene_number}`, {
    screen_time_seconds: scene.screen_time_seconds,
    start_time_seconds: scene.start_time_seconds,
    end_time_seconds: scene.end_time_seconds,
    description: scene.description,
    capture_key: grant.object_key,
  });
};

// ── Songs ────────────────────────────────────────────────────────────────────
// Same story: title/author/length overwrite, the two media keys COALESCE.

export const uploadBoxArt = async (
  song: Song,
  file: File,
): Promise<Song> => {
  const grant = await presign(`/songs/${song.id}/box_art`);
  await putBytes(grant.upload_url, file);
  return api.put<Song>(`/songs/${song.id}`, {
    title: song.title,
    author: song.author,
    length_seconds: song.length_seconds,
    box_art_key: grant.object_key,
  });
};

export const MAX_PREVIEW_SECONDS = 30;

/** Reads a clip's duration from its metadata without uploading. */
export const readAudioDuration = (file: File): Promise<number> =>
  new Promise((resolve, reject) => {
    const element = document.createElement("audio");
    element.preload = "metadata";
    const url = URL.createObjectURL(file);
    const cleanup = () => URL.revokeObjectURL(url);
    element.onloadedmetadata = () => {
      cleanup();
      resolve(element.duration);
    };
    element.onerror = () => {
      cleanup();
      reject(new Error("could not read the audio file"));
    };
    element.src = url;
  });

export const uploadAudioPreview = async (
  song: Song,
  file: File,
): Promise<Song> => {
  const duration = await readAudioDuration(file);
  if (duration > MAX_PREVIEW_SECONDS) {
    throw new Error(
      `preview clips are capped at ${MAX_PREVIEW_SECONDS}s (this one is ${Math.round(duration)}s)`,
    );
  }
  const grant = await presign(`/songs/${song.id}/audio_preview`, {
    duration_seconds: Math.round(duration),
  });
  await putBytes(grant.upload_url, file);
  return api.put<Song>(`/songs/${song.id}`, {
    title: song.title,
    author: song.author,
    length_seconds: song.length_seconds,
    audio_preview_key: grant.object_key,
  });
};
