// NLE-style movie timeline: a film ruler, scene blocks on the video lane
// (capture thumbnails when present), and license segments on the music
// lane color-coded by negotiation state. Clicking a scene selects it and
// focuses its license board below.

import { useSongTitles } from "../hooks/useSongTitles";
import { mediaUrl } from "../api/media";
import { formatFee, STATE_LABELS, type License, type LicenseState, type Scene } from "../api/types";

interface Props {
  scenes: Scene[];
  licenses: License[];
  selected: number | null;
  onSelectScene: (sceneNumber: number) => void;
}

const tickStep = (total: number): number => {
  if (total <= 120) return 10;
  if (total <= 600) return 30;
  return 60;
};

const pct = (value: number, total: number): string => `${(value / Math.max(1, total)) * 100}%`;

export const MovieTimeline = ({ scenes, licenses, selected, onSelectScene }: Props) => {
  const total = scenes.reduce((sum, scene) => sum + scene.screen_time_seconds, 0);
  const step = tickStep(total);
  const ticks: number[] = [];
  for (let t = step; t < total; t += step) ticks.push(t);

  const sceneByNumber = new Map(scenes.map((scene) => [scene.scene_number, scene]));
  const titles = useSongTitles(licenses.map((license) => license.song_id));

  return (
    <div className="tl" style={{ marginBottom: 20 }}>
      {/* Ruler */}
      <div className="tl-ruler">
        {ticks.map((tick) => (
          <span key={tick} style={{ left: pct(tick, total) }}>
            {tick}s
          </span>
        ))}
        <span style={{ left: "100%" }}>{total}s</span>
      </div>

      {/* Scenes (video lane) */}
      <div className="tl-lane" role="listbox" aria-label="scenes">
        {scenes.map((scene) => (
          <button
            key={scene.scene_number}
            type="button"
            role="option"
            aria-selected={selected === scene.scene_number}
            className={`tl-scene${selected === scene.scene_number ? " selected" : ""}`}
            style={{
              left: pct(scene.start_time_seconds, total),
              width: pct(scene.screen_time_seconds, total),
              ...(scene.capture_key && {
                backgroundImage: `url(${mediaUrl("movie", scene.capture_key)})`,
              }),
            }}
            title={`Scene ${scene.scene_number}: ${scene.description || "no description"}`}
            onClick={() => onSelectScene(scene.scene_number)}
          >
            S{scene.scene_number}
          </button>
        ))}
      </div>

      {/* Licenses (music lane) */}
      <div className="tl-lane licenses" aria-label="licenses">
        {licenses.length === 0 && <span className="meta tl-empty">no licenses yet</span>}
        {licenses.map((license) => {
          const scene = sceneByNumber.get(license.scene_number);
          if (!scene) return null; // stale row for a scene we can't see
          const filmStart = scene.start_time_seconds + license.start_time_seconds;
          const duration = license.end_time_seconds - license.start_time_seconds;
          const label = `${titles.get(license.song_id) ?? "song"} · ${
            STATE_LABELS[license.state]
          } · ${formatFee(license.license_fee_cents)}`;
          return (
            <span
              key={license.id}
              className={`tl-license ${license.state}`}
              style={{ left: pct(filmStart, total), width: pct(duration, total) }}
              title={label}
            >
              {label}
            </span>
          );
        })}
      </div>

      {/* Legend */}
      <div className="tl-legend">
        {(["OFFER", "COUNTER_OFFER", "ACCEPTED", "REJECTED"] as LicenseState[]).map((state) => (
          <span key={state}>
            <i className={`tl-license ${state}`} />
            {STATE_LABELS[state]}
          </span>
        ))}
      </div>
    </div>
  );
};
