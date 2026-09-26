import { useEffect, useState } from "react";
import { api } from "../api";
import { setVisibleInterval } from "../lib/visibleInterval";
import type { ServerInfo } from "../types";

interface Props {
  /** Keys the saved domain - a local instance id or a remote link id. */
  instanceId: string;
  isRunning: boolean;
  /** Where the numbers come from - the local backend or a remote host. */
  load: () => Promise<ServerInfo>;
  /** Remote view: the IPs are the host's, and come back in `load`'s result
   * (or not at all, without full access) instead of being looked up here. */
  remote?: boolean;
}

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  if (h >= 24) return `${Math.floor(h / 24)}d ${h % 24}h`;
  if (h >= 1) return `${h}h ${Math.floor((seconds % 3600) / 60)}m`;
  return `${Math.floor(seconds / 60)}m`;
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${Math.round(bytes / 1024)} KB`;
}

// The machine's public IP is the same for every server tab - looked up once
// per launcher run instead of on every tab open.
let publicIpCache: Promise<string> | null = null;

const domainKey = (id: string) => `mint.serverDomain.${id}`;

function loadDomain(id: string): string {
  try {
    return localStorage.getItem(domainKey(id)) ?? "";
  } catch {
    return "";
  }
}

export function ServerInfoPanel({ instanceId, isRunning, load: loadInfo, remote = false }: Props) {
  const [info, setInfo] = useState<ServerInfo | null>(null);
  const [publicIp, setPublicIp] = useState<string | null>(null);
  const [publicIpError, setPublicIpError] = useState(false);
  const [showPublic, setShowPublic] = useState(true);
  const [domain, setDomain] = useState("");
  const [editingDomain, setEditingDomain] = useState(false);
  const [domainDraft, setDomainDraft] = useState("");
  const [copied, setCopied] = useState<string | null>(null);

  useEffect(() => {
    setInfo(null);
    setPublicIp(null);
    setShowPublic(true);
    setDomain(loadDomain(instanceId));
    setEditingDomain(false);
    if (!remote) lookupPublicIp();
    let cancelled = false;
    const load = () =>
      loadInfo()
        .then((i) => {
          if (cancelled) return;
          setInfo(i);
          if (remote) setPublicIp(i.publicIp ?? null);
        })
        .catch(() => {});
    load();
    const stop = setVisibleInterval(load, 60000);
    return () => {
      cancelled = true;
      stop();
    };
  }, [instanceId]);

  function lookupPublicIp() {
    setPublicIpError(false);
    publicIpCache ??= api.getPublicIp();
    publicIpCache.then(setPublicIp).catch(() => {
      publicIpCache = null;
      setPublicIpError(true);
    });
  }

  // Shown by default; Hide is there for screen-shares/streams.
  function toggleReveal() {
    if (!remote && !showPublic && !publicIp) lookupPublicIp();
    setShowPublic(!showPublic);
  }

  function saveDomain() {
    // Strip a pasted scheme/port/path - only the hostname is kept.
    const cleaned = domainDraft.trim().replace(/^[a-z]+:\/\//i, "").replace(/[:/].*$/, "");
    try {
      if (cleaned) localStorage.setItem(domainKey(instanceId), cleaned);
      else localStorage.removeItem(domainKey(instanceId));
    } catch {
      // Storage unavailable - the domain just won't be remembered.
    }
    setDomain(cleaned);
    setEditingDomain(false);
  }

  function copy(text: string, key: string) {
    navigator.clipboard.writeText(text).then(() => {
      setCopied(key);
      setTimeout(() => setCopied((c) => (c === key ? null : c)), 1500);
    });
  }

  if (!info) return <div className="placeholder">Loading…</div>;

  // A remote admin without full access gets neither address from the host.
  const hiddenByHost = remote && !!info.ipsHidden;
  const publicShown = hiddenByHost
    ? "Full access only"
    : showPublic
      ? publicIp
        ? `${publicIp}:${info.port}`
        : publicIpError || remote
          ? "Lookup failed"
          : "Looking up…"
      : "••••••••••";
  const localShown = info.localIp ? `${info.localIp}:${info.port}` : hiddenByHost ? "Full access only" : "Unknown";

  return (
    <div className="mods-list server-info">
      <div className="panel-header players-section-header">
        <h4>Connect</h4>
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">Local IP (same network)</span>
        <span className="server-info-value">{localShown}</span>
        {info.localIp && (
          <button className="secondary" onClick={() => copy(localShown, "local")}>
            {copied === "local" ? "Copied" : "Copy"}
          </button>
        )}
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">Public IP (internet)</span>
        <span className="server-info-value">{publicShown}</span>
        {!hiddenByHost && (
          <button className="secondary" onClick={toggleReveal}>
            {showPublic ? "Hide" : "Show"}
          </button>
        )}
        {showPublic && publicIp && (
          <button className="secondary" onClick={() => copy(`${publicIp}:${info.port}`, "public")}>
            {copied === "public" ? "Copied" : "Copy"}
          </button>
        )}
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">Domain / DDNS</span>
        {editingDomain ? (
          <>
            <input
              className="server-info-input"
              autoFocus
              value={domainDraft}
              placeholder="play.example.com"
              onChange={(e) => setDomainDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") saveDomain();
                if (e.key === "Escape") setEditingDomain(false);
              }}
            />
            <button className="secondary" onClick={saveDomain}>
              Save
            </button>
          </>
        ) : (
          <>
            <span className="server-info-value">{domain ? (info.port === 25565 ? domain : `${domain}:${info.port}`) : "Not set"}</span>
            <button
              className="secondary"
              onClick={() => {
                setDomainDraft(domain);
                setEditingDomain(true);
              }}
            >
              {domain ? "Edit" : "Set"}
            </button>
            {domain && (
              <button
                className="secondary"
                onClick={() => copy(info.port === 25565 ? domain : `${domain}:${info.port}`, "domain")}
              >
                {copied === "domain" ? "Copied" : "Copy"}
              </button>
            )}
          </>
        )}
      </div>
      <div className="placeholder server-info-hint">
        Friends outside your network need the public IP, and port {info.port} forwarded to this machine on your router.
      </div>

      <div className="panel-header players-section-header">
        <h4>Activity</h4>
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">Status</span>
        <span className="server-info-value">{isRunning ? "Running" : "Stopped"}</span>
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">Total players joined</span>
        <span className="server-info-value">{info.totalPlayersJoined}</span>
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">Total playtime</span>
        <span className="server-info-value">{formatDuration(info.totalPlaytimeSeconds)}</span>
      </div>
      {info.topPlayers.length > 0 && (
        <>
          <div className="panel-header players-section-header">
            <h4>Most playtime</h4>
          </div>
          {info.topPlayers.map((p) => (
            <div key={p.name} className="mod-row server-info-row">
              <span className="server-info-label">{p.name}</span>
              <span className="server-info-value">{formatDuration(p.playtimeSeconds)}</span>
            </div>
          ))}
        </>
      )}

      <div className="panel-header players-section-header">
        <h4>Server</h4>
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">World</span>
        <span className="server-info-value">
          {info.levelName} ({formatBytes(info.worldSizeBytes)})
        </span>
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">Mods</span>
        <span className="server-info-value">{info.modCount}</span>
      </div>
      <div className="mod-row server-info-row">
        <span className="server-info-label">Operators / Whitelisted / Banned</span>
        <span className="server-info-value">
          {info.opCount} / {info.whitelistCount} / {info.banCount}
        </span>
      </div>
    </div>
  );
}
