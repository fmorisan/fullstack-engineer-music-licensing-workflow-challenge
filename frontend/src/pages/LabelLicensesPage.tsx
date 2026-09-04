// Label view: incoming licenses across all studios' movies, grouped by
// song, with counter/accept/reject actions.

import { useQuery } from "@tanstack/react-query";
import { licenses } from "../api/endpoints";
import { useSongTitles } from "../hooks/useSongTitles";
import { formatFee, STATE_LABELS, type License } from "../api/types";
import { useState } from "react";
import { Link } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { ApiError } from "../api/client";

const IncomingRow = ({ license }: { license: License }) => {
  const queryClient = useQueryClient();
  const [countering, setCountering] = useState(false);
  const [fee, setFee] = useState(String(license.license_fee_cents / 100));
  const [error, setError] = useState<string | null>(null);

  const act = useMutation({
    mutationFn: (input: { action: string; fee?: number }) =>
      licenses.act(license.id, input.action, input.fee),
    onSuccess: () => {
      setError(null);
      setCountering(false);
      queryClient.invalidateQueries({ queryKey: ["licenses"] });
    },
    onError: (err) =>
      setError(err instanceof ApiError ? err.message : "action failed"),
  });

  const active = license.state === "OFFER" || license.state === "COUNTER_OFFER";

  return (
    <tr>
      <td>
        <span className={`badge ${license.state}`}>{STATE_LABELS[license.state]}</span>
      </td>
      <td>{formatFee(license.license_fee_cents)}</td>
      <td className="meta">
        scene {license.scene_number} · {license.start_time_seconds}s–{license.end_time_seconds}s ·{" "}
        <Link to={`/movies/${license.movie_id}/context`}>view movie</Link>
      </td>
      <td>
        {active && license.state === "OFFER" && (
          <>
            <button
              type="button"
              className="small"
              disabled={act.isPending}
              onClick={() => setCountering(!countering)}
            >
              Counter
            </button>{" "}
            <button
              type="button"
              className="small primary"
              disabled={act.isPending}
              onClick={() => act.mutate({ action: "ACCEPT" })}
            >
              Accept
            </button>{" "}
            <button
              type="button"
              className="small danger"
              disabled={act.isPending}
              onClick={() => act.mutate({ action: "REJECT" })}
            >
              Reject
            </button>
            {countering && (
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
                    act.mutate({
                      action: "COUNTER_OFFER",
                      fee: Math.round(Number(fee) * 100),
                    })
                  }
                >
                  Send
                </button>
              </span>
            )}
          </>
        )}
        {active && license.state === "COUNTER_OFFER" && (
          <button
            type="button"
            className="small danger"
            disabled={act.isPending}
            onClick={() => act.mutate({ action: "REJECT" })}
          >
            Withdraw
          </button>
        )}
        {error && <span className="form-error" style={{ marginLeft: 8 }}>{error}</span>}
      </td>
    </tr>
  );
};

export const LabelLicensesPage = () => {
  const list = useQuery({ queryKey: ["licenses", "label"], queryFn: licenses.forLabel });
  const titles = useSongTitles((list.data ?? []).map((license) => license.song_id));

  const bySong = new Map<string, License[]>();
  for (const license of list.data ?? []) {
    const group = bySong.get(license.song_id) ?? [];
    group.push(license);
    bySong.set(license.song_id, group);
  }

  return (
    <div>
      <div className="page-header">
        <h2>Incoming licenses</h2>
        <span className="meta">Offers studios made for your catalog.</span>
      </div>

      {list.isPending && <div className="loading">Loading…</div>}
      {list.data && list.data.length === 0 && (
        <div className="empty-state">
          <h3>No licenses yet</h3>
          <p>When a studio licenses one of your songs, the offer lands here — live.</p>
        </div>
      )}

      {[...bySong.entries()].map(([songId, group]) => (
        <div key={songId} className="card" style={{ marginBottom: 16 }}>
          <h3 style={{ margin: 0 }}>{titles.get(songId) ?? "…"}</h3>
          <table>
            <thead>
              <tr>
                <th>State</th>
                <th>Fee</th>
                <th>Usage</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {group.map((license) => (
                <IncomingRow key={license.id} license={license} />
              ))}
            </tbody>
          </table>
        </div>
      ))}
    </div>
  );
};
