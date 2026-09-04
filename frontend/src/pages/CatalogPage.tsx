// Label view: the song catalog with inline publication.

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { songs } from "../api/endpoints";
import {
  MAX_PREVIEW_SECONDS,
  readAudioDuration,
  uploadAudioPreview,
  uploadBoxArt,
} from "../api/media";
import { ApiError } from "../api/client";
import { FilePickerField } from "../components/FilePickerField";
import { MediaUpload } from "../components/MediaUpload";
import { formatDuration, type Song } from "../api/types";

export const CatalogPage = () => {
  const queryClient = useQueryClient();
  const [title, setTitle] = useState("");
  const [author, setAuthor] = useState("");
  const [length, setLength] = useState("180");
  const [boxArt, setBoxArt] = useState<File | null>(null);
  const [preview, setPreview] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);

  const list = useQuery({ queryKey: ["songs"], queryFn: songs.listMine });

  // Reject over-cap clips the moment they are picked — before the song
  // (and the bytes) ever leave the browser.
  const validatePreview = async (file: File): Promise<string | null> => {
    try {
      const duration = await readAudioDuration(file);
      return duration > MAX_PREVIEW_SECONDS
        ? `preview clips are capped at ${MAX_PREVIEW_SECONDS}s (this one is ${Math.round(duration)}s)`
        : null;
    } catch {
      return "could not read the audio file";
    }
  };

  // Same create-then-upload chain as movies: each media failure downgrades
  // to a warning; the song itself is safely published either way.
  const create = useMutation({
    mutationFn: async (): Promise<string | null> => {
      const created = await songs.create({
        title,
        author,
        length_seconds: Number(length),
      });
      const warnings: string[] = [];
      if (boxArt) {
        try {
          await uploadBoxArt(created, boxArt);
        } catch (err) {
          warnings.push(`box art: ${err instanceof Error ? err.message : "unknown error"}`);
        }
      }
      if (preview) {
        try {
          await uploadAudioPreview(created, preview);
        } catch (err) {
          warnings.push(`preview: ${err instanceof Error ? err.message : "unknown error"}`);
        }
      }
      return warnings.length
        ? `Song published, but some uploads failed — retry on its card (${warnings.join("; ")})`
        : null;
    },
    onSuccess: (warning) => {
      setTitle("");
      setAuthor("");
      setBoxArt(null);
      setPreview(null);
      setError(warning);
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
        <FilePickerField
          label="Box art"
          accept="image/*"
          variant="image"
          file={boxArt}
          onSelect={setBoxArt}
          disabled={create.isPending}
        />
        <FilePickerField
          label={`Preview (≤${MAX_PREVIEW_SECONDS}s)`}
          accept="audio/*"
          variant="audio"
          file={preview}
          onSelect={setPreview}
          validate={validatePreview}
          disabled={create.isPending}
        />
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
            <div style={{ display: "flex", flexDirection: "column", gap: 8, marginTop: 8 }}>
              <MediaUpload
                bucket="song"
                variant="image"
                label="Upload box art"
                mediaKey={song.box_art_key}
                onUpload={(file) => uploadBoxArt(song, file)}
                onDone={() => void queryClient.invalidateQueries({ queryKey: ["songs"] })}
              />
              <MediaUpload
                bucket="song"
                variant="audio"
                label="Upload preview (≤30s)"
                mediaKey={song.audio_preview_key}
                onUpload={(file) => uploadAudioPreview(song, file)}
                onDone={() => void queryClient.invalidateQueries({ queryKey: ["songs"] })}
              />
            </div>
            <div className="spacer" />
            <span className="meta">Published {new Date(song.created_at).toLocaleDateString()}</span>
          </div>
        ))}
      </div>
    </div>
  );
};
