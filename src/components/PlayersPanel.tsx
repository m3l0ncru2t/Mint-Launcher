import { useEffect, useState } from "react";
import { api } from "../api";
import { PlayerAvatar } from "./PlayerAvatar";
import type { BannedPlayerEntry, OpEntry, WhitelistEntry } from "../types";

interface Props {
  instanceId: string;
  isRunning: boolean;
}

interface OnlinePlayer {
  id: string;
  name: string;
}

/** A tiny "type a username, hit Add" row - reused for the whitelist/ops/bans
 * sections below the online-players list, and (exported) by
 * `RemoteInstanceDetail`'s Players tab for the same rows there. */
export function AddByUsername({
  placeholder,
  busy,
  onAdd,
}: {
  placeholder: string;
  busy: boolean;
  onAdd: (username: string) => void;
}) {
  const [value, setValue] = useState("");
  return (
    <div className="server-command-bar">
      <input
        type="text"
        placeholder={placeholder}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && value.trim()) {
            onAdd(value.trim());
            setValue("");
          }
        }}
        disabled={busy}
      />
      <button
        className="ghost-btn small"
        disabled={busy || !value.trim()}
        onClick={() => {
          onAdd(value.trim());
          setValue("");
        }}
      >
        Add
      </button>
    </div>
  );
}

