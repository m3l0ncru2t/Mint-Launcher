import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { InstanceFilesPanel } from "./InstanceFilesPanel";
import { InstanceIcon } from "./InstanceIcon";
import { ServersDialog } from "./ServersDialog";
import type { Instance, LaunchProgressEvent } from "../types";

// Exported so Sidebar can show a loading state for every instance, not just
// the selected one - kept as a single source of truth for "what stage means
// this instance is currently busy" so the two views can't drift apart.
export const ACTIVE_STAGES = new Set(["java", "client", "libraries", "assets", "launching"]);

/// A crash's actual cause is usually buried hundreds of lines up the
/// console, well past what "exited with code N" tells you - matching a
/// handful of well-known signatures against the tail of the log turns that
/// into an actionable one-liner instead of a scavenger hunt.
const CRASH_SIGNATURES: [RegExp, string][] = [
  [/UnsupportedClassVersionError/, "This build of Java is too old for this Minecraft version - try relaunching so it can fetch a matching runtime."],
  [/java\.lang\.OutOfMemoryError/, "Minecraft ran out of memory - try raising the memory limit in this instance's settings."],
  [/Failed to find Main Class|NoClassDefFoundError|ClassNotFoundException/, "A required file failed to load - try Backup, then delete and reinstall the instance's mods."],
  [/Pixel Format not accelerated|Couldn't set pixel format|NativeCreationException/, "The graphics driver rejected Minecraft's display setup - update your GPU driver."],
  [/EXCEPTION_ACCESS_VIOLATION|A fatal error has been detected by the Java Runtime Environment/, "The JVM crashed at a low level, likely a broken mod or a graphics driver issue."],
];

function findCrashHint(logLines: string[]): string | null {
  const tail = logLines.slice(-500).join("\n");
  for (const [pattern, hint] of CRASH_SIGNATURES) {
    if (pattern.test(tail)) return hint;
  }
  return null;
}

function parseExitCode(message: string): number | null {
  const match = message.match(/exited with code (-?\d+)/);
  return match ? Number(match[1]) : null;
}

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
  // Only covers the brief gap between clicking Play and the first
  // "launch-progress" event - actual play/stop button state is derived from
  // `progress` (lifted up to App.tsx, keyed by instance id) instead, since
  // `InstanceDetail` gets remounted (see `key={instance.id}` in App.tsx)
  // every time the selected instance changes, which would otherwise reset
  // local state back to "not running" when switching back to an instance
  // that's still playing.
  const [starting, setStarting] = useState(false);
  const [showServers, setShowServers] = useState(false);
  const [serverCount, setServerCount] = useState(0);
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const [stopError, setStopError] = useState<string | null>(null);
  const [showConsole, setShowConsole] = useState(false);
  const [copied, setCopied] = useState(false);
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

  // A launch failure or a non-zero exit almost always means something worth
  // reading went to the console - pop it open automatically instead of
  // leaving the user to notice it's hidden.
  useEffect(() => {
    if (progress?.stage === "error") {
      setShowConsole(true);
    } else if (progress?.stage === "exited" && parseExitCode(progress.message) !== 0) {
      setShowConsole(true);
    }
  }, [progress]);

  async function handlePlay(serverAddress?: string) {
    setStarting(true);
    try {
      await api.launchInstance(instance.id, serverAddress);
    } catch {
      // surfaced via the launch-progress "error" event
    } finally {
      setStarting(false);
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

  async function handleCopyConsole() {
    await navigator.clipboard.writeText(logLines.join("\n"));
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
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
  const isBusy = starting || (progress ? ACTIVE_STAGES.has(progress.stage) : false);
  const isRunning = progress?.stage === "running";
  const crashHint =
    progress?.stage === "exited" && parseExitCode(progress.message) !== 0 ? findCrashHint(logLines) : null;

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
            {exporting ? "Backing up…" : "Backup"}
          </button>
          <button className="ghost-btn" onClick={() => setShowServers(true)}>
            Servers{serverCount > 0 ? ` (${serverCount})` : ""}
          </button>
          {isRunning ? (
            <button className="stop-btn" onClick={handleStop}>
              Stop
            </button>
          ) : (
            <button className="play-btn" onClick={() => handlePlay()} disabled={isBusy || !canPlay}>
              {isBusy ? "Working…" : "Play"}
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
            {crashHint && <div className="crash-hint">{crashHint}</div>}
            {progress.total > 1 && (
              <div className="progress-bar-track">
                <div className="progress-bar-fill" style={{ width: `${pct}%` }} />
              </div>
            )}
          </div>
        )}

        <InstanceFilesPanel instanceId={instance.id} />

        <div className="log-console-bar">
          <span className="log-console-label">Console</span>
          <div className="log-console-bar-actions">
            <button
              className="ghost-btn small"
              onClick={handleCopyConsole}
              disabled={logLines.length === 0}
            >
              {copied ? "Copied!" : "Copy"}
            </button>
            <button className="ghost-btn small" onClick={() => setShowConsole((s) => !s)}>
              {showConsole ? "Hide" : "Show"}
            </button>
          </div>
        </div>
        {showConsole && (
          <div className="log-console" ref={logRef}>
            {logLines.length === 0 ? (
              <span className="placeholder">Game output will appear here once you hit Play.</span>
            ) : (
              logLines.join("\n")
            )}
          </div>
        )}
      </div>

      {showServers && (
        <ServersDialog
          instance={instance}
          onClose={() => setShowServers(false)}
          onUpdated={() => {
            onChanged();
            loadServerCount();
          }}
          joinDisabled={isBusy || !canPlay}
          onJoin={(address) => {
            setShowServers(false);
            handlePlay(address);
          }}
        />
      )}
    </div>
  );
}
