import { useState } from "react";
import { api } from "../api";
import type { Instance, RestartSchedule } from "../types";

interface Props {
  instance: Instance;
  onClose: () => void;
  onSaved: () => void;
}

/** When a server restarts itself, and whether it comes back after a crash
 * (see server_automation.rs). Restarts use the normal restart path, so
 * players still get the in-game countdown and it can be canceled. */
export function ServerScheduleDialog({ instance, onClose, onSaved }: Props) {
  const initial: RestartSchedule = instance.restartSchedule ?? {
    enabled: false,
    mode: "daily",
    times: ["05:00"],
    intervalHours: 12,
  };
  const [enabled, setEnabled] = useState(initial.enabled);
  const [mode, setMode] = useState<RestartSchedule["mode"]>(initial.mode);
  const [times, setTimes] = useState<string[]>(initial.times.length > 0 ? initial.times : ["05:00"]);
  const [intervalHours, setIntervalHours] = useState(initial.intervalHours || 12);
  const [autoRestart, setAutoRestart] = useState(instance.autoRestartOnCrash ?? false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function save() {
    setSaving(true);
    setError(null);
    try {
      await api.setServerAutomation(
        instance.id,
        { enabled, mode, times: times.filter((t) => t.trim() !== ""), intervalHours },
        autoRestart,
      );
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>Restarts</h3>
        <div className="subtitle">{instance.name}</div>

        <div className="form-field">
          <label className="schedule-check">
            <input type="checkbox" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />
            Restart on a schedule
          </label>
          <div className="hint">
            Players get the usual 60-second countdown first, and you can cancel it from the Restart button.
          </div>
        </div>

        {enabled && (
          <div className="form-field schedule-options">
            <label className="schedule-check">
              <input type="radio" checked={mode === "daily"} onChange={() => setMode("daily")} />
              Every day at
            </label>
            {mode === "daily" && (
              <div className="schedule-times">
                {times.map((t, i) => (
                  <div key={i} className="schedule-time-row">
                    <input
                      type="time"
                      value={t}
                      onChange={(e) => setTimes((prev) => prev.map((p, j) => (j === i ? e.target.value : p)))}
                    />
                    {times.length > 1 && (
                      <button
                        className="icon-btn"
                        title="Remove this time"
                        onClick={() => setTimes((prev) => prev.filter((_, j) => j !== i))}
                      >
                        ✕
                      </button>
                    )}
                  </div>
                ))}
                {times.length < 12 && (
                  <button className="ghost-btn small" onClick={() => setTimes((prev) => [...prev, "17:00"])}>
                    + Add a time
                  </button>
                )}
                <div className="hint">This computer's local time.</div>
              </div>
            )}

            <label className="schedule-check" style={{ marginTop: 10 }}>
              <input type="radio" checked={mode === "interval"} onChange={() => setMode("interval")} />
              Every
              <input
                type="number"
                className="schedule-hours"
                min={1}
                max={168}
                value={intervalHours}
                disabled={mode !== "interval"}
                onChange={(e) => setIntervalHours(Math.max(1, Math.min(168, Number(e.target.value) || 1)))}
              />
              hours of uptime
            </label>
          </div>
        )}

        <div className="form-field">
          <label className="schedule-check">
            <input type="checkbox" checked={autoRestart} onChange={(e) => setAutoRestart(e.target.checked)} />
            Restart automatically if it crashes
          </label>
          <div className="hint">
            Starts it again 10 seconds after a crash. Stopping or killing it yourself doesn't count. If it crashes 3
            times within 15 minutes, Mint stops retrying so a broken server doesn't loop.
          </div>
        </div>

        {error && <div className="error-text">{error}</div>}

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Cancel
          </button>
          <button className="primary-btn" onClick={save} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}

/** When the next scheduled restart will happen, for the header - `null`
 * when there's no schedule (or it can't be worked out yet). */
export function nextScheduledRestart(instance: Instance, startedAtSecs: number | null | undefined): Date | null {
  const s = instance.restartSchedule;
  if (!s?.enabled) return null;
  if (s.mode === "interval") {
    if (!startedAtSecs) return null;
    return new Date((startedAtSecs + Math.max(1, s.intervalHours) * 3600) * 1000);
  }
  const now = new Date();
  let best: Date | null = null;
  for (const t of s.times) {
    const m = /^(\d{1,2}):(\d{2})$/.exec(t.trim());
    if (!m) continue;
    const d = new Date(now);
    d.setHours(Number(m[1]), Number(m[2]), 0, 0);
    if (d <= now) d.setDate(d.getDate() + 1);
    if (!best || d < best) best = d;
  }
  return best;
}
