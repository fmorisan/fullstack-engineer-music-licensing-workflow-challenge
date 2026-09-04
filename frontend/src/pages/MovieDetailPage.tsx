// Movie detail: scene strip with a timeline, add-scene form, and the
// license board per scene. Offer creation links into Find Music.

import { useRef, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { licenses, movies } from "../api/endpoints";
import { mediaUrl, uploadMoviePoster, uploadSceneCapture } from "../api/media";
import { ApiError } from "../api/client";
import { LicenseBoard } from "../components/LicenseBoard";
import { MediaUpload } from "../components/MediaUpload";
import { MovieTimeline } from "../components/MovieTimeline";
import type { License, MovieDetail, Scene } from "../api/types";

const SceneChip = ({
  scene,
  movie,
  selected,
  onUploaded,
}: {
  scene: Scene;
  movie: MovieDetail;
  selected: boolean;
  onUploaded: () => void;
}) => {
  const offset = (scene.start_time_seconds / Math.max(1, totalRuntime(movie))) * 100;
  const width = (scene.screen_time_seconds / Math.max(1, totalRuntime(movie))) * 100;
  return (
    <div className={`scene-chip${selected ? " selected" : ""}`}>
      <strong>Scene {scene.scene_number}</strong>
      <div className="timeline" title="position within the film">
        <div style={{ marginLeft: `${offset}%`, width: `${width}%` }} />
      </div>
      <span className="meta">{scene.description || "No description."}</span>
      <span className="meta">
        film {scene.start_time_seconds}s → {scene.end_time_seconds}s ({scene.screen_time_seconds}s)
      </span>
      <MediaUpload
        bucket="movie"
        variant="image"
        label="Add capture"
        mediaKey={scene.capture_key}
        onUpload={(file) => uploadSceneCapture(movie.movie.id, scene, file)}
        onDone={onUploaded}
      />
    </div>
  );
};

const totalRuntime = (movie: MovieDetail): number =>
  movie.scenes.reduce((sum, scene) => sum + scene.screen_time_seconds, 0);

export const MovieDetailPage = () => {
  const { movieId = "" } = useParams();
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [screenTime, setScreenTime] = useState("60");
  const [description, setDescription] = useState("");
  const [selectedScene, setSelectedScene] = useState<number | null>(null);
  const boards = useRef(new Map<number, HTMLDivElement>());

  const focusScene = (sceneNumber: number) => {
    setSelectedScene(sceneNumber);
    boards.current.get(sceneNumber)?.scrollIntoView({ behavior: "smooth", block: "center" });
  };

  const detail = useQuery({
    queryKey: ["movie", movieId],
    queryFn: () => movies.detail(movieId),
  });
  const board = useQuery({
    queryKey: ["licenses", "movie", movieId],
    queryFn: () => licenses.forMovie(movieId),
  });

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ["movie", movieId] });

  const addScene = useMutation({
    mutationFn: () => {
      const start = (detail.data?.scenes ?? []).reduce(
        (sum, scene) => sum + scene.screen_time_seconds,
        0,
      );
      const screen = Number(screenTime);
      return movies.addScene(movieId, {
        screen_time_seconds: screen,
        start_time_seconds: start,
        end_time_seconds: start + screen,
        description,
      });
    },
    onSuccess: () => {
      setDescription("");
      setError(null);
      queryClient.invalidateQueries({ queryKey: ["movie", movieId] });
    },
    onError: (err) =>
      setError(err instanceof ApiError ? err.message : "could not add the scene"),
  });

  if (detail.isPending) return <div className="loading">Loading…</div>;
  if (detail.isError || !detail.data) {
    return <p className="form-error">Could not load this movie.</p>;
  }

  const { movie, scenes } = detail.data;
  const licensesByScene = new Map<number, License[]>();
  for (const license of board.data ?? []) {
    const list = licensesByScene.get(license.scene_number) ?? [];
    list.push(license);
    licensesByScene.set(license.scene_number, list);
  }

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
              {scenes.length} scene{scenes.length === 1 ? "" : "s"} · {totalRuntime(detail.data)}s runtime
            </p>
            <div style={{ marginTop: 8 }}>
              <MediaUpload
                bucket="movie"
                variant="image"
                label="Upload poster"
                mediaKey={movie.poster_key}
                onUpload={(file) => uploadMoviePoster(movie, file)}
                onDone={() => void invalidate()}
              />
            </div>
          </div>
        </div>
        <div className="actions">
          <Link
            to="/search"
            state={{
              movieId,
              sceneNumber: scenes[0]?.scene_number,
              sceneEndTime: scenes[0]?.screen_time_seconds,
            }}
          >
            <button className="primary" type="button">Find music</button>
          </Link>
        </div>
      </div>

      {scenes.length > 0 && (
        <>
          <MovieTimeline
            scenes={scenes}
            licenses={board.data ?? []}
            selected={selectedScene}
            onSelectScene={focusScene}
          />
          <div className="scene-strip" style={{ marginBottom: 20 }}>
            {scenes.map((scene) => (
              <SceneChip
                key={scene.scene_number}
                scene={scene}
                movie={detail.data!}
                selected={selectedScene === scene.scene_number}
                onUploaded={() => void invalidate()}
              />
            ))}
          </div>
        </>
      )}

      <form
        className="card"
        style={{ marginBottom: 24, flexDirection: "row", alignItems: "end", gap: 12 }}
        onSubmit={(e) => {
          e.preventDefault();
          addScene.mutate();
        }}
      >
        <label style={{ width: 120 }}>
          Screen time (s)
          <input
            type="number"
            min="1"
            required
            value={screenTime}
            onChange={(e) => setScreenTime(e.target.value)}
          />
        </label>
        <label className="grow">
          Description
          <input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="The pursuit begins"
          />
        </label>
        <button type="submit" className="primary" disabled={addScene.isPending}>
          {addScene.isPending ? "Adding…" : "Add scene"}
        </button>
      </form>
      {error && <p className="form-error">{error}</p>}

      {scenes.map((scene) => (
        <div
          key={scene.scene_number}
          ref={(element) => {
            if (element) boards.current.set(scene.scene_number, element);
            else boards.current.delete(scene.scene_number);
          }}
          className={selectedScene === scene.scene_number ? "board-focus" : undefined}
        >
          <LicenseBoard
            movieId={movieId}
            scene={scene}
            licenses={licensesByScene.get(scene.scene_number) ?? []}
            perspective="studio"
          />
        </div>
      ))}
    </div>
  );
};
