import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import type { AdminLogEntry } from "../types";
import { ConfirmDialog } from "./ConfirmDialog";

interface Props {
  onClose: () => void;
}

/** What remote admins did on this host, plus scheduled restarts and crash
 * auto-restarts ("Mint"), newest first (see admin_log.rs). */
export function AdminLogDialog({ onClose }: Props) {
  const [entries, setEntries] = useState<AdminLogEntry[] | null>(null);
  const [filter, setFilter] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);

  function load() {
    api
      .getAdminLog(2000)
      .then(setEntries)
      .catch((e) => setError(String(e)));
  }

  useEffect(load, []);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!entries) return [];
    if (!q) return entries;
    return entries.filter((e) => `${e.actor} ${e.action} ${e.detail}`.toLowerCase().includes(q));
  }, [entries, filter]);

  return (
    <>
      <div className="modal-backdrop" onClick={onClose}>
        <div className="modal wide admin-log-modal" onClick={(e) => e.stopPropagation()}>
          <h3>Admin log</h3>
          <div className="subtitle">
            Everything remote admins did on this server, plus scheduled and crash restarts - newest first.
          </div>

          <div className="admin-log-toolbar">
            <input
              placeholder="Filter by name, action or detail…"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
            <button className="ghost-btn small" onClick={load}>
              Refresh
            </button>
            <button
              className="danger-btn small"
              onClick={() => setConfirmClear(true)}
              disabled={!entries || entries.length === 0}
            >
              Clear
            </button>
          </div>

          {error && <div className="error-text">{error}</div>}

          <div className="admin-log-list">
            {entries === null ? (
              <div className="placeholder">Loading…</div>
            ) : shown.length === 0 ? (
              <div className="placeholder">{entries.length === 0 ? "Nothing logged yet." : "No matches."}</div>
            ) : (
              shown.map((e, i) => (
                <div key={i} className="admin-log-row">
                  <span className="admin-log-time">{new Date(e.time).toLocaleString()}</span>
                  <span className={`admin-log-actor${e.uuid ? "" : " system"}`}>{e.actor}</span>
                  <span className="admin-log-action">{e.action}</span>
                  <span className="admin-log-detail" title={e.detail}>
                    {e.detail}
                  </span>
                </div>
              ))
            )}
          </div>

          <div className="modal-actions">
            <button className="primary-btn" onClick={onClose}>
              Close
            </button>
          </div>
        </div>
      </div>

      {confirmClear && (
        <ConfirmDialog
          title="Clear the admin log?"
          message="This deletes every entry. It can't be undone."
          confirmLabel="Clear"
          danger
          onConfirm={() => {
            setConfirmClear(false);
            api
              .clearAdminLog()
              .then(() => setEntries([]))
              .catch((e) => setError(String(e)));
          }}
          onCancel={() => setConfirmClear(false)}
        />
      )}
    </>
  );
}
