// Studio view: movies with their scenes and license board per scene.

import { useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { movies } from "../api/endpoints";
import { mediaUrl, uploadMoviePoster } from "../api/media";
import { ApiError } from "../api/client";
import { FilePickerField } from "../components/FilePickerField";
import type { Movie } from "../api/types";

export const MoviesPage = () => {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [poster, setPoster] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);

  const list = useQuery({ queryKey: ["movies"], queryFn: movies.list });

  // Presigning needs an existing owner, so creation runs first and any
  // picked poster rides the standard pipeline right after; an upload
  // failure downgrades to a warning (the movie exists, "Replace" retries).
  const create = useMutation({
    mutationFn: async (): Promise<string | null> => {
      const created = await movies.create({ title, description });
      if (!poster) return null;
      try {
        await uploadMoviePoster(created, poster);
        return null;
      } catch (err) {
        return `Movie created, but the poster upload failed: ${
          err instanceof Error ? err.message : "unknown error"
        }`;
      }
    },
    onSuccess: (warning) => {
      setTitle("");
      setDescription("");
      setPoster(null);
      setError(warning);
      queryClient.invalidateQueries({ queryKey: ["movies"] });
    },
    onError: (err) =>
      setError(err instanceof ApiError ? err.message : "could not create the movie"),
  });

  return (
    <div>
      <div className="page-header">
        <h2>Movies</h2>
      </div>

      <form
        className="card"
        style={{ marginBottom: 20, flexDirection: "row", alignItems: "end", gap: 12 }}
        onSubmit={(e) => {
          e.preventDefault();
          create.mutate();
        }}
      >
        <label className="grow">
          Title
          <input
            required
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Neon Pursuit"
          />
        </label>
        <label className="grow">
          Description
          <input
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="A rain-soaked chase through a neon city."
          />
        </label>
        <FilePickerField
          label="Poster"
          accept="image/*"
          variant="image"
          file={poster}
          onSelect={setPoster}
          disabled={create.isPending}
        />
        <button type="submit" className="primary" disabled={create.isPending || !title.trim()}>
          {create.isPending ? "Adding…" : "Add movie"}
        </button>
      </form>
      {error && <p className="form-error">{error}</p>}

      {list.isPending && <div className="loading">Loading…</div>}
      {list.isError && <p className="form-error">Could not load movies.</p>}
      {list.data && list.data.length === 0 && (
        <div className="empty-state">
          <h3>No movies yet</h3>
          <p>Create your first production above.</p>
        </div>
      )}

      <div className="grid">
        {list.data?.map((movie: Movie) => (
          <Link key={movie.id} to={`/movies/${movie.id}`} className="card" style={{ color: "inherit" }}>
            {movie.poster_key && (
              <img
                className="media-thumb"
                src={mediaUrl("movie", movie.poster_key)}
                alt={`${movie.title} poster`}
              />
            )}
            <h3>{movie.title}</h3>
            <p className="meta">{movie.description || "No description."}</p>
            <div className="spacer" />
            <span className="meta">
              {new Date(movie.created_at).toLocaleDateString()}
            </span>
          </Link>
        ))}
      </div>
    </div>
  );
};