export function PlayersPanel({ instanceId, isRunning }: Props) {
  const [online, setOnline] = useState<OnlinePlayer[] | null>(null);
  const [maxPlayers, setMaxPlayers] = useState<number | null>(null);
  const [ops, setOps] = useState<OpEntry[]>([]);
  const [whitelist, setWhitelist] = useState<WhitelistEntry[]>([]);
  const [bans, setBans] = useState<BannedPlayerEntry[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busyAction, setBusyAction] = useState<string | null>(null);

  function refreshLists() {
    api.getOps(instanceId).then(setOps).catch(() => {});
    api.getWhitelist(instanceId).then(setWhitelist).catch(() => {});
    api.getBannedPlayers(instanceId).then(setBans).catch(() => {});
  }

  useEffect(() => {
    refreshLists();
    const interval = setInterval(refreshLists, 5000);
    return () => clearInterval(interval);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId]);

  // Same approach `InstanceDetail` uses for the running card's player count:
  // prefer sending `list` through the console and reading the response back
  // off the log, since (unlike a network ping) that still works behind a
  // proxy-only protection mod like TCPShield that rejects direct connections
  // to the game port - falling back to the server-list-ping's `sample` field
  // only when console access isn't available (e.g. an adopted server Mint
  // didn't launch itself).
  //
  // `pingGaveUp` latches once that fallback ping fails once, so it isn't
  // retried every 5s forever after console access is lost (e.g. right after
  // a Mint restart) - a mod like TCPShield exists specifically to reject and
  // loudly log that ping, so hammering it on a dead end would just spam the
  // server's own console until the next Stop/Start restores console access.
  useEffect(() => {
    if (!isRunning) {
      setOnline(null);
      setMaxPlayers(null);
      return;
    }
    let cancelled = false;
    let port = "25565";
    let pingGaveUp = false;
    async function poll() {
      try {
        const status = await api.listOnlinePlayers(instanceId);
        pingGaveUp = false;
        if (!cancelled) {
          setOnline(status.sample);
          setMaxPlayers(status.max);
        }
        return;
      } catch {
        // Fall through to the network-ping fallback below.
      }
      if (pingGaveUp) {
        if (!cancelled) setOnline(null);
        return;
      }
      try {
        const status = await api.pingServer(`localhost:${port}`);
        if (!cancelled) {
          setOnline(status.sample);
          setMaxPlayers(status.max);
        }
      } catch {
        pingGaveUp = true;
        if (!cancelled) setOnline(null);
      }
    }
    api
      .getServerProperties(instanceId)
      .then((props) => {
        port = props["server-port"] || "25565";
      })
      .catch(() => {})
      .finally(poll);
    const interval = setInterval(poll, 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, [instanceId, isRunning]);

  async function run(actionKey: string, action: () => Promise<void>) {
    setBusyAction(actionKey);
    setError(null);
    try {
      await action();
      // Console commands (kick/ban/op/...) take a moment to land and update
      // the on-disk json files - a short delay before refetching avoids
      // showing stale state right after the action completes.
      setTimeout(refreshLists, 400);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyAction(null);
    }
  }

  const isOp = (name: string) => ops.some((o) => o.name.toLowerCase() === name.toLowerCase());

  function handleKick(name: string) {
    run(`kick-${name}`, () => api.sendInstanceCommand(instanceId, `kick ${name}`));
  }

  function handleBanOnline(name: string) {
    run(`ban-${name}`, () => api.sendInstanceCommand(instanceId, `ban ${name}`));
  }

  function handleToggleOp(name: string) {
    const key = `op-${name}`;
    if (isOp(name)) {
      run(key, () =>
        isRunning ? api.sendInstanceCommand(instanceId, `deop ${name}`) : api.removeOpEntry(instanceId, name),
      );
    } else {
      run(key, () =>
        isRunning ? api.sendInstanceCommand(instanceId, `op ${name}`) : api.addOpEntry(instanceId, name),
      );
    }
  }

  function handleAddOp(username: string) {
    run(`add-op-${username}`, () =>
      isRunning ? api.sendInstanceCommand(instanceId, `op ${username}`) : api.addOpEntry(instanceId, username),
    );
  }

  function handleRemoveOp(name: string) {
    run(`remove-op-${name}`, () =>
      isRunning ? api.sendInstanceCommand(instanceId, `deop ${name}`) : api.removeOpEntry(instanceId, name),
    );
  }

  function handleAddWhitelist(username: string) {
    run(`add-wl-${username}`, () =>
      isRunning
        ? api.sendInstanceCommand(instanceId, `whitelist add ${username}`)
        : api.addWhitelistEntry(instanceId, username),
    );
  }

  function handleRemoveWhitelist(name: string) {
    run(`remove-wl-${name}`, () =>
      isRunning
        ? api.sendInstanceCommand(instanceId, `whitelist remove ${name}`)
        : api.removeWhitelistEntry(instanceId, name),
    );
  }

  function handleAddBan(username: string) {
    run(`add-ban-${username}`, () =>
      isRunning ? api.sendInstanceCommand(instanceId, `ban ${username}`) : api.addBanEntry(instanceId, username, ""),
    );
  }

  function handleUnban(name: string) {
    run(`unban-${name}`, () =>
      isRunning ? api.sendInstanceCommand(instanceId, `pardon ${name}`) : api.unbanPlayerEntry(instanceId, name),
    );
  }

  return (
    <div className="mods-list players-panel">
      <div className="panel-header">
        <h4>Online Players{online ? ` (${online.length}${maxPlayers != null ? ` / ${maxPlayers}` : ""})` : ""}</h4>
        <div className="panel-actions">
          <button className="ghost-btn small" onClick={refreshLists}>
            Refresh
          </button>
        </div>
      </div>
      {!isRunning && <div className="placeholder">Start the server to see who's online.</div>}
      {isRunning && online != null && online.length === 0 && <div className="placeholder">No players online.</div>}
      {isRunning && online == null && <div className="placeholder">Waiting for a response from the server…</div>}
      {isRunning &&
        online != null &&
        online.map((p) => (
          <div key={p.id} className="mod-row player-row">
            <PlayerAvatar uuid={p.id} username={p.name} className="player-row-avatar" size={24} />
            <div className="mod-name-block">
              <span className="mod-name">{p.name}</span>
              {isOp(p.name) && <span className="mod-filename">Operator</span>}
            </div>
            <div className="player-row-actions">
              <button
                className="ghost-btn small"
                disabled={busyAction === `op-${p.name}`}
                onClick={() => handleToggleOp(p.name)}
              >
                {isOp(p.name) ? "De-op" : "Op"}
              </button>
              <button
                className="ghost-btn small"
                disabled={busyAction === `kick-${p.name}`}
                onClick={() => handleKick(p.name)}
              >
                Kick
              </button>
              <button
                className="danger-btn small"
                disabled={busyAction === `ban-${p.name}`}
                onClick={() => handleBanOnline(p.name)}
              >
                Ban
              </button>
            </div>
          </div>
        ))}

      {error && <div className="error-text">{error}</div>}

      <div className="panel-header players-section-header">
        <h4>Whitelist{whitelist.length > 0 ? ` (${whitelist.length})` : ""}</h4>
      </div>
      <AddByUsername placeholder="Add a username to the whitelist…" busy={!!busyAction} onAdd={handleAddWhitelist} />
      {whitelist.length === 0 && <div className="placeholder">Nobody's whitelisted.</div>}
      {whitelist.map((w) => (
        <div key={w.uuid} className="mod-row player-row">
          <PlayerAvatar uuid={w.uuid} username={w.name} className="player-row-avatar" size={24} />
          <div className="mod-name-block">
            <span className="mod-name">{w.name}</span>
          </div>
          <button
            className="icon-btn"
            title="Remove from whitelist"
            disabled={busyAction === `remove-wl-${w.name}`}
            onClick={() => handleRemoveWhitelist(w.name)}
          >
            ✕
          </button>
        </div>
      ))}

      <div className="panel-header players-section-header">
        <h4>Operators{ops.length > 0 ? ` (${ops.length})` : ""}</h4>
      </div>
      <AddByUsername placeholder="Add a username as operator…" busy={!!busyAction} onAdd={handleAddOp} />
      {ops.length === 0 && <div className="placeholder">No operators.</div>}
      {ops.map((o) => (
        <div key={o.uuid} className="mod-row player-row">
          <PlayerAvatar uuid={o.uuid} username={o.name} className="player-row-avatar" size={24} />
          <div className="mod-name-block">
            <span className="mod-name">{o.name}</span>
            <span className="mod-filename">Level {o.level}</span>
          </div>
          <button
            className="icon-btn"
            title="Remove operator"
            disabled={busyAction === `remove-op-${o.name}`}
            onClick={() => handleRemoveOp(o.name)}
          >
            ✕
          </button>
        </div>
      ))}

      <div className="panel-header players-section-header">
        <h4>Banned Players{bans.length > 0 ? ` (${bans.length})` : ""}</h4>
      </div>
      <AddByUsername placeholder="Ban a username…" busy={!!busyAction} onAdd={handleAddBan} />
      {bans.length === 0 && <div className="placeholder">Nobody's banned.</div>}
      {bans.map((b) => (
        <div key={b.uuid} className="mod-row player-row">
          <PlayerAvatar uuid={b.uuid} username={b.name} className="player-row-avatar" size={24} />
          <div className="mod-name-block">
            <span className="mod-name">{b.name}</span>
            {b.reason && <span className="mod-filename">{b.reason}</span>}
          </div>
          <button
            className="ghost-btn small"
            disabled={busyAction === `unban-${b.name}`}
            onClick={() => handleUnban(b.name)}
          >
            Unban
          </button>
        </div>
      ))}
    </div>
  );
}
