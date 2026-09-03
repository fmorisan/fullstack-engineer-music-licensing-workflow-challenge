// Domain types mirroring the backend wire contracts (licensing-core).

export type Role = "STUDIO" | "LABEL" | "ADMIN";

export type LicenseState =
  | "OFFER"
  | "COUNTER_OFFER"
  | "ACCEPTED"
  | "REJECTED";

export type LicenseAction =
  | "OFFER"
  | "COUNTER_OFFER"
  | "ACCEPT"
  | "REJECT";

export interface User {
  id: string;
  email: string;
  display_name: string;
  role: Role;
  org_id: string | null;
  created_at: string;
}

export interface Movie {
  id: string;
  studio_id: string;
  title: string;
  description: string;
  poster_key: string | null;
  created_at: string;
  updated_at: string;
}

export interface Scene {
  movie_id: string;
  scene_number: number;
  screen_time_seconds: number;
  start_time_seconds: number;
  end_time_seconds: number;
  description: string;
  capture_key: string | null;
}

export interface MovieDetail {
  movie: Movie;
  scenes: Scene[];
}

export interface Song {
  id: string;
  label_id: string;
  title: string;
  author: string;
  length_seconds: number;
  box_art_key: string | null;
  audio_preview_key: string | null;
  created_at: string;
  updated_at: string;
}

export interface SongHit {
  song_id: string;
  label_id: string;
  title: string;
  author: string;
  length_seconds: number;
  box_art_key: string | null;
  audio_preview_key: string | null;
}

export interface License {
  id: string;
  movie_id: string;
  scene_number: number;
  song_id: string;
  studio_id: string;
  label_id: string;
  state: LicenseState;
  license_fee_cents: number;
  start_time_seconds: number;
  end_time_seconds: number;
  created_at: string;
  updated_at: string;
}

export interface LicenseLogEntry {
  sequence: number;
  from_state: string | null;
  to_state: string;
  action: string;
  actor_user_id: string;
  license_fee_cents: number;
  created_at: string;
}

export interface LicenseDetail extends License {
  log: LicenseLogEntry[];
}

export type NotificationType =
  | "OFFER_RECEIVED"
  | "COUNTER_OFFER_RECEIVED"
  | "OFFER_ACCEPTED"
  | "OFFER_REJECTED";

export interface Notification {
  id: string;
  recipient_user_id: string | null;
  recipient_org_id: string | null;
  notification_type: NotificationType;
  payload: {
    event_id: string;
    kind: string;
    license: {
      license_id: string;
      movie_id: string;
      scene_number: number;
      song_id: string;
      state: LicenseState;
      license_fee_cents: number;
      studio_id: string;
      label_id: string;
    };
  };
  read_at: string | null;
  created_at: string;
}

export const formatFee = (cents: number): string => {
  const sign = cents < 0 ? "-" : "";
  const abs = Math.abs(cents);
  return `${sign}$${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
};

export const formatDuration = (seconds: number): string => {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m}:${String(s).padStart(2, "0")}`;
};

export const STATE_LABELS: Record<LicenseState, string> = {
  OFFER: "Offer out",
  COUNTER_OFFER: "Countered",
  ACCEPTED: "Accepted",
  REJECTED: "Rejected",
};
