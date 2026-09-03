// License board for one scene: rows of licenses with state badges, fees,
// and role-gated actions (studio accepts counters / re-offers; label
// counters / accepts offers; both may reject). State transitions follow
// the backend state machine exactly.

import { useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { licenses } from "../api/endpoints";
import { ApiError } from "../api/client";
import { useSongTitles } from "../hooks/useSongTitles";
import { formatFee, STATE_LABELS, type License, type LicenseState, type Scene } from "../api/types";

interface Props {
  movieId: string;
  scene: Scene;
  licenses: License[];
  perspective: "studio" | "label";
}

export const LicenseBoard = ({ movieId, scene, licenses: rows, perspective }: Props) => {
  const titles = useSongTitles(rows.map((row) => row.song_id));
  return (
    <div className="card" style={{ marginBottom: 16 }}>
      <div className="row">
        <h3 style={{ margin: 0 }}>Scene {scene.scene_number}</h3>
        <span className="meta">{scene.description}</span>
        <div className="spacer" />
        {perspective === "studio" && (
          <Link
            to="/search"
            state={{ movieId, sceneNumber: scene.scene_number, sceneEndTime: scene.screen_time_seconds }}
          >
            <button type="button" className="small">+ License a song</button>
          </Link>
        )}
      </div>

      {rows.length === 0 ? (
        <p className="meta" style={{ margin: 0 }}>
          No licenses for this scene yet.
        </p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Song</th>
              <th>State</th>
              <th>Fee</th>
              <th>Window</th>
              <th />
            </tr>
          </thead>
          <tbody>
            {rows.map((license) => (
              <LicenseRow
                key={license.id}
                license={license}
                songTitle={titles.get(license.song_id) ?? "…"}
                perspective={perspective}
              />
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
};

const actionsFor = (
  state: LicenseState,
  perspective: "studio" | "label",
): { action: string; label: string; kind: "primary" | "danger" | "" }[] => {
  switch (state) {
    case "OFFER":
      return perspective === "label"
        ? [
            { action: "COUNTER_OFFER", label: "Counter", kind: "" },
            { action: "ACCEPT", label: "Accept", kind: "primary" },
            { action: "REJECT", label: "Reject", kind: "danger" },
          ]
        : [{ action: "REJECT", label: "Withdraw", kind: "danger" }];
    case "COUNTER_OFFER":
      return perspective === "studio"
        ? [
            { action: "ACCEPT", label: "Accept counter", kind: "primary" },
            { action: "OFFER", label: "Re-offer", kind: "" },
            { action: "REJECT", label: "Reject", kind: "danger" },
          ]
        : [{ action: "REJECT", label: "Withdraw", kind: "danger" }];
    default:
      return [];
  }
};

const LicenseRow = ({
  license,
  songTitle,
  perspective,
}: {
  license: License;
  songTitle: string;
  perspective: "studio" | "label";
}) => {
  const queryClient = useQueryClient();
  const [countering, setCountering] = useState<string | null>(null);
  const [fee, setFee] = useState(String(license.license_fee_cents / 100));
  const [error, setError] = useState<string | null>(null);

  const act = useMutation({
    mutationFn: (input: { action: string; fee?: number }) =>
      licenses.act(license.id, input.action, input.fee),
    onSuccess: () => {
      setError(null);
      setCountering(null);
      queryClient.invalidateQueries({ queryKey: ["licenses"] });
      queryClient.invalidateQueries({ queryKey: ["movie"] });
    },
    onError: (err) =>
      setError(err instanceof ApiError ? err.message : "action failed"),
  });

  return (
    <tr>
      <td>
        <strong>{songTitle}</strong>
      </td>
      <td>
        <span className={`badge ${license.state}`}>{STATE_LABELS[license.state]}</span>
      </td>
      <td>{formatFee(license.license_fee_cents)}</td>
      <td className="meta">
        {license.start_time_seconds}s–{license.end_time_seconds}s
      </td>
      <td>
        {actionsFor(license.state, perspective).map(({ action, label, kind }) => {
          const needsFee = action === "OFFER" || action === "COUNTER_OFFER";
          const open = countering === action;
          return (
            <span key={action} style={{ display: "inline-flex", gap: 6, alignItems: "center" }}>
              <button
                type="button"
                className={`small ${kind}`}
                disabled={act.isPending}
                onClick={() => {
                  if (needsFee) {
                    setCountering(open ? null : action);
                  } else {
                    act.mutate({ action });
                  }
                }}
              >
                {label}
              </button>
              {open && needsFee && (
                <span className="fee-input">
                  <span>$</span>
                  <input
                    style={{ width: 90 }}
                    type="number"
                    min="0"
                    step="0.01"
                    value={fee}
                    onChange={(e) => setFee(e.target.value)}
                  />
                  <button
                    type="button"
                    className="small primary"
                    onClick={() =>
                      act.mutate({ action, fee: Math.round(Number(fee) * 100) })
                    }
                  >
                    Send
                  </button>
                </span>
              )}
            </span>
          );
        })}
        {error && <span className="form-error" style={{ marginLeft: 8 }}>{error}</span>}
      </td>
    </tr>
  );
};
