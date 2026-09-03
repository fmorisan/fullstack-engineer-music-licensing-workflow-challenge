// Resolves song titles for license rows. License rows carry only song ids;
// titles come from the authoritative song endpoint (no N+1 — one request
// per distinct id, cached by TanStack Query).

import { useMemo } from "react";
import { useQueries } from "@tanstack/react-query";
import { api } from "../api/client";
import type { Song } from "../api/types";

const songQuery = (songId: string) => ({
  queryKey: ["song", songId],
  queryFn: () => api.get<Song>(`/songs/${songId}`),
  staleTime: 60_000,
});

export const useSongTitles = (songIds: string[]): Map<string, string> => {
  const distinct = useMemo(
    () => [...new Set(songIds)],
    [songIds.join(",")], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const queries = useQueries({ queries: distinct.map(songQuery) });

  return useMemo(() => {
    const map = new Map<string, string>();
    distinct.forEach((id, index) => {
      const song = queries[index].data;
      if (song) map.set(id, `${song.title} — ${song.author}`);
    });
    return map;
  }, [distinct, queries]);
};
