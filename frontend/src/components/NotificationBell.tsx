// Notification bell: unread badge, dropdown inbox with mark-read, live via
// the notifications SSE stream. Org-scoped server-side; the badge and list
// are cache-driven so SSE frames and manual actions converge.

import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { notifications } from "../api/endpoints";
import { useAuth } from "../auth/AuthContext";
import { useMovieTitles } from "../hooks/useMovieTitles";
import { useSongTitles } from "../hooks/useSongTitles";
import { useNotificationStream } from "../hooks/useLiveStreams";
import type { NotificationType } from "../api/types";

const TYPE_TEXT: Record<NotificationType, string> = {
  OFFER_RECEIVED: "offer received",
  COUNTER_OFFER_RECEIVED: "counter-offer received",
  OFFER_ACCEPTED: "offer accepted",
  OFFER_REJECTED: "offer rejected",
};

export const NotificationBell = () => {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);
  const queryClient = useQueryClient();
  const { user } = useAuth();

  const { connected } = useNotificationStream();

  const unread = useQuery({
    queryKey: ["notifications", "count"],
    queryFn: notifications.unreadCount,
    refetchInterval: connected ? false : 30_000,
  });

  const inbox = useQuery({
    queryKey: ["notifications", "list"],
    queryFn: () => notifications.list({ limit: 20 }),
    enabled: open,
  });

  // Enrich rows with the names the decision is about; every notification
  // implies a movie the recipient can read (owner studio or licensed label).
  const movieTitles = useMovieTitles(
    (inbox.data ?? []).map((n) => n.payload.license.movie_id),
  );
  const songTitles = useSongTitles(
    (inbox.data ?? []).map((n) => n.payload.license.song_id),
  );

  const markRead = useMutation({
    mutationFn: notifications.markRead,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
  });

  const markAll = useMutation({
    mutationFn: notifications.markAllRead,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
  });

  useEffect(() => {
    const onClick = (event: MouseEvent) => {
      if (open && wrapRef.current && !wrapRef.current.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener("mousedown", onClick);
    return () => document.removeEventListener("mousedown", onClick);
  }, [open]);

  const count = unread.data ?? 0;

  return (
    <div className="bell-wrap" ref={wrapRef}>
      <button
        type="button"
        className="bell-button"
        aria-label={`Notifications${count > 0 ? `, ${count} unread` : ""}`}
        onClick={() => setOpen(!open)}
      >
        🔔
        {count > 0 && <span className="bell-badge">{count > 99 ? "99+" : count}</span>}
      </button>

      {open && (
        <div className="bell-menu">
          <header>
            <strong>
              Inbox{" "}
              <span className={`badge live ${connected ? "" : "off"}`} style={connected ? undefined : { opacity: 0.3 }}>
                {connected ? "live" : "…"}
              </span>
            </strong>
            {count > 0 && (
              <button type="button" className="small ghost" onClick={() => markAll.mutate()}>
                Mark all read
              </button>
            )}
          </header>

          {inbox.isPending && <div className="bell-empty">Loading…</div>}
          {inbox.data?.length === 0 && (
            <div className="bell-empty">Nothing yet — offers and decisions land here live.</div>
          )}

          {inbox.data?.map((notification) => {
            const { license } = notification.payload;
            // Labels decide in the movie context; studios on the movie page.
            const destination =
              user?.role === "LABEL"
                ? `/movies/${license.movie_id}/context`
                : `/movies/${license.movie_id}`;
            return (
              <Link
                key={notification.id}
                to={destination}
                className={`notif ${notification.read_at ? "read" : "unread"}`}
                onClick={() => {
                  if (!notification.read_at) markRead.mutate(notification.id);
                  setOpen(false);
                }}
              >
                <span className="dot" />
                <span className="body">
                  <span className={`type ${notification.notification_type}`}>
                    {TYPE_TEXT[notification.notification_type]}
                  </span>{" "}
                  for {formatFee(license.license_fee_cents)}
                  <br />
                  <span className="what">
                    “{songTitles.get(license.song_id) ?? "…"}” · scene{" "}
                    {license.scene_number} ·{" "}
                    {movieTitles.get(license.movie_id) ?? "…"}
                  </span>
                  <br />
                  <span className="when">
                    {new Date(notification.created_at).toLocaleString()}
                  </span>
                </span>
              </Link>
            );
          })}
        </div>
      )}
    </div>
  );
};

const formatFee = (cents: number): string => {
  const abs = Math.abs(cents);
  return `$${Math.floor(abs / 100)}.${String(abs % 100).padStart(2, "0")}`;
};
