// Label's negotiation context for one movie: the full NLE-style timeline
// (every license on the movie, color-coded) plus per-scene boards where the
// label can act on its own rows while seeing competitors read-only. This is
// the view behind accept / counter / reject decisions.

import { useState } from "react";
import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { licenses, movies } from "../api/endpoints";
import { mediaUrl } from "../api/media";
import { useAuth } from "../auth/AuthContext";
import { LicenseBoard } from "../components/LicenseBoard";
import { MovieTimeline } from "../components/MovieTimeline";
import type { License } from "../api/types";

export const LabelMovieContextPage = () => {
  const { movieId = "" } = useParams();
  const { user } = useAuth();
  const [selectedScene, setSelectedScene] = useState<number | null>(null);

  const detail = useQuery({
    queryKey: ["movie", movieId],
    queryFn: () => movies.detail(movieId),
  });
  const context = useQuery({
    queryKey: ["licenses", "movie-context", movieId],
    queryFn: () => licenses.forMovieContext(movieId),
  });

  if (detail.isPending) return <div className="loading">Loading…</div>;
  if (detail.isError || !detail.data) {
    return <p className="form-error">Could not load this movie.</p>;
  }

  const { movie, scenes } = detail.data;
  const rows = context.data ?? [];
  // Rejected negotiations are dead — the count, like the track, is live-only.
  const live = rows.filter((license) => license.state !== "REJECTED").length;

  const byScene = new Map<number, License[]>();
  for (const license of rows) {
    const group = byScene.get(license.scene_number) ?? [];
    group.push(license);
    byScene.set(license.scene_number, group);
  }

  const runtime = scenes.reduce((sum, scene) => sum + scene.screen_time_seconds, 0);

  return (
    <div>
      <div className="page-header">
        <div style={{ display: "flex", gap: 16, alignItems: "flex-start" }}>
          {movie.poster_key && (
            <img
              className="media-thumb poster"
              src={mediaUrl("movie", movie.poster_key)}
              alt={`${movie.title} poster`}
            />
          )}
          <div>
            <h2>{movie.title}</h2>
            <p className="meta" style={{ margin: 0 }}>
              {movie.description || "No description."}
            </p>
            <p className="meta" style={{ margin: "4px 0 0" }}>
              {scenes.length} scene{scenes.length === 1 ? "" : "s"} · {runtime}s runtime ·{" "}
              {live} live license{live === 1 ? "" : "s"} on this movie
            </p>
          </div>
        </div>
      </div>

      {scenes.length > 0 && (
        <MovieTimeline
          scenes={scenes}
          licenses={rows}
          selected={selectedScene}
          onSelectScene={setSelectedScene}
        />
      )}

      <p className="meta" style={{ marginBottom: 12 }}>
        Every license on this movie is shown — your rows carry actions, other
        labels' rows are read-only context.
      </p>

      {scenes.map((scene) => (
        <LicenseBoard
          key={scene.scene_number}
          movieId={movieId}
          scene={scene}
          licenses={byScene.get(scene.scene_number) ?? []}
          perspective="label"
          canAct={(license) => license.label_id === user?.org_id}
        />
      ))}
    </div>
  );
};
