// Studio view: fuzzy song search with as-you-type debouncing, and the
// offer modal that creates a license for a chosen movie/scene.

import { useEffect, useMemo, useState } from "react";
import { useLocation } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { licenses, movies, songs } from "../api/endpoints";
import { ApiError } from "../api/client";
import { WindowSelector } from "../components/WindowSelector";
import { formatDuration, formatFee, type SongHit } from "../api/types";

interface OfferTarget {
  movieId: string;
  sceneNumber: number;
  sceneEndTime: number;
}

export const SearchPage = () => {
  const location = useLocation() as { state: OfferTarget | null };
  const preset = location.state;
  const [query, setQuery] = useState("");
  const [debounced, setDebounced] = useState("");
  const [target, setTarget] = useState<OfferTarget | null>(preset);
  const [offering, setOffering] = useState<SongHit | null>(null);

  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim()), 250);
    return () => clearTimeout(timer);
  }, [query]);

  const search = useQuery({
    queryKey: ["song-search", debounced],
    queryFn: () => songs.search(debounced),
    enabled: debounced.length > 0,
  });

  const myMovies = useQuery({
    queryKey: ["movies"],
    queryFn: movies.list,
    enabled: !preset,
  });

  return (
    <div>
      <div className="page-header">
        <h2>Find music</h2>
        {target && (
          <span className="meta">
            Licensing into scene {target.sceneNumber} ·{" "}
            <button
              type="button"
              className="small ghost"
              onClick={() => setTarget(null)}
            >
              change
            </button>
          </span>
        )}
      </div>

      {!target && myMovies.data && (
        <div className="card" style={{ marginBottom: 16 }}>
          <p className="meta" style={{ margin: 0 }}>
            Pick a movie first — you'll choose the scene when creating the offer.
          </p>
          <div className="row">
            {myMovies.data.map((movie) => (
              <button key={movie.id} type="button" onClick={() => setTarget({
                movieId: movie.id,
                sceneNumber: -1,
                sceneEndTime: 0,
              })}>
                {movie.title}
              </button>
            ))}
          </div>
        </div>
      )}
      {target && target.sceneNumber === -1 && (
        <ScenePicker target={target} onPicked={setTarget} />
      )}

      <input
        style={{ fontSize: 18, padding: "12px 16px", marginBottom: 16 }}
        placeholder="Search the catalog — try “night”, or a typo like “nightcal”…"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        autoFocus
      />

      {debounced && search.isPending && <div className="loading">Searching…</div>}
      {debounced && search.data?.hits.length === 0 && (
        <div className="empty-state">
          <h3>Nothing found for “{debounced}”</h3>
          <p>The catalog hydrates from Kafka within a couple of seconds of publication.</p>
        </div>
      )}

      <div className="grid">
        {search.data?.hits.map((hit) => (
          <div key={hit.song_id} className="card">
            <h3>{hit.title}</h3>
            <p className="meta">{hit.author}</p>
            <p className="meta">{formatDuration(hit.length_seconds)}</p>
            <div className="spacer" />
            <button
              type="button"
              className="primary"
              disabled={!target || target.sceneNumber === -1}
              onClick={() => setOffering(hit)}
            >
              License this song
            </button>
          </div>
        ))}
      </div>

      {offering && target && target.sceneNumber !== -1 && (
        <OfferModal
          song={offering}
          target={target}
          onClose={() => setOffering(null)}
        />
      )}
    </div>
  );
};

const ScenePicker = ({
  target,
  onPicked,
}: {
  target: OfferTarget;
  onPicked: (target: OfferTarget) => void;
}) => {
  const detail = useQuery({
    queryKey: ["movie", target.movieId],
    queryFn: () => movies.detail(target.movieId),
  });
  if (detail.isPending) return null;
  return (
    <div className="card" style={{ marginBottom: 16 }}>
      <p className="meta" style={{ margin: 0 }}>Which scene?</p>
      <div className="row">
        {detail.data?.scenes.map((scene) => (
          <button
            key={scene.scene_number}
            type="button"
            onClick={() =>
              onPicked({
                movieId: target.movieId,
                sceneNumber: scene.scene_number,
                sceneEndTime: scene.screen_time_seconds,
              })
            }
          >
            Scene {scene.scene_number} ({scene.screen_time_seconds}s)
          </button>
        ))}
      </div>
    </div>
  );
};

const OfferModal = ({
  song,
  target,
  onClose,
}: {
  song: SongHit;
  target: OfferTarget;
  onClose: () => void;
}) => {
  const [fee, setFee] = useState("1500");
  const [start, setStart] = useState("0");
  const [end, setEnd] = useState(String(Math.min(30, target.sceneEndTime)));
  const [error, setError] = useState<string | null>(null);

  // The selector owns integer windows; the inputs stay the source of truth
  // for typing exact values.
  const setWindow = (nextStart: number, nextEnd: number) => {
    setStart(String(nextStart));
    setEnd(String(nextEnd));
  };

  const create = useMutation({
    mutationFn: () =>
      licenses.create({
        movie_id: target.movieId,
        scene_number: target.sceneNumber,
        song_id: song.song_id,
        start_time_seconds: Number(start),
        end_time_seconds: Number(end),
        license_fee_cents: Math.round(Number(fee) * 100),
      }),
    onSuccess: onClose,
    onError: (err) =>
      setError(err instanceof ApiError ? err.message : "could not create the offer"),
  });

  const maxWindow = useMemo(() => target.sceneEndTime, [target]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <form
        className="modal"
        onClick={(e) => e.stopPropagation()}
        onSubmit={(e) => {
          e.preventDefault();
          create.mutate();
        }}
      >
        <h3>License “{song.title}”</h3>
        <p className="meta">
          Scene {target.sceneNumber} · playback window within the scene (0–{maxWindow}s)
        </p>

        <WindowSelector
          maxWindow={maxWindow}
          songLength={song.length_seconds}
          start={Number(start)}
          end={Number(end)}
          onChange={setWindow}
        />

        <label>
          Offer fee (USD)
          <div className="fee-input">
            <span>$</span>
            <input
              required
              type="number"
              min="0"
              step="0.01"
              value={fee}
              onChange={(e) => setFee(e.target.value)}
            />
          </div>
        </label>
        <div className="row">
          <label className="grow">
            Start (s)
            <input
              required
              type="number"
              min="0"
              max={maxWindow}
              value={start}
              onChange={(e) => setStart(e.target.value)}
            />
          </label>
          <label className="grow">
            End (s)
            <input
              required
              type="number"
              min="1"
              max={maxWindow}
              value={end}
              onChange={(e) => setEnd(e.target.value)}
            />
          </label>
        </div>
        {error && <p className="error">{error}</p>}
        <p className="hint">Your offer: {formatFee(Math.round(Number(fee || "0") * 100))}</p>

        <div className="row" style={{ justifyContent: "flex-end" }}>
          <button type="button" className="ghost" onClick={onClose}>
            Cancel
          </button>
          <button type="submit" className="primary" disabled={create.isPending}>
            {create.isPending ? "Sending…" : "Send offer"}
          </button>
        </div>
      </form>
    </div>
  );
};
