// Draggable playback window for the offer modal: two handles on the
// scene track (0..scene screen time) pick the license window, with the
// scene's other licensed songs rendered behind as state-colored segments —
// NLE-style, so overlap is visible while placing. The song track below
// shows exactly which excerpt of the track that window buys (the song
// plays from its start for the window's duration).

import { useRef } from "react";
import { formatDuration, STATE_LABELS, type LicenseState } from "../api/types";

/** A song already licensed into this scene (scene-relative window). */
export interface ExistingUse {
  start: number;
  end: number;
  state: LicenseState;
  label: string;
}

interface Props {
  /** Scene screen time in seconds (window ceiling). */
  maxWindow: number;
  /** Full song length in seconds. */
  songLength: number;
  start: number;
  end: number;
  existing?: ExistingUse[];
  onChange: (start: number, end: number) => void;
}

const clamp = (value: number, min: number, max: number): number =>
  Math.min(max, Math.max(min, value));

export const WindowSelector = ({
  maxWindow,
  songLength,
  start,
  end,
  existing,
  onChange,
}: Props) => {
  const track = useRef<HTMLDivElement>(null);

  const secondsAt = (clientX: number): number => {
    const rect = track.current?.getBoundingClientRect();
    if (!rect || rect.width === 0) return start;
    const ratio = clamp((clientX - rect.left) / rect.width, 0, 1);
    return Math.round(ratio * maxWindow);
  };

  const beginDrag = (which: "start" | "end") => (event: React.PointerEvent) => {
    event.preventDefault();
    const move = (ev: PointerEvent) => {
      const seconds = secondsAt(ev.clientX);
      if (which === "start") {
        onChange(clamp(seconds, 0, end - 1), end);
      } else {
        onChange(start, clamp(seconds, start + 1, maxWindow));
      }
    };
    const up = () => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointerup", up);
    };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", up);
  };

  const left = (value: number): string => `${(value / Math.max(1, maxWindow)) * 100}%`;

  // The window plays the song from its beginning; anything past the window
  // length is unheard.
  const audible = Math.min(end - start, songLength);

  // Rejected/withdrawn negotiations are dead — they don't count as overlap.
  const collisions = (existing ?? []).filter(
    (use) => use.state !== "REJECTED" && use.end > start && use.start < end,
  );

  return (
    <div className="ws">
      <div className="ws-caption meta">scene track — drag the handles</div>
      <div className="ws-track" ref={track} data-testid="ws-track">
        {(existing ?? []).map((use, index) => (
          <span
            key={index}
            className={`ws-existing ${use.state}`}
            style={{ left: left(use.start), width: left(use.end - use.start) }}
            title={`${use.label} · ${STATE_LABELS[use.state]} · ${use.start}s–${use.end}s`}
          />
        ))}
        <div className="ws-window" style={{ left: left(start), width: left(end - start) }} />
        <div
          className="ws-handle"
          role="slider"
          aria-label="window start"
          aria-valuenow={start}
          style={{ left: left(start) }}
          onPointerDown={beginDrag("start")}
        />
        <div
          className="ws-handle"
          role="slider"
          aria-label="window end"
          aria-valuenow={end}
          style={{ left: left(end) }}
          onPointerDown={beginDrag("end")}
        />
      </div>
      {collisions.length > 0 && (
        <span className="ws-overlap">
          overlaps {collisions.map((use) => use.label).join(", ")}
        </span>
      )}

      <div className="ws-caption meta">
        song track — {formatDuration(audible)} of {formatDuration(songLength)} audible
        {end - start >= songLength && " (whole song)"}
      </div>
      <div className="ws-track song">
        <div className="ws-audible" style={{ width: `${(audible / Math.max(1, songLength)) * 100}%` }} />
        <span className="ws-label">0s</span>
        <span className="ws-label">{formatDuration(songLength)}</span>
      </div>
    </div>
  );
};
