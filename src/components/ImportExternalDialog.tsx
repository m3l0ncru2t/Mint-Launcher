import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { ImportCandidate, ImportProgressEvent, SuggestedPath } from "../types";

interface Props {
  onClose: () => void;
  onImported: (lastInstanceId: string) => void;
}

const LAUNCHER_LABELS: Record<ImportCandidate["launcher"], string> = {
  official: "Official Launcher",
  multiMc: "MultiMC / Prism / PolyMC",
  curseForge: "CurseForge",
};

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

type RowStatus = "idle" | "importing" | "done" | "error";

export function ImportExternalDialog({ onClose, onImported }: Props) {
  const [suggestions, setSuggestions] = useState<SuggestedPath[]>([]);
  const [scanning, setScanning] = useState(false);
  const [scanError, setScanError] = useState<string | null>(null);
  const [candidates, setCandidates] = useState<ImportCandidate[] | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [statuses, setStatuses] = useState<Record<number, { status: RowStatus; error?: string }>>({});
  const [importing, setImporting] = useState(false);
  const [currentProgress, setCurrentProgress] = useState<ImportProgressEvent | null>(null);

  useEffect(() => {
    api.suggestLauncherPaths().then(setSuggestions).catch(() => {});
  }, []);

  useEffect(() => {
    const unlisten = listen<ImportProgressEvent>("import-progress", (event) => {
      setCurrentProgress(event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  async function scan(path: string) {
    setScanning(true);
    setScanError(null);
    setCandidates(null);
    try {
      const found = await api.scanExternalLauncher(path);
      if (found.length === 0) {
        setScanError("No importable instances found in that folder.");
      } else {
        setCandidates(found);
        setSelected(new Set());
        setStatuses({});
      }
    } catch (e) {
      setScanError(String(e));
    } finally {
      setScanning(false);
    }
  }

  async function handleBrowse() {
    const path = await open({ directory: true, multiple: false });
    if (!path) return;
    scan(path);
  }

  function toggle(index: number) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }

  async function handleImport() {
    if (!candidates) return;
    setImporting(true);
    let lastId: string | null = null;
    for (const index of selected) {
      setStatuses((prev) => ({ ...prev, [index]: { status: "importing" } }));
      setCurrentProgress(null);
      try {
        const inst = await api.importExternalInstance(candidates[index]);
        lastId = inst.id;
        setStatuses((prev) => ({ ...prev, [index]: { status: "done" } }));
      } catch (e) {
        setStatuses((prev) => ({ ...prev, [index]: { status: "error", error: String(e) } }));
      } finally {
        setCurrentProgress(null);
      }
    }
    setImporting(false);
    if (lastId) onImported(lastId);
  }

  const allDone =
    candidates !== null &&
    selected.size > 0 &&
    Array.from(selected).every((i) => statuses[i]?.status === "done" || statuses[i]?.status === "error");

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal wide" onClick={(e) => e.stopPropagation()}>
        <h3>Import from another launcher</h3>
        <div className="subtitle">
          Bring over worlds, mods, resource packs, and server lists from the official launcher, MultiMC-family
          launchers (MultiMC, Prism Launcher, PolyMC), or CurseForge.
        </div>

        {!candidates && (
          <div className="form-field">
            <label>Where is it installed?</label>
            {suggestions.length > 0 && (
              <div className="import-suggestions">
                {suggestions.map((s) => (
                  <button
                    key={s.path}
                    type="button"
                    className="ghost-btn small"
                    disabled={scanning}
                    onClick={() => scan(s.path)}
                  >
                    {s.label}
                  </button>
                ))}
              </div>
            )}
            <button className="primary-btn" style={{ width: "100%", marginTop: 10 }} onClick={handleBrowse} disabled={scanning}>
              {scanning ? "Scanning…" : "Browse for a folder…"}
            </button>
            <div className="hint">
              Pick your <code>.minecraft</code> folder, a Prism/PolyMC/MultiMC "instances" folder, or a CurseForge
              "Instances" folder.
            </div>
            {scanError && <div className="error-text">{scanError}</div>}
          </div>
        )}

        {candidates && (
          <>
            <div className="import-candidate-list">
              {candidates.map((c, i) => {
                const status = statuses[i]?.status ?? "idle";
                return (
                  <label key={i} className={`import-candidate-row${selected.has(i) ? " selected" : ""}`}>
                    <input
                      type="checkbox"
                      checked={selected.has(i)}
                      disabled={importing}
                      onChange={() => toggle(i)}
                    />
                    <div className="import-candidate-info">
                      <div className="import-candidate-name">{c.name}</div>
                      <div className="import-candidate-meta">
                        <span className="launcher-badge">{LAUNCHER_LABELS[c.launcher]}</span>
                        {c.versionId}
                        {c.loader !== "vanilla" && ` · ${c.loader}${c.loaderVersion ? ` ${c.loaderVersion}` : ""}`}
                        {" · "}
                        {formatSize(c.sizeBytes)}
                      </div>
                      {status === "importing" && currentProgress && currentProgress.total > 0 && (
                        <div className="progress-bar-track">
                          <div
                            className="progress-bar-fill"
                            style={{
                              width: `${Math.min(100, Math.round((currentProgress.current / currentProgress.total) * 100))}%`,
                            }}
                          />
                        </div>
                      )}
                      {status === "error" && <div className="error-text">{statuses[i]?.error}</div>}
                    </div>
                    <div className="import-candidate-status">
                      {status === "importing" && (
                        <span className="hint-inline">
                          {currentProgress && currentProgress.total > 0
                            ? `${Math.min(100, Math.round((currentProgress.current / currentProgress.total) * 100))}%`
                            : "Importing…"}
                        </span>
                      )}
                      {status === "done" && <span className="hint-inline">✓ Imported</span>}
                      {status === "error" && <span className="hint-inline">Failed</span>}
                    </div>
                  </label>
                );
              })}
            </div>
            <button
              type="button"
              className="ghost-btn small"
              onClick={() => {
                setCandidates(null);
                setScanError(null);
              }}
              disabled={importing}
            >
              ← Choose a different folder
            </button>
          </>
        )}

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            {allDone ? "Close" : "Cancel"}
          </button>
          {candidates && (
            <button
              className="primary-btn"
              onClick={handleImport}
              disabled={importing || selected.size === 0 || allDone}
            >
              {importing ? "Importing…" : `Import ${selected.size || ""} selected`}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}
