// Resolves movie titles for notification rows. Notifications carry movie
// ids; titles come from the movie endpoint (one request per distinct id,
// cached by TanStack Query — and the key is shared with the detail pages,
// so opening one pre-warms the other).

import { useMemo } from "react";
import { useQueries } from "@tanstack/react-query";
import { movies } from "../api/endpoints";

export const useMovieTitles = (movieIds: string[]): Map<string, string> => {
  const distinct = useMemo(
    () => [...new Set(movieIds)],
    [movieIds.join(",")], // eslint-disable-line react-hooks/exhaustive-deps
  );
  const queries = useQueries({
    queries: distinct.map((movieId) => ({
      queryKey: ["movie", movieId],
      queryFn: () => movies.detail(movieId),
      staleTime: 60_000,
    })),
  });

  return useMemo(() => {
    const map = new Map<string, string>();
    distinct.forEach((id, index) => {
      const detail = queries[index].data;
      if (detail) map.set(id, detail.movie.title);
    });
    return map;
  }, [distinct, queries]);
};
