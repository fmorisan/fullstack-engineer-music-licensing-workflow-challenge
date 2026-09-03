// Studio view: movies with their scenes and license board per scene.

import { useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { movies } from "../api/endpoints";
import { ApiError } from "../api/client";
import type { Movie } from "../api/types";

export const MoviesPage = () => {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [error, setError] = useState<string | null>(null);

  const list = useQuery({ queryKey: ["movies"], queryFn: movies.list });

  const create = useMutation({
    mutationFn: () => movies.create({ title, description }),
    onSuccess: () => {
      setTitle("");
      setDescription("");
      setError(null);
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
