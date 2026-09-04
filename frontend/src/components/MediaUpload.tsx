// Reusable pre-signed upload control: pick a file, run the
// presign → PUT → record pipeline, and preview the stored object
// (image thumbnail or audio player) once a key exists.

import { useRef, useState } from "react";
import { mediaUrl } from "../api/media";

interface MediaUploadProps {
  bucket: "movie" | "song";
  variant: "image" | "audio";
  /** Action verb on the button, e.g. "Upload poster". */
  label: string;
  mediaKey: string | null;
  /** Runs the full pipeline; resolves with the updated entity. */
  onUpload: (file: File) => Promise<unknown>;
  onDone?: () => void;
}

export const MediaUpload = ({
  bucket,
  variant,
  label,
  mediaKey,
  onUpload,
  onDone,
}: MediaUploadProps) => {
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handle = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    setError(null);
    try {
      await onUpload(file);
      onDone?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : "upload failed");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="media-upload">
      {mediaKey && variant === "image" && (
        <img
          className="media-thumb"
          src={mediaUrl(bucket, mediaKey)}
          alt={label}
        />
      )}
      {mediaKey && variant === "audio" && (
        <audio controls preload="none" src={mediaUrl(bucket, mediaKey)} />
      )}
      <button
        type="button"
        className="ghost"
        disabled={busy}
        onClick={() => input.current?.click()}
      >
        {busy ? "Uploading…" : mediaKey ? "Replace" : label}
      </button>
      {error && <span className="form-error">{error}</span>}
      <input
        hidden
        type="file"
        accept={variant === "image" ? "image/*" : "audio/*"}
        ref={input}
        onChange={(event) => {
          void handle(event.target.files?.[0]);
          event.target.value = ""; // allow re-picking the same file
        }}
      />
    </div>
  );
};
