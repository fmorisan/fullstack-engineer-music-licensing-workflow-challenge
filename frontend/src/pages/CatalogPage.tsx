// Label view: the song catalog with inline publication.

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { songs } from "../api/endpoints";
import { ApiError } from "../api/client";
import { formatDuration, type Song } from "../api/types";

export const CatalogPage = () => {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [author, setAuthor] = useState("");
  const [length, setLength] = useState("180");
  const [error, setError] = useState<string | null>(null);

  const list = useQuery({ queryKey: ["songs"], queryFn: songs.listMine });

  const create = useMutation({
    mutationFn: () =>
      songs.create({
        title,
        author,
        length_seconds: Number(length),
      }),
    onSuccess: () => {
      setTitle("");
      setAuthor("");
      setError(null);
      queryClient.invalidateQueries({ queryKey: ["songs"] });
    },
    onError: (err) =>
      setError(err instanceof ApiError ? err.message : "could not publish the song"),
  });

  return (
    <div>
      <div className="page-header">
        <h2>Catalog</h2>
        <span className="meta">
          Published songs become searchable by studios within seconds (Kafka → ElasticSearch).
        </span>
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
          <input required value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Nightcall" />
        </label>
        <label className="grow">
          Author
          <input required value={author} onChange={(e) => setAuthor(e.target.value)} placeholder="Kavinsky" />
        </label>
        <label style={{ width: 120 }}>
          Length (s)
          <input required type="number" min="1" value={length} onChange={(e) => setLength(e.target.value)} />
        </label>
        <button type="submit" className="primary" disabled={create.isPending}>
          {create.isPending ? "Publishing…" : "Publish song"}
        </button>
      </form>
      {error && <p className="form-error">{error}</p>}

      {list.isPending && <div className="loading">Loading…</div>}
      {list.data && list.data.length === 0 && (
        <div className="empty-state">
          <h3>No songs yet</h3>
          <p>Publish your first track above — studios find it through search.</p>
        </div>
      )}

      <div className="grid">
        {list.data?.map((song: Song) => (
          <div key={song.id} className="card">
            <h3>{song.title}</h3>
            <p className="meta">{song.author}</p>
            <p className="meta">{formatDuration(song.length_seconds)}</p>
            <div className="spacer" />
            <span className="meta">Published {new Date(song.created_at).toLocaleDateString()}</span>
          </div>
        ))}
      </div>
    </div>
  );
};
