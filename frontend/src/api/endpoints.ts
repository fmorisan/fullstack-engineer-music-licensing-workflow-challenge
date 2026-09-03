// Endpoint modules: thin typed wrappers over the api client.

import { api } from "./client";
import type {
  License,
  LicenseDetail,
  Movie,
  MovieDetail,
  Notification,
  Scene,
  Song,
  SongHit,
  User,
} from "./types";

// ── Auth ─────────────────────────────────────────────────────────────────────

export interface Credentials {
  email: string;
  password: string;
}

export const auth = {
  login: (body: Credentials) =>
    api.post<{ user: User; access_token: string }>("/auth/login", body),
  register: (body: Credentials & {
    display_name: string;
    role: string;
    org_name?: string;
  }) => api.post<{ user: User; access_token: string }>("/auth/register", body),
  me: () => api.get<User>("/auth/me"),
  logout: () => api.post<void>("/auth/logout"),
};

// ── Movies (studio) ──────────────────────────────────────────────────────────

export const movies = {
  list: () => api.get<Movie[]>("/movies"),
  create: (body: { title: string; description: string }) =>
    api.post<Movie>("/movies", body),
  detail: (id: string) => api.get<MovieDetail>(`/movies/${id}`),
  addScene: (
    id: string,
    body: { screen_time_seconds: number; start_time_seconds: number; end_time_seconds: number; description: string },
  ) => api.put<Scene>(`/movies/${id}/scenes`, body),
};

// ── Songs (label + search) ───────────────────────────────────────────────────

export const songs = {
  listMine: () => api.get<Song[]>("/songs"),
  create: (body: { title: string; author: string; length_seconds: number }) =>
    api.post<Song>("/songs", body),
  search: (q: string) =>
    api.get<{ total: number; hits: SongHit[] }>(
      `/songs/search?q=${encodeURIComponent(q)}`,
    ),
};

// ── Licenses ─────────────────────────────────────────────────────────────────

export interface CreateLicenseInput {
  movie_id: string;
  scene_number: number;
  song_id: string;
  start_time_seconds: number;
  end_time_seconds: number;
  license_fee_cents: number;
}

export const licenses = {
  create: (body: CreateLicenseInput) => api.post<License>("/licenses", body),
  forMovie: (movieId: string) =>
    api.get<License[]>(`/licenses?movie_id=${movieId}`),
  forLabel: () => api.get<License[]>("/licenses"),
  detail: (id: string) => api.get<LicenseDetail>(`/licenses/${id}`),
  act: (id: string, action: string, license_fee_cents?: number) =>
    api.put<License>(`/licenses/${id}`, { action, license_fee_cents }),
};

// ── Notifications ────────────────────────────────────────────────────────────

export const notifications = {
  list: (params?: { limit?: number; unread?: boolean }) => {
    const query = new URLSearchParams();
    if (params?.limit) query.set("limit", String(params.limit));
    if (params?.unread) query.set("unread", "true");
    const suffix = query.toString() ? `?${query}` : "";
    return api.get<Notification[]>(`/notifications${suffix}`);
  },
  unreadCount: () => api.get<number>("/notifications/unread_count"),
  markRead: (id: string) => api.put<void>(`/notifications/${id}/read`),
  markAllRead: () => api.put<void>("/notifications/read-all"),
};
