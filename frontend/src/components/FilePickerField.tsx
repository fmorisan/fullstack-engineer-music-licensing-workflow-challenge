// Controlled file picker for creation forms: button + selected-file chip
// (image thumbnails via object URLs, audio by name), with pick-time
// validation hooks and a clear action.

import { useEffect, useRef, useState } from "react";
interface FilePickerFieldProps {
  label: string;
  accept: string;
  /** "image" renders an object-URL thumbnail; "audio" shows the file name. */
  variant: "image" | "audio";
  file: File | null;
  onSelect: (file: File | null) => void;
  /** Pick-time validation; return an error message (sync or async) or null. */
  validate?: (file: File) => string | null | Promise<string | null>;
  disabled?: boolean;
}

export const FilePickerField = ({
  label,
  accept,
  variant,
  file,
  onSelect,
  validate,
  disabled,
}: FilePickerFieldProps) => {
  const input = useRef<HTMLInputElement>(null);
  const [error, setError] = useState<string | null>(null);
  const [thumbUrl, setThumbUrl] = useState<string | null>(null);

  // Release the object URL when it is replaced or the picker unmounts.
  useEffect(
    () => () => {
      if (thumbUrl) URL.revokeObjectURL(thumbUrl);
    },
    [thumbUrl],
  );

  const handle = async (picked: File | null) => {
    setError(null);
    if (picked && validate) {
      const message = await validate(picked);
      if (message) {
        setError(message);
        return; // keep the previous selection
      }
    }
    if (thumbUrl) URL.revokeObjectURL(thumbUrl);
    setThumbUrl(picked && variant === "image" ? URL.createObjectURL(picked) : null);
    onSelect(picked);
  };

  return (
    <div className="file-picker">
      {thumbUrl && <img className="media-thumb picker" src={thumbUrl} alt="" />}
      <div className="file-picker-row">
        <button
          type="button"
          className="ghost"
          disabled={disabled}
          onClick={() => input.current?.click()}
        >
          {label}
        </button>
        {file && (
          <>
            {variant === "audio" && (
              <span className="meta">{file.name}</span>
            )}
            <button
              type="button"
              className="ghost"
              disabled={disabled}
              onClick={() => void handle(null)}
              aria-label="clear selection"
            >
              ✕
            </button>
          </>
        )}
      </div>
      {error && <span className="form-error">{error}</span>}
      <input
        hidden
        type="file"
        accept={accept}
        ref={input}
        onChange={(event) => {
          void handle(event.target.files?.[0] ?? null);
          event.target.value = ""; // allow re-picking the same file
        }}
      />
    </div>
  );
};
