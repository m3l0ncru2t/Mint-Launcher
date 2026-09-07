import { useEffect, useRef, useState } from "react";
import { remoteApi } from "../remoteApi";
import { RemoteBrowseModsDialog } from "./RemoteBrowseModsDialog";
import { RemoteBrowseResourcePacksDialog } from "./RemoteBrowseResourcePacksDialog";
import type { ModFile, ModUpdateInfo, PlayerSample, RemoteServerLink, ResourcePackFile } from "../types";

interface Props {
  link: RemoteServerLink;
  onRemove: (id: string) => void;
  onLinkUpdated: (link: RemoteServerLink) => void;
  spaciousView: boolean;
}

type Tab = "mods" | "resourcepacks" | "console" | "players";

/// A parallel, remote-only detail view rather than a "local vs. remote"
/// branch threaded through InstanceDetail/ServerConsolePanel/PlayersPanel -
/// those files are heavily iterated on for the local case, and this way a
/// remote-admin bug can't destabilize the local Start/Stop/console flow (or
/// vice versa). Talks entirely through `remoteApi` (plain fetch/WebSocket to
/// the host), not Tauri IPC - see remote_api.rs for the endpoints this calls.
export function RemoteInstanceDetail({ link, onRemove, onLinkUpdated, spaciousView }: Props) {
  function onTokenRefreshed(token: string) {
    onLinkUpdated({ ...link, token });
  }
  const api = remoteApi(link, onTokenRefreshed);
  const [tab, setTab] = useState<Tab>("console");
  const [iconUrl, setIconUrl] = useState<string | null>(null);
  const [running, setRunning] = useState<boolean | null>(null);
  const [players, setPlayers] = useState<{ online: number; max: number; sample: PlayerSample[] } | null>(null);
  const [stats, setStats] = useState<{
    cpuPercent: number;
    memoryMb: number;
    memoryLimitMb: number;
    systemUsedMemoryMb: number;
    systemTotalMemoryMb: number;
    tps: number | null;
    mspt: number | null;
  } | null>(null);
  const [actionBusy, setActionBusy] = useState<"start" | "stop" | "restart" | "kill" | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .getIcon()
      .then((info) => !cancelled && setIconUrl(info.iconUrl))
      .catch(() => !cancelled && setIconUrl(null));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [link.id]);

  // `/status` is the same tracked-pid source the host's own local UI trusts
  // for its running badge - accurate regardless of whether the host's
  // console link is available right now. `/players` is a separate,
  // best-effort call (it needs that console link) polled independently so a
  // stale console after a host rebuild only blanks the player list, instead
  // of wrongly showing the whole server as stopped.
  useEffect(() => {
    let cancelled = false;
    function poll() {
      api
        .getStatus()
        .then((status) => !cancelled && setRunning(status.running))
        .catch(() => !cancelled && setRunning(false));
    }
    poll();
    const interval = setInterval(poll, 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [link.id]);

  useEffect(() => {
    if (!running) {
      setPlayers(null);
      return;
    }
    let cancelled = false;
    function poll() {
      api
        .getPlayers()
        .then((status) => {
          if (!cancelled) setPlayers({ online: status.online ?? 0, max: status.max ?? 0, sample: status.sample });
        })
        .catch(() => !cancelled && setPlayers(null));
    }
    poll();
    const interval = setInterval(poll, 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [link.id, running]);

  // Same source as the local running card (CPU/memory via sysinfo, TPS/MSPT
  // via the console `time query gametime` trick) - see remote_api.rs's
  // `/stats`. Best-effort like `/players`: a stale console link after a host
  // rebuild just blanks TPS/MSPT within the response rather than failing the
  // whole poll, since CPU/memory don't need console access at all.
  useEffect(() => {
    if (!running) {
      setStats(null);
      return;
    }
    let cancelled = false;
    function poll() {
      api
        .getStats()
        .then((s) => !cancelled && setStats(s))
        .catch(() => !cancelled && setStats(null));
    }
    poll();
    const interval = setInterval(poll, 3000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [link.id, running]);

  async function runAction(action: "start" | "stop" | "restart" | "kill") {
    setActionBusy(action);
    setActionError(null);
    try {
      if (action === "start") await api.start();
      else if (action === "stop") await api.stop();
      else if (action === "kill") await api.kill();
      else await api.restart();
    } catch (e) {
      setActionError(String(e));
    } finally {
      setActionBusy(null);
    }
  }

  return (
    <div className="main-panel">
      <div className="instance-header">
        {iconUrl ? (
          <img src={iconUrl} className="instance-header-icon" alt="" />
        ) : (
          <div
            className="instance-header-icon"
            style={{ display: "flex", alignItems: "center", justifyContent: "center", fontWeight: 700 }}
          >
            {link.name.slice(0, 1).toUpperCase()}
          </div>
        )}
        <div className="instance-header-text">
          <h2>{link.name}</h2>
          <div className="meta">
            {link.versionId} · {link.loader} · {link.host}:{link.port}
          </div>
        </div>
        <div className="instance-header-actions">
          <button className="danger-btn" onClick={() => onRemove(link.id)}>
            Disconnect
          </button>
          {running ? (
            <>
              <button className="restart-btn" onClick={() => runAction("restart")} disabled={!!actionBusy}>
                {actionBusy === "restart" ? "Restarting…" : "Restart"}
              </button>
              <button className="stop-btn" onClick={() => runAction("stop")} disabled={!!actionBusy}>
                {actionBusy === "stop" ? "Stopping…" : "Stop"}
              </button>
              <button className="danger-btn" onClick={() => runAction("kill")} disabled={!!actionBusy}>
                {actionBusy === "kill" ? "Killing…" : "Kill"}
              </button>
            </>
          ) : (
            <button className="play-btn" onClick={() => runAction("start")} disabled={!!actionBusy}>
              {actionBusy === "start" ? "Working…" : "Start"}
            </button>
          )}
        </div>
      </div>

      <div className={spaciousView ? "instance-body-spacious" : "instance-body"}>
        {running != null && (
          <div className="progress-card">
            <div className="progress-card-row">
              <div className="progress-card-main">
                <div className="stage">{running ? "running" : "stopped"}</div>
                <div>{running ? "Server is running" : "Server is stopped"}</div>
              </div>
              {running && (stats || players) && (
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
                            <div className="server-resource-bar-fill" style={{ width: `${Math.min(100, stats.cpuPercent)}%` }} />
                          </div>
                        </div>
                      </div>
                    )}
                    {players && (
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
                              {players.online} / {players.max}
                            </span>
                          </div>
                          <div className="server-resource-bar-track">
                            <div
                              className="server-resource-bar-fill"
                              style={{ width: `${players.max > 0 ? Math.min(100, (players.online / players.max) * 100) : 0}%` }}
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
                              {stats.memoryMb} / {stats.memoryLimitMb} MB
                            </span>
                          </div>
                          <div className="server-resource-bar-track">
                            <div
                              className="server-resource-bar-fill"
                              style={{ width: `${Math.min(100, (stats.memoryMb / stats.memoryLimitMb) * 100)}%` }}
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
                              style={{ width: `${Math.min(100, (stats.systemUsedMemoryMb / stats.systemTotalMemoryMb) * 100)}%` }}
                            />
                          </div>
                        </div>
                      </div>
                    </div>
                  )}
                  {stats?.tps != null && stats?.mspt != null && (
                    <div className="server-resource-column">
                      <div className="server-resource-stat">
                        <svg className="server-resource-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6">
                          <path d="M3 13h3.5l1.8-5 3 10 2.2-9 1.5 4H21" />
                        </svg>
                        <div className="server-resource-info">
                          <div className="server-resource-line">
                            <span className="server-resource-label">TPS</span>
                            <span className="server-resource-value">{stats.tps.toFixed(1)} / 20</span>
                          </div>
                          <div className="server-resource-bar-track">
                            <div className="server-resource-bar-fill" style={{ width: `${Math.min(100, (stats.tps / 20) * 100)}%` }} />
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
                            <span className="server-resource-value">{stats.mspt.toFixed(1)} ms</span>
                          </div>
                        </div>
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
            {actionError && <div className="error-text">{actionError}</div>}
          </div>
        )}
  
        <div className="mods-panel">
          <div className="files-tab-bar">
            <button className={`files-tab${tab === "mods" ? " active" : ""}`} onClick={() => setTab("mods")}>
              Mods
            </button>
            <button
              className={`files-tab${tab === "resourcepacks" ? " active" : ""}`}
              onClick={() => setTab("resourcepacks")}
            >
              Resource Packs
            </button>
            <button className={`files-tab${tab === "console" ? " active" : ""}`} onClick={() => setTab("console")}>
              Console
            </button>
            <button className={`files-tab${tab === "players" ? " active" : ""}`} onClick={() => setTab("players")}>
              Players
            </button>
          </div>
          {tab === "mods" && <RemoteModsTab link={link} onTokenRefreshed={onTokenRefreshed} />}
          {tab === "resourcepacks" && <RemoteResourcePacksTab link={link} onTokenRefreshed={onTokenRefreshed} />}
          {tab === "console" && (
            <RemoteConsoleTab link={link} isRunning={!!running} onTokenRefreshed={onTokenRefreshed} />
          )}
          {tab === "players" && <RemotePlayersTab players={players} running={running} />}
        </div>
      </div>
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function RemoteModsTab({ link, onTokenRefreshed }: { link: RemoteServerLink; onTokenRefreshed: (token: string) => void }) {
  const api = remoteApi(link, onTokenRefreshed);
  const [mods, setMods] = useState<ModFile[]>([]);
  const [updates, setUpdates] = useState<Record<string, ModUpdateInfo>>({});
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updating, setUpdating] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [showBrowse, setShowBrowse] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  function load() {
    setLoading(true);
    api
      .listMods()
      .then((list) => {
        setMods(list);
        setError(null);
        checkUpdates();
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }

  function checkUpdates() {
    setCheckingUpdates(true);
    api
      .checkModUpdates()
      .then((list) => {
        const byFile: Record<string, ModUpdateInfo> = {};
        for (const info of list) byFile[info.fileName] = info;
        setUpdates(byFile);
      })
      .catch(() => {
        // Non-fatal - the mod list still works without Modrinth reachable.
      })
      .finally(() => setCheckingUpdates(false));
  }

  useEffect(load, [link.id]);

  async function handleUpload(file: File) {
    setUploading(true);
    setError(null);
    try {
      await api.uploadMod(file);
      load();
    } catch (e) {
      setError(String(e));
    } finally {
      setUploading(false);
    }
  }

  async function handleDelete(fileName: string) {
    try {
      await api.deleteMod(fileName);
      load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleToggle(fileName: string, enabled: boolean) {
    setMods((prev) => prev.map((m) => (m.fileName === fileName ? { ...m, fileName, enabled } : m)));
    try {
      const result = await api.toggleMod(fileName, enabled);
      setMods((prev) => prev.map((m) => (m.fileName === fileName ? { ...m, fileName: result.fileName } : m)));
    } catch (e) {
      setError(String(e));
      load();
    }
  }

  async function handleUpdate(info: ModUpdateInfo) {
    if (!info.downloadUrl) return;
    setUpdating((prev) => new Set(prev).add(info.fileName));
    try {
      await api.applyModUpdate(info.fileName, info.downloadUrl);
      load();
    } catch (e) {
      setError(String(e));
    } finally {
      setUpdating((prev) => {
        const next = new Set(prev);
        next.delete(info.fileName);
        return next;
      });
    }
  }

  const updateCount = Object.values(updates).filter((u) => u.updateAvailable).length;

  return (
    <>
      <div className="panel-header">
        <h4>Mods{mods.length > 0 ? ` (${mods.length})` : ""}</h4>
        {checkingUpdates && <span className="hint-inline">Checking for updates…</span>}
        {!checkingUpdates && updateCount > 0 && (
          <span className="hint-inline update-count">
            {updateCount} update{updateCount === 1 ? "" : "s"} available
          </span>
        )}
        <div className="panel-actions">
          <button className="primary-btn small" onClick={() => setShowBrowse(true)}>
            Browse Mods
          </button>
          <button className="ghost-btn small" onClick={load}>
            Refresh
          </button>
          <button className="ghost-btn small" onClick={() => fileInputRef.current?.click()} disabled={uploading}>
            {uploading ? "Uploading…" : "Upload .jar"}
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".jar"
            hidden
            onChange={(e) => {
              const file = e.target.files?.[0];
              e.target.value = "";
              if (file) handleUpload(file);
            }}
          />
        </div>
      </div>
      <div className="mods-list">
        {loading && <div className="placeholder">Loading…</div>}
        {!loading && error && <div className="error-text">{error}</div>}
        {!loading && !error && mods.length === 0 && <div className="placeholder">No mods installed.</div>}
        {!loading &&
          !error &&
          mods.map((m) => {
            const update = updates[m.fileName];
            const isUpdating = updating.has(m.fileName);
            return (
              <div key={m.fileName} className={`mod-row${m.enabled ? "" : " disabled"}`} style={{ cursor: "default" }}>
                <label className="mod-toggle-wrap">
                  <input
                    type="checkbox"
                    className="mod-toggle"
                    checked={m.enabled}
                    title={m.enabled ? "Disable mod" : "Enable mod"}
                    onChange={(e) => handleToggle(m.fileName, e.target.checked)}
                  />
                </label>
                {update?.iconUrl ? (
                  <img src={update.iconUrl} className="mod-row-icon" alt="" />
                ) : (
                  <div className="mod-row-icon placeholder-icon" />
                )}
                <div className="mod-name-block">
                  <span className="mod-name">{update?.title ?? m.fileName}</span>
                  {update?.title && <span className="mod-filename">{m.fileName}</span>}
                </div>
                {update?.updateAvailable && <span className="mod-update-badge">→ {update.latestVersion}</span>}
                <span className="mod-size">{formatSize(m.size)}</span>
                {update?.updateAvailable && (
                  <button className="ghost-btn small" disabled={isUpdating} onClick={() => handleUpdate(update)}>
                    {isUpdating ? <span className="spinner-small" /> : "Update"}
                  </button>
                )}
                <button className="icon-btn" title="Delete" onClick={() => handleDelete(m.fileName)}>
                  ✕
                </button>
              </div>
            );
          })}
      </div>

      {showBrowse && (
        <RemoteBrowseModsDialog
          link={link}
          onTokenRefreshed={onTokenRefreshed}
          onClose={() => setShowBrowse(false)}
          onInstalled={load}
        />
      )}
    </>
  );
}

function RemoteResourcePacksTab({
  link,
  onTokenRefreshed,
}: {
  link: RemoteServerLink;
  onTokenRefreshed: (token: string) => void;
}) {
  const api = remoteApi(link, onTokenRefreshed);
  const [packs, setPacks] = useState<ResourcePackFile[]>([]);
  const [updates, setUpdates] = useState<Record<string, ModUpdateInfo>>({});
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updating, setUpdating] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);
  const [showBrowse, setShowBrowse] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  function load() {
    setLoading(true);
    api
      .listResourcePacks()
      .then((list) => {
        setPacks(list);
        setError(null);
        checkUpdates();
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }

  function checkUpdates() {
    setCheckingUpdates(true);
    api
      .checkResourcePackUpdates()
      .then((list) => {
        const byFile: Record<string, ModUpdateInfo> = {};
        for (const info of list) byFile[info.fileName] = info;
        setUpdates(byFile);
      })
      .catch(() => {
        // Non-fatal - the pack list still works without Modrinth reachable.
      })
      .finally(() => setCheckingUpdates(false));
  }

  useEffect(load, [link.id]);

  async function handleUpload(file: File) {
    setUploading(true);
    setError(null);
    try {
      await api.uploadResourcePack(file);
      load();
    } catch (e) {
      setError(String(e));
    } finally {
      setUploading(false);
    }
  }

  async function handleDelete(fileName: string) {
    try {
      await api.deleteResourcePack(fileName);
      load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleToggle(fileName: string, enabled: boolean) {
    setPacks((prev) => prev.map((p) => (p.fileName === fileName ? { ...p, enabled } : p)));
    try {
      await api.toggleResourcePack(fileName, enabled);
    } catch (e) {
      setError(String(e));
      load();
    }
  }

  async function handleUpdate(info: ModUpdateInfo) {
    if (!info.downloadUrl) return;
    setUpdating((prev) => new Set(prev).add(info.fileName));
    try {
      await api.applyResourcePackUpdate(info.fileName, info.downloadUrl);
      load();
    } catch (e) {
      setError(String(e));
    } finally {
      setUpdating((prev) => {
        const next = new Set(prev);
        next.delete(info.fileName);
        return next;
      });
    }
  }

  const updateCount = Object.values(updates).filter((u) => u.updateAvailable).length;

  return (
    <>
      <div className="panel-header">
        <h4>Resource Packs{packs.length > 0 ? ` (${packs.length})` : ""}</h4>
        {checkingUpdates && <span className="hint-inline">Checking for updates…</span>}
        {!checkingUpdates && updateCount > 0 && (
          <span className="hint-inline update-count">
            {updateCount} update{updateCount === 1 ? "" : "s"} available
          </span>
        )}
        <div className="panel-actions">
          <button className="primary-btn small" onClick={() => setShowBrowse(true)}>
            Browse Resource Packs
          </button>
          <button className="ghost-btn small" onClick={load}>
            Refresh
          </button>
          <button className="ghost-btn small" onClick={() => fileInputRef.current?.click()} disabled={uploading}>
            {uploading ? "Uploading…" : "Upload .zip"}
          </button>
          <input
            ref={fileInputRef}
            type="file"
            accept=".zip"
            hidden
            onChange={(e) => {
              const file = e.target.files?.[0];
              e.target.value = "";
              if (file) handleUpload(file);
            }}
          />
        </div>
      </div>
      <div className="mods-list">
        {loading && <div className="placeholder">Loading…</div>}
        {!loading && error && <div className="error-text">{error}</div>}
        {!loading && !error && packs.length === 0 && <div className="placeholder">No resource packs installed.</div>}
        {!loading &&
          !error &&
          packs.map((p) => {
            const update = updates[p.fileName];
            const isUpdating = updating.has(p.fileName);
            return (
              <div key={p.fileName} className={`mod-row${p.enabled ? "" : " disabled"}`} style={{ cursor: "default" }}>
                <label className="mod-toggle-wrap">
                  <input
                    type="checkbox"
                    className="mod-toggle"
                    checked={p.enabled}
                    title={p.enabled ? "Disable resource pack" : "Enable resource pack"}
                    onChange={(e) => handleToggle(p.fileName, e.target.checked)}
                  />
                </label>
                {update?.iconUrl ? (
                  <img src={update.iconUrl} className="mod-row-icon" alt="" />
                ) : (
                  <div className="mod-row-icon placeholder-icon" />
                )}
                <div className="mod-name-block">
                  <span className="mod-name">{update?.title ?? p.fileName}</span>
                  {update?.title && <span className="mod-filename">{p.fileName}</span>}
                </div>
                {update?.updateAvailable && <span className="mod-update-badge">→ {update.latestVersion}</span>}
                <span className="mod-size">{formatSize(p.size)}</span>
                {update?.updateAvailable && (
                  <button className="ghost-btn small" disabled={isUpdating} onClick={() => handleUpdate(update)}>
                    {isUpdating ? <span className="spinner-small" /> : "Update"}
                  </button>
                )}
                <button className="icon-btn" title="Delete" onClick={() => handleDelete(p.fileName)}>
                  ✕
                </button>
              </div>
            );
          })}
      </div>

      {showBrowse && (
        <RemoteBrowseResourcePacksDialog
          link={link}
          onTokenRefreshed={onTokenRefreshed}
          onClose={() => setShowBrowse(false)}
          onInstalled={load}
        />
      )}
    </>
  );
}

function RemoteConsoleTab({
  link,
  isRunning,
  onTokenRefreshed,
}: {
  link: RemoteServerLink;
  isRunning: boolean;
  onTokenRefreshed: (token: string) => void;
}) {
  const api = remoteApi(link, onTokenRefreshed);
  const [lines, setLines] = useState<string[]>([]);
  const [copied, setCopied] = useState(false);
  const logRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .getConsoleTail()
      .then((text) => !cancelled && setLines(text ? text.split("\n") : []))
      .catch(() => {});

    const disconnect = api.connectConsole(
      (line) => !cancelled && setLines((prev) => [...prev.slice(-2000), line]),
      () => {},
    );
    return () => {
      cancelled = true;
      disconnect();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [link.id]);

  useEffect(() => {
    if (logRef.current) logRef.current.scrollTop = logRef.current.scrollHeight;
  }, [lines]);

  async function handleCopy() {
    await navigator.clipboard.writeText(lines.join("\n"));
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <>
      <div className="panel-header">
        <h4>Console</h4>
        <div className="panel-actions">
          <button className="ghost-btn small" onClick={handleCopy} disabled={lines.length === 0}>
            {copied ? "Copied!" : "Copy"}
          </button>
        </div>
      </div>
      <div className="log-console" ref={logRef}>
        {lines.length > 0 ? (
          lines.join("\n")
        ) : (
          <span className="placeholder">
            {isRunning ? "Waiting for console output…" : "Server output will appear here once it's running."}
          </span>
        )}
      </div>
    </>
  );
}

function RemotePlayersTab({
  players,
  running,
}: {
  players: { online: number; max: number; sample: PlayerSample[] } | null;
  running: boolean | null;
}) {
  return (
    <div className="mods-list">
      <div className="panel-header">
        <h4>Online Players{players ? ` (${players.online} / ${players.max})` : ""}</h4>
      </div>
      {!running && <div className="placeholder">Start the server to see who's online.</div>}
      {running && players && players.sample.length === 0 && <div className="placeholder">No players online.</div>}
      {running &&
        players?.sample.map((p) => (
          <div key={p.id} className="mod-row" style={{ cursor: "default" }}>
            <div className="mod-name-block">
              <span className="mod-name">{p.name}</span>
            </div>
          </div>
        ))}
    </div>
  );
}
