import { useEffect, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { ConfirmDialog } from "./ConfirmDialog";
import { InstanceFilesPanel } from "./InstanceFilesPanel";
import { InstanceIcon } from "./InstanceIcon";
import { ServersDialog } from "./ServersDialog";
import type { Instance, LaunchProgressEvent, ProcessStats, TpsInfo } from "../types";
import { setVisibleInterval } from "../lib/visibleInterval";
import { useUptime } from "../lib/uptime";
import { getPublicIpCached } from "../lib/publicIp";

/** Connect addresses for a server's header: LAN IP, and the owner's domain
 * if they set one on the Server tab (else the public IP). */
function useServerAddresses(instanceId: string | null) {
  const [value, setValue] = useState<{ local: string | null; public: string | null; isDomain: boolean } | null>(null);
  useEffect(() => {
    setValue(null);
    if (!instanceId) return;
    let cancelled = false;
    async function load() {
      try {
        const a = await api.getServerAddresses(instanceId!);
        const withPort = (host: string) => (a.port === 25565 ? host : `${host}:${a.port}`);
        const local = a.localIp ? `${a.localIp}:${a.port}` : null;
        if (a.domain) {
          if (!cancelled) setValue({ local, public: withPort(a.domain), isDomain: true });
          return;
        }
        if (!cancelled) setValue({ local, public: null, isDomain: false });
        const ip = await getPublicIpCached().catch(() => null);
        if (!cancelled && ip) setValue({ local, public: `${ip}:${a.port}`, isDomain: false });
      } catch {
        // No addresses to show.
      }
    }
    load();
    const onDomainChanged = (e: Event) => {
      if ((e as CustomEvent<string>).detail === instanceId) load();
    };
    window.addEventListener("mint-server-domain-changed", onDomainChanged);
    return () => {
      cancelled = true;
      window.removeEventListener("mint-server-domain-changed", onDomainChanged);
    };
  }, [instanceId]);
  return value;
}

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

// A clean exit's message ("Minecraft closed"/"Server stopped") has no code
// in it at all; every other exit's message ends in "(... exit code N ...)".
function parseExitCode(message: string): number | null {
  const match = message.match(/exit code (-?\d+)/);
  return match ? Number(match[1]) : null;
}

function isCrashExit(progress: LaunchProgressEvent | null): boolean {
  if (progress?.stage !== "exited") return false;
  const code = parseExitCode(progress.message);
  return code !== null && code !== 0;
}

function stageLabel(progress: LaunchProgressEvent | null, isServer: boolean): string {
  if (!progress) return "running";
  if (progress.stage === "exited") {
    if (isCrashExit(progress)) return progress.message.includes("crashed") ? "crashed" : "stopped unexpectedly";
    return isServer ? "stopped" : "closed";
  }
  return progress.stage;
}

interface Props {
  instance: Instance;
  progress: LaunchProgressEvent | null;
  logLines: string[];
  pid: number | null;
  restartCountdown: number | null;
  showConfigsLogsTabs: boolean;
  spaciousView: boolean;
  onDelete: (id: string) => void;
  onChanged: () => void;
  onDismissProgress: () => void;
  canPlay: boolean;
}

export function InstanceDetail({
  instance,
  progress,
  logLines,
  pid,
  restartCountdown,
  showConfigsLogsTabs,
  spaciousView,
  onDelete,
  onChanged,
  onDismissProgress,
  canPlay,
}: Props) {
  // Only covers the brief gap between clicking Play and the first
  // "launch-progress"/"instance-running-changed" event - actual play/stop
  // button state is derived from `pid` (lifted up to App.tsx, keyed by
  // instance id) instead, since `InstanceDetail` gets remounted (see
  // `key={instance.id}` in App.tsx) every time the selected instance
  // changes, which would otherwise reset local state back to "not running"
  // when switching back to an instance that's still playing.
  const [starting, setStarting] = useState(false);
  const [showServers, setShowServers] = useState(false);
  const [serverCount, setServerCount] = useState(0);
  const [exporting, setExporting] = useState(false);
  const [exportError, setExportError] = useState<string | null>(null);
  const [stopError, setStopError] = useState<string | null>(null);
  const [restarting, setRestarting] = useState(false);
  const [confirmAction, setConfirmAction] = useState<"restart" | "stop" | "kill" | null>(null);
  const [showConsole, setShowConsole] = useState(false);
  const [copied, setCopied] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);

  const isServer = instance.kind === "server";
  // Deliberately keyed off `pid` (sourced from `runningByInstance` in
  // App.tsx, which is reconciled against actually-alive processes on every
  // startup) rather than `progress?.stage === "running"` - the latter is
  // only ever set by a live "launch-progress" event received *this*
  // session, so if Mint itself restarts (an update, a crash, ...) while an
  // instance keeps running, there's no such event to see and the button
  // would otherwise fall back to "Start"/"Play" even though the sidebar
  // (which already used the reconciled pid) correctly still shows it running.
  const isRunning = pid != null;

  const [stats, setStats] = useState<ProcessStats | null>(null);
  const addresses = useServerAddresses(isServer ? instance.id : null);
  const uptime = useUptime(isServer && isRunning ? stats?.startTime : null);

  useEffect(() => {
    if (!isServer || !isRunning || pid == null) {
      setStats(null);
      return;
    }
    let cancelled = false;
    function poll() {
      api
        .getProcessStats(pid!)
        .then((s) => !cancelled && setStats(s))
        .catch(() => !cancelled && setStats(null));
    }
    poll();
    const stopInterval = setVisibleInterval(poll, 2000);
    return () => {
      cancelled = true;
      stopInterval();
    };
  }, [isServer, isRunning, pid]);

  const [tps, setTps] = useState<TpsInfo | null>(null);

  // Only available once Mint holds this server's console (same requirement
  // as the player list above) - there's no network-ping fallback for this
  // one, since TPS/MSPT aren't part of the Server List Ping protocol at all.
  useEffect(() => {
    if (!isServer || !isRunning) {
      setTps(null);
      return;
    }
    let cancelled = false;
    function poll() {
      api
        .getServerTps(instance.id)
        .then((info) => !cancelled && setTps(info))
        .catch(() => !cancelled && setTps(null));
    }
    poll();
    const stopInterval = setVisibleInterval(poll, 5000);
    return () => {
      cancelled = true;
      stopInterval();
    };
  }, [instance.id, isServer, isRunning]);

  const [playerCount, setPlayerCount] = useState<{ online: number; max: number } | null>(null);

  // Prefers sending the server's own `list` console command and reading the
  // response back off its log - unlike a network ping, that still works for
  // a server sitting behind a proxy-only protection mod (TCPShield, most
  // notably) that rejects any direct connection to the game port, Mint's own
  // status ping included. Only available while Mint itself is holding that
  // console's stdin, though, so this falls back to the same server-list-ping
  // the "Servers" bookmark dialog uses (pointed at this instance's own port
  // on localhost) whenever it isn't - e.g. an adopted server Mint didn't
  // launch itself.
  //
  // That fallback ping is exactly what a mod like TCPShield exists to reject
  // (and log loudly) - fine as a one-off check, but retrying it every 5s
  // forever after console access is lost (e.g. right after a Mint restart,
  // before the next Stop/Start) would spam the server's own log with
  // rejection warnings indefinitely. `pingGaveUp` latches once that ping
  // fails so this stops trying it again until console access actually comes
  // back - it'll just show nothing in the meantime instead of retrying a
  // dead end.
  useEffect(() => {
    if (!isServer || !isRunning) {
      setPlayerCount(null);
      return;
    }
    let cancelled = false;
    let port = "25565";
    let pingGaveUp = false;
    async function poll() {
      try {
        const status = await api.listOnlinePlayers(instance.id);
        pingGaveUp = false;
        if (!cancelled && status.online != null && status.max != null) {
          setPlayerCount({ online: status.online, max: status.max });
        }
        return;
      } catch {
        // Fall through to the network-ping fallback below.
      }
      if (pingGaveUp) {
        if (!cancelled) setPlayerCount(null);
        return;
      }
      try {
        const status = await api.pingServer(`localhost:${port}`);
        if (!cancelled && status.online != null && status.max != null) {
          setPlayerCount({ online: status.online, max: status.max });
        }
      } catch {
        pingGaveUp = true;
        if (!cancelled) setPlayerCount(null);
      }
    }
    api
      .getServerProperties(instance.id)
      .then((props) => {
        port = props["server-port"] || "25565";
      })
      .catch(() => {})
      .finally(poll);
    const stopInterval = setVisibleInterval(poll, 5000);
    return () => {
      cancelled = true;
      stopInterval();
    };
  }, [instance.id, isServer, isRunning]);

  // A server started outside Mint entirely (by hand, by another panel, or
  // before Mint was even open) has no launch of its own for Mint to have
  // seen - `pid`/`isRunning` would otherwise stay stuck at "not running"
  // forever. Checks once on selecting the instance and again whenever the
  // window regains focus (same "might have changed while I was away"
  // pattern the mod/resource-pack panels already use), rather than polling
  // continuously - a full process scan isn't something to run every couple
  // of seconds.
  useEffect(() => {
    if (!isServer || isRunning) return;
    const detect = () => {
      api.detectRunningServer(instance.id).catch(() => {});
    };
    detect();
    window.addEventListener("focus", detect);
    return () => window.removeEventListener("focus", detect);
  }, [instance.id, isServer, isRunning]);

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
    } else if (isCrashExit(progress)) {
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

  async function handleKill() {
    setStopError(null);
    try {
      await api.killInstance(instance.id);
    } catch (e) {
      setStopError(String(e));
    }
  }

  async function handleRestart() {
    setStopError(null);
    setRestarting(true);
    try {
      // Broadcasts the 60/30/10-second in-game warning itself before
      // actually stopping and relaunching - see restart_instance. Can be
      // called off mid-countdown with handleCancelRestart below.
      await api.restartInstance(instance.id);
    } catch (e) {
      setStopError(String(e));
    } finally {
      setRestarting(false);
    }
  }

  async function handleCancelRestart() {
    try {
      await api.cancelRestart(instance.id);
    } catch (e) {
      setStopError(String(e));
    }
  }

  // Restart/Stop/Kill all disrupt anyone currently connected (Kill with no
  // warning at all) - routed through a confirmation dialog instead of
  // running straight off the button click, since one misclick shouldn't be
  // able to knock everyone off a live server.
  function runConfirmedAction() {
    const action = confirmAction;
    setConfirmAction(null);
    if (action === "restart") handleRestart();
    else if (action === "stop") handleStop();
    else if (action === "kill") handleKill();
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
  const crashHint =
    isCrashExit(progress) ? findCrashHint(logLines) : null;

  return (
    <div className="main-panel">
      <div className="instance-header">
        <InstanceIcon instance={instance} className="instance-header-icon" />
        <div className="instance-header-text">
          <h2>{instance.name}</h2>
          <div className="meta">
            {instance.versionId} · {instance.loader}
            {instance.loaderVersion ? ` ${instance.loaderVersion}` : ""}
            {uptime ? ` · up ${uptime}` : ""}
            {!isServer && instance.lastPlayed ? ` · last played ${new Date(instance.lastPlayed).toLocaleString()}` : ""}
          </div>
          {addresses && (
            <div className="meta server-address-meta">
              {addresses.local && (
                <span title="Local IP - for players on the same network">LAN {addresses.local}</span>
              )}
              {addresses.local && addresses.public && " · "}
              {addresses.public && (
                <span title={addresses.isDomain ? "Domain - what players outside your network use" : "Public IP - for players outside your network"}>
                  {addresses.isDomain ? "" : "Public "}
                  {addresses.public}
                </span>
              )}
            </div>
          )}
        </div>
        <div className="instance-header-actions">
          <button className="danger-btn" onClick={() => onDelete(instance.id)}>
            Delete
          </button>
          <button className="ghost-btn" onClick={handleExport} disabled={exporting}>
            {exporting ? "Backing up…" : "Backup"}
          </button>
          {!isServer && (
            <button className="ghost-btn" onClick={() => setShowServers(true)}>
              Servers{serverCount > 0 ? ` (${serverCount})` : ""}
            </button>
          )}
          {isServer ? (
            isRunning ? (
              <>
                <button className="restart-btn" onClick={() => setConfirmAction("restart")} disabled={restarting}>
                  {restartCountdown != null ? `Restarting in ${restartCountdown}s…` : restarting ? "Restarting…" : "Restart"}
                </button>
                {restartCountdown != null && (
                  <button className="ghost-btn" onClick={handleCancelRestart}>
                    Cancel restart
                  </button>
                )}
                <button className="stop-btn" onClick={() => setConfirmAction("stop")}>
                  Stop
                </button>
                <button className="danger-btn" onClick={() => setConfirmAction("kill")}>
                  Kill
                </button>
              </>
            ) : (
              <button className="play-btn" onClick={() => handlePlay()} disabled={isBusy}>
                {isBusy ? "Working…" : "Start"}
              </button>
            )
          ) : isRunning ? (
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

      <div className={spaciousView ? "instance-body-spacious" : "instance-body"}>
        {exportError && <div className="error-text">{exportError}</div>}
        {stopError && <div className="error-text">{stopError}</div>}

        {!isServer && !canPlay && (
          <div className="progress-card">
            <div className="stage">Sign in required</div>
            Sign in with an account before launching this instance.
          </div>
        )}

        {(progress || isRunning) && (
          <div className="progress-card">
            {(progress?.stage === "exited" || progress?.stage === "error") && (
              <button className="modal-close-btn" title="Dismiss" onClick={onDismissProgress}>
                ✕
              </button>
            )}
            <div className="progress-card-row">
              <div className="progress-card-main">
                <div className="stage">{stageLabel(progress, isServer)}</div>
                <div>{progress?.message ?? (isServer ? "Server is running" : "Minecraft is running")}</div>
              </div>
              {isServer && isRunning && (stats || playerCount || tps) && (
                <div className="server-resource-stats">
                  <div className="server-resource-column">
                    {stats && (
                      <div className="server-resource-stat">
                        <svg className="server-resource-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
                          <rect x="7" y="7" width="10" height="10" rx="1.5" />
                          <path d="M9.5 2.5v4M14.5 2.5v4M9.5 17.5v4M14.5 17.5v4M2.5 9.5h4M2.5 14.5h4M17.5 9.5h4M17.5 14.5h4" />
                        </svg>
                        <div className="server-resource-info">
                          <div className="server-resource-line">
                            <span className="server-resource-label">CPU</span>
                            <span className="server-resource-value">{stats.cpuPercent.toFixed(0)}%</span>
                          </div>
                          <div className="server-resource-bar-track">
                            <div
                              className="server-resource-bar-fill"
                              style={{ width: `${Math.min(100, stats.cpuPercent)}%` }}
                            />
                          </div>
                        </div>
                      </div>
                    )}
                    {playerCount && (
                      <div className="server-resource-stat">
                        <svg className="server-resource-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
                          <circle cx="9" cy="8" r="3.2" />
                          <path d="M3.5 20c0-3.5 2.5-6 5.5-6s5.5 2.5 5.5 6" />
                          <circle cx="17" cy="9" r="2.6" />
                          <path d="M15.2 14.3c2.4.4 4.3 2.6 4.3 5.7" />
                        </svg>
                        <div className="server-resource-info">
                          <div className="server-resource-line">
                            <span className="server-resource-label">Players</span>
                            <span className="server-resource-value">
                              {playerCount.online} / {playerCount.max}
                            </span>
                          </div>
                          <div className="server-resource-bar-track">
                            <div
                              className="server-resource-bar-fill"
                              style={{
                                width: `${playerCount.max > 0 ? Math.min(100, (playerCount.online / playerCount.max) * 100) : 0}%`,
                              }}
                            />
                          </div>
                        </div>
                      </div>
                    )}
                  </div>
                  {stats && (
                    <div className="server-resource-column">
                      <div className="server-resource-stat">
                        <svg className="server-resource-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
                          <rect x="3" y="8" width="18" height="9" rx="1.5" />
                          <path d="M7 8V4.5M11 8V4.5M15 8V4.5M7 20v-3M11 20v-3M15 20v-3" />
                        </svg>
                        <div className="server-resource-info">
                          <div className="server-resource-line">
                            <span className="server-resource-label">Memory</span>
                            <span className="server-resource-value">
                              {stats.memoryMb} / {instance.memoryMb} MB
                            </span>
                          </div>
                          <div className="server-resource-bar-track">
                            <div
                              className="server-resource-bar-fill"
                              style={{ width: `${Math.min(100, (stats.memoryMb / instance.memoryMb) * 100)}%` }}
                            />
                          </div>
                        </div>
                      </div>
                      <div className="server-resource-stat">
                        <svg className="server-resource-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
                          <rect x="3" y="4" width="18" height="12" rx="1.5" />
                          <path d="M9 20h6M12 16v4" />
                        </svg>
                        <div className="server-resource-info">
                          <div className="server-resource-line">
                            <span className="server-resource-label">System</span>
                            <span className="server-resource-value">
                              {stats.systemUsedMemoryMb} / {stats.systemTotalMemoryMb} MB
                            </span>
                          </div>
                          <div className="server-resource-bar-track">
                            <div
                              className="server-resource-bar-fill"
                              style={{
                                width: `${Math.min(100, (stats.systemUsedMemoryMb / stats.systemTotalMemoryMb) * 100)}%`,
                              }}
                            />
                          </div>
                        </div>
                      </div>
                    </div>
                  )}
                  {tps && (
                    <div className="server-resource-column">
                      <div className="server-resource-stat">
                        <svg className="server-resource-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
                          <path d="M3 13h3.5l1.8-5 3 10 2.2-9 1.5 4H21" />
                        </svg>
                        <div className="server-resource-info">
                          <div className="server-resource-line">
                            <span className="server-resource-label">TPS</span>
                            <span className="server-resource-value">{tps.tps.toFixed(1)} / 20</span>
                          </div>
                          <div className="server-resource-bar-track">
                            <div
                              className="server-resource-bar-fill"
                              style={{ width: `${Math.min(100, (tps.tps / 20) * 100)}%` }}
                            />
                          </div>
                        </div>
                      </div>
                      <div className="server-resource-stat">
                        <svg className="server-resource-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
                          <circle cx="12" cy="13.5" r="8" />
                          <path d="M12 9.5v4l3 2M10 2h4" />
                        </svg>
                        <div className="server-resource-info">
                          <div className="server-resource-line">
                            <span className="server-resource-label">MSPT</span>
                            <span className="server-resource-value">{tps.mspt.toFixed(1)} ms</span>
                          </div>
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
            {crashHint && <div className="crash-hint">{crashHint}</div>}
            {progress && progress.total > 1 && (
              <div className="progress-bar-track">
                <div className="progress-bar-fill" style={{ width: `${pct}%` }} />
              </div>
            )}
          </div>
        )}

        <InstanceFilesPanel
          instanceId={instance.id}
          isServer={isServer}
          logLines={logLines}
          isRunning={isRunning}
          showConfigsLogsTabs={showConfigsLogsTabs}
        />

        {!isServer && (
          <>
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
          </>
        )}
      </div>

      {!isServer && showServers && (
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

      {confirmAction && (
        <ConfirmDialog
          title={
            confirmAction === "restart" ? "Restart server?" : confirmAction === "stop" ? "Stop server?" : "Kill server?"
          }
          message={
            confirmAction === "restart"
              ? "Players get a 60/30/10/5-second warning in-game before it actually restarts."
              : confirmAction === "stop"
                ? "Connected players will be disconnected while it shuts down gracefully."
                : "This force-kills the process immediately - connected players are disconnected with no warning, and any progress since the last autosave could be lost."
          }
          confirmLabel={confirmAction === "restart" ? "Restart" : confirmAction === "stop" ? "Stop" : "Kill"}
          danger={confirmAction !== "restart"}
          onConfirm={runConfirmedAction}
          onCancel={() => setConfirmAction(null)}
        />
      )}
    </div>
  );
}
