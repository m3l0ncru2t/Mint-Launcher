import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { InstanceFilesPanel } from "./InstanceFilesPanel";
import { InstanceIcon } from "./InstanceIcon";
import { ServersDialog } from "./ServersDialog";
import type { Instance, LaunchProgressEvent } from "../types";

interface Props {
  instance: Instance;
  progress: LaunchProgressEvent | null;
  logLines: string[];
  onDelete: (id: string) => void;
  onChanged: () => void;
  onDismissProgress: () => void;
  canPlay: boolean;
}

export function InstanceDetail({
  instance,
  progress,
  logLines,
  onDelete,
  onChanged,
  onDismissProgress,
  canPlay,
}: Props) {
  const [launching, setLaunching] = useState(false);
  const [showServers, setShowServers] = useState(false);
  const [serverCount, setServerCount] = useState(0);
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const [stopError, setStopError] = useState<string | null>(null);
  const logRef = useRef<HTMLDivElement>(null);

  function loadServerCount() {
    api
      .listServers(instance.id)
      .then((list) => setServerCount(list.length))
      .catch(() => {});
  }

  useEffect(() => {
    loadServerCount();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instance.id]);

  useEffect(() => {
    if (logRef.current) {
      logRef.current.scrollTop = logRef.current.scrollHeight;
    }
  }, [logLines]);

  useEffect(() => {
    if (progress?.stage === "exited" || progress?.stage === "error") {
      setLaunching(false);
    }
  }, [progress]);

  async function handlePlay(serverAddress?: string) {
    setLaunching(true);
    try {
      await api.launchInstance(instance.id, serverAddress);
    } catch {
      // surfaced via the launch-progress "error" event
    } finally {
      setLaunching(false);
    }
  }

  async function handleStop() {
    setStopError(null);
    try {
      await api.stopInstance(instance.id);
    } catch (e) {
      setStopError(String(e));
    }
  }

  async function handleExport() {
    setExportError(null);
    const destPath = await save({
      defaultPath: `${instance.name}.zip`,
      filters: [{ name: "Mint Launcher Backup", extensions: ["zip"] }],
    });
    if (!destPath) return;
    setExporting(true);
    try {
      await api.exportInstance(instance.id, destPath);
    } catch (e) {
      setExportError(String(e));
    } finally {
      setExporting(false);
    }
  }

  const pct =
    progress && progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;
  const isRunning = launching && progress?.stage === "launching";

  return (
    <div className="main-panel">
      <div className="instance-header">
        <InstanceIcon instance={instance} className="instance-header-icon" />
        <div className="instance-header-text">
          <h2>{instance.name}</h2>
          <div className="meta">
            {instance.versionId} · {instance.loader}
            {instance.loaderVersion ? ` ${instance.loaderVersion}` : ""}
            {instance.lastPlayed ? ` · last played ${new Date(instance.lastPlayed).toLocaleString()}` : ""}
          </div>
        </div>
        <div className="instance-header-actions">
          <button className="danger-btn" onClick={() => onDelete(instance.id)}>
            Delete
          </button>
          <button className="ghost-btn" onClick={handleExport} disabled={exporting}>
            {exporting ? "Exporting…" : "Export"}
          </button>
          <button className="ghost-btn" onClick={() => setShowServers(true)}>
            Servers{serverCount > 0 ? ` (${serverCount})` : ""}
          </button>
          {isRunning ? (
            <button className="stop-btn" onClick={handleStop}>
              Stop
            </button>
          ) : (
            <button className="play-btn" onClick={() => handlePlay()} disabled={launching || !canPlay}>
              {launching ? "Working…" : "Play"}
            </button>
          )}
        </div>
      </div>

      <div className="instance-body">
        {exportError && <div className="error-text">{exportError}</div>}
        {stopError && <div className="error-text">{stopError}</div>}

        {!canPlay && (
          <div className="progress-card">
            <div className="stage">Sign in required</div>
            Sign in with an account before launching this instance.
          </div>
        )}

        {progress && (
          <div className="progress-card">
            {(progress.stage === "exited" || progress.stage === "error") && (
              <button className="modal-close-btn" title="Dismiss" onClick={onDismissProgress}>
                ✕
              </button>
            )}
            <div className="stage">{progress.stage}</div>
            {progress.message}
            {progress.total > 1 && (
              <div className="progress-bar-track">
                <div className="progress-bar-fill" style={{ width: `${pct}%` }} />
              </div>
            )}
          </div>
        )}

        <InstanceFilesPanel instanceId={instance.id} />

        <div className="log-console" ref={logRef}>
          {logLines.length === 0 ? (
            <span className="placeholder">Game output will appear here once you hit Play.</span>
          ) : (
            logLines.join("\n")
          )}
        </div>
      </div>

      {showServers && (
        <ServersDialog
          instance={instance}
          onClose={() => setShowServers(false)}
          onUpdated={() => {
            onChanged();
            loadServerCount();
          }}
          joinDisabled={launching || !canPlay}
          onJoin={(address) => {
            setShowServers(false);
            handlePlay(address);
          }}
        />
      )}
    </div>
  );
}
