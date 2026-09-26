import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { ImportCandidate, ImportProgressEvent, SuggestedPath } from "../types";

interface Props {
  onClose: () => void;
  onImported: (lastInstanceId: string) => void;
}

const LAUNCHER_ORDER: ImportCandidate["launcher"][] = ["modrinth", "curseForge", "multiMc", "official"];

const LAUNCHER_CHIP: Record<ImportCandidate["launcher"], { letter: string; color: string }> = {
  official: { letter: "M", color: "#5b8c3a" },
  multiMc: { letter: "P", color: "#e08a2c" },
  curseForge: { letter: "C", color: "#f16436" },
  modrinth: { letter: "R", color: "#1bb76e" },
};

function LauncherChip({ launcher }: { launcher: ImportCandidate["launcher"] }) {
  const chip = LAUNCHER_CHIP[launcher];
  return (
    <span className="launcher-chip" style={{ background: chip.color }} title={LAUNCHER_LABELS[launcher]}>
      {chip.letter}
    </span>
  );
}

function CandidateIcon({ candidate }: { candidate: ImportCandidate }) {
  return candidate.iconBase64 ? (
    <img className="import-candidate-icon" src={`data:image/png;base64,${candidate.iconBase64}`} alt="" />
  ) : (
    <div className="import-candidate-icon import-candidate-icon-fallback">
      {candidate.name.slice(0, 1).toUpperCase() || "?"}
    </div>
  );
}

const LAUNCHER_LABELS: Record<ImportCandidate["launcher"], string> = {
  official: "Official Launcher",
  multiMc: "MultiMC / Prism / PolyMC",
  curseForge: "CurseForge",
  modrinth: "Modrinth App",
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
  const [restoring, setRestoring] = useState(false);
  const [restoreError, setRestoreError] = useState<string | null>(null);

  // Scans every launcher folder Mint already knows to look in as soon as the
  // dialog opens, so whatever's installed shows up ready to tick - no
  // clicking through a button per launcher first.
  const [autoScanning, setAutoScanning] = useState(true);
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const paths = await api.suggestLauncherPaths().catch(() => [] as SuggestedPath[]);
      if (cancelled) return;
      setSuggestions(paths);
      const results = await Promise.allSettled(paths.map((p) => api.scanExternalLauncher(p.path)));
      if (cancelled) return;
      const seen = new Set<string>();
      const found: ImportCandidate[] = [];
      for (const r of results) {
        if (r.status !== "fulfilled") continue;
        for (const c of r.value) {
          if (seen.has(c.sourcePath)) continue;
          seen.add(c.sourcePath);
          found.push(c);
        }
      }
      if (found.length > 0) setCandidates(found);
      setAutoScanning(false);
    })();
    return () => {
      cancelled = true;
    };
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

  async function handleRestoreBackup() {
    const path = await open({
      multiple: false,
      filters: [{ name: "Mint Launcher Backup", extensions: ["zip"] }],
    });
    if (!path) return;
    setRestoreError(null);
    setRestoring(true);
    try {
      const inst = await api.importInstance(path);
      onImported(inst.id);
    } catch (e) {
      setRestoreError(String(e));
    } finally {
      setRestoring(false);
    }
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
        <h3>Import instance</h3>
        <div className="subtitle">
          Restore a Mint Launcher backup, or bring over worlds, mods, resource packs, and server lists from the
          official launcher, MultiMC-family launchers (MultiMC, Prism Launcher, PolyMC), CurseForge, or the Modrinth
          App.
        </div>

        {!candidates && autoScanning && <div className="hint">Looking for instances from your other launchers…</div>}

        {!candidates && (
          <>
            {!autoScanning && suggestions.length > 0 && (
              <div className="hint">No instances were found automatically - pick a folder below.</div>
            )}
            <div className="form-field">
              <label>Have a Mint Launcher backup?</label>
              <button className="ghost-btn" style={{ width: "100%" }} onClick={handleRestoreBackup} disabled={restoring}>
                {restoring ? "Restoring…" : "Restore backup (.zip)…"}
              </button>
              {restoreError && <div className="error-text">{restoreError}</div>}
            </div>

            <div className="form-field">
              <label>Or import from another launcher</label>
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
                Pick your <code>.minecraft</code> folder, a Prism/PolyMC/MultiMC "instances" folder, a CurseForge
                "Instances" folder, or the Modrinth App's data folder.
              </div>
              {scanError && <div className="error-text">{scanError}</div>}
            </div>
          </>
        )}

        {candidates && (
          <>
            <div className="import-list-toolbar">
              <span className="hint-inline">
                {candidates.length} found · {selected.size} selected
              </span>
              <span>
                <button
                  type="button"
                  className="ghost-btn small"
                  disabled={importing}
                  onClick={() => setSelected(new Set(candidates.map((_, i) => i)))}
                >
                  Select all
                </button>{" "}
                <button type="button" className="ghost-btn small" disabled={importing} onClick={() => setSelected(new Set())}>
                  None
                </button>
              </span>
            </div>
            <div className="import-candidate-list">
              {LAUNCHER_ORDER.map((launcher) => {
                const rows = candidates.map((c, i) => ({ c, i })).filter(({ c }) => c.launcher === launcher);
                if (rows.length === 0) return null;
                return (
                  <div key={launcher} className="import-group">
                    <div className="import-group-header">
                      <LauncherChip launcher={launcher} />
                      {LAUNCHER_LABELS[launcher]}
                      <span className="hint-inline">({rows.length})</span>
                    </div>
                    {rows.map(({ c, i }) => {
                      const status = statuses[i]?.status ?? "idle";
                      return (
                        <label key={i} className={`import-candidate-row${selected.has(i) ? " selected" : ""}`}>
                          <input
                            type="checkbox"
                            checked={selected.has(i)}
                            disabled={importing}
                            onChange={() => toggle(i)}
                          />
                          <CandidateIcon candidate={c} />
                          <div className="import-candidate-info">
                            <div className="import-candidate-name">{c.name}</div>
                            <div className="import-candidate-meta">
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
              ← Restore a backup or pick another folder
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
