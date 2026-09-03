// Live-updates hooks: EventSource on the two SSE streams (ADR-005).
//
// Tokens ride the query string (EventSource cannot send headers); both
// endpoints accept `access_token` and filter payloads server-side to the
// caller's org. Consumers reconcile by invalidating TanStack Query caches
// — missed frames (fire-and-forget PubSub) heal on refetch/reconnect.

import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { getAccessToken, sseUrl } from "../api/client";

type EventShape = Record<string, unknown>;

const useEventStream = (path: string, eventName: string, onEvent: (payload: EventShape) => void) => {
  const [connected, setConnected] = useState(false);
  const queryClient = useQueryClient();
  const handlerRef = useRef(onEvent);
  handlerRef.current = onEvent;

  useEffect(() => {
    if (!getAccessToken()) return;
    const source = new EventSource(sseUrl(path));

    source.onopen = () => setConnected(true);
    source.onerror = () => setConnected(false); // EventSource auto-reconnects
    source.addEventListener(eventName, (event) => {
      try {
        handlerRef.current(JSON.parse((event as MessageEvent).data));
      } catch {
        // Malformed frame: ignore; refetch-on-reconnect heals.
      }
    });

    return () => {
      source.close();
      setConnected(false);
    };
    // Reconnect when the access token rotates (login/logout).
  }, [path, eventName, getAccessToken()]);

  return { connected, queryClient };
};

export const useLicenseStream = (onUpdate?: (snapshot: EventShape) => void) => {
  const queryClient = useQueryClient();
  return useEventStream("/licenses/stream", "license-updated", (snapshot) => {
    // Invalidate every license-shaped cache; org filtering happened
    // server-side, so a delivered snapshot always concerns this org.
    void queryClient.invalidateQueries({ queryKey: ["licenses"] });
    void queryClient.invalidateQueries({ queryKey: ["movie"] });
    onUpdate?.(snapshot);
  });
};

export const useNotificationStream = (onNotification?: (dto: EventShape) => void) => {
  const queryClient = useQueryClient();
  return useEventStream("/notifications/stream", "notification", (dto) => {
    void queryClient.invalidateQueries({ queryKey: ["notifications"] });
    onNotification?.(dto);
  });
};
