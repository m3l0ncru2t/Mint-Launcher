import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { ModsPanel } from "./ModsPanel";
import { ServersDialog } from "./ServersDialog";
import type { Instance, LaunchProgressEvent } from "../types";

interface Props {
  instance: Instance;
  progress: LaunchProgressEvent | null;
  logLines: string[];
  onDelete: (id: string) => void;
  onChanged: () => void;
  canPlay: boolean;
}

export function InstanceDetail({ instance, progress, logLines, onDelete, onChanged, canPlay }: Props) {
  const [launching, setLaunching] = useState(false);
  const [showServers, setShowServers] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);

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

  const pct =
    progress && progress.total > 0 ? Math.round((progress.current / progress.total) * 100) : 0;

  return (
    <div className="main-panel">
      <div className="instance-header">
        <div className="instance-header-icon">{instance.name.slice(0, 1).toUpperCase()}</div>
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
          <button className="ghost-btn" onClick={() => setShowServers(true)}>
            Servers{instance.servers.length > 0 ? ` (${instance.servers.length})` : ""}
          </button>
          <button className="play-btn" onClick={() => handlePlay()} disabled={launching || !canPlay}>
            {launching ? "Working…" : "Play"}
          </button>
        </div>
      </div>

      <div className="instance-body">
        {!canPlay && (
          <div className="progress-card">
            <div className="stage">Sign in required</div>
            Sign in with an account before launching this instance.
          </div>
        )}

        {progress && (
          <div className="progress-card">
            <div className="stage">{progress.stage}</div>
            {progress.message}
            {progress.total > 1 && (
              <div className="progress-bar-track">
                <div className="progress-bar-fill" style={{ width: `${pct}%` }} />
              </div>
            )}
          </div>
        )}

        <ModsPanel instanceId={instance.id} />

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
          onUpdated={onChanged}
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
