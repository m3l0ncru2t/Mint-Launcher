import { useEffect, useState } from "react";
import { api } from "../api";
import type { Instance, ServerEntry, ServerStatus, TextRun } from "../types";

/** Splits styled MOTD runs into lines on literal newlines - servers use
 * these to lay the MOTD out in two lines with deliberate spacing/centering,
 * which a single wrapped block of text would otherwise flatten away. */
function splitMotdLines(runs: TextRun[]): TextRun[][] {
  const lines: TextRun[][] = [[]];
  for (const run of runs) {
    run.text.split("\n").forEach((part, i) => {
      if (i > 0) lines.push([]);
      if (part.length > 0) lines[lines.length - 1].push({ ...run, text: part });
    });
  }
  return lines;
}

interface Props {
  instance: Instance;
  onClose: () => void;
  onUpdated: () => void;
  onJoin: (address: string) => void;
  joinDisabled: boolean;
}

export function ServersDialog({ instance, onClose, onUpdated, onJoin, joinDisabled }: Props) {
  const [servers, setServers] = useState<ServerEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .listServers(instance.id)
      .then(setServers)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [instance.id]);

  async function persist(next: ServerEntry[]) {
    setSaving(true);
    setError(null);
    try {
      const updated = await api.saveServers(instance.id, next);
      setServers(updated);
      onUpdated();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  function handleAdd() {
    if (!name.trim() || !address.trim()) return;
    persist([...servers, { name: name.trim(), address: address.trim() }]);
    setName("");
    setAddress("");
  }

  function handleRemove(index: number) {
    persist(servers.filter((_, i) => i !== index));
  }

  function handleRename(index: number, newName: string) {
    const trimmed = newName.trim();
    if (!trimmed || trimmed === servers[index].name) return;
    persist(servers.map((s, i) => (i === index ? { ...s, name: trimmed } : s)));
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <button className="modal-close-btn" title="Close" onClick={onClose}>
          ✕
        </button>
        <h3>Servers</h3>
        <div className="subtitle">
          Join connects directly on Minecraft 1.20+. Older versions launch normally - add the
          server yourself once you're in.
        </div>

        {error && <div className="error-text">{error}</div>}

        <div className="server-list">
          {loading && <div className="placeholder">Loading…</div>}
          {!loading && servers.length === 0 && <div className="placeholder">No servers saved yet.</div>}
          {!loading && servers.map((s, i) => (
            <ServerRow
              key={`${s.address}-${i}`}
              server={s}
              joinDisabled={joinDisabled}
              onJoin={() => onJoin(s.address)}
              onRemove={() => handleRemove(i)}
              onRename={(newName) => handleRename(i, newName)}
            />
          ))}
        </div>

        <div className="form-field">
          <label>Add a server</label>
          <input
            type="text"
            placeholder="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          />
        </div>
        <div className="form-field">
          <input
            type="text"
            placeholder="address:port"
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          />
        </div>

        <div className="modal-actions">
          <button className="primary-btn" disabled={saving || !name.trim() || !address.trim()} onClick={handleAdd}>
            Add server
          </button>
        </div>
      </div>
    </div>
  );
}

interface ServerRowProps {
  server: ServerEntry;
  joinDisabled: boolean;
  onJoin: () => void;
  onRemove: () => void;
  onRename: (name: string) => void;
}

function ServerRow({ server, joinDisabled, onJoin, onRemove, onRename }: ServerRowProps) {
  const [status, setStatus] = useState<ServerStatus | null>(null);
  const [pinging, setPinging] = useState(true);
  const [failed, setFailed] = useState(false);
  const [editingName, setEditingName] = useState(false);
  const [draftName, setDraftName] = useState(server.name);

  function commitRename() {
    setEditingName(false);
    onRename(draftName);
  }

  useEffect(() => {
    let cancelled = false;
    setPinging(true);
    setFailed(false);
    setStatus(null);
    api
      .pingServer(server.address)
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      })
      .finally(() => {
        if (!cancelled) setPinging(false);
      });
    return () => {
      cancelled = true;
    };
  }, [server.address]);

  return (
    <div className="server-row">
      <div className="server-favicon">
        {status?.favicon ? <img src={status.favicon} alt="" /> : <div className="server-favicon-placeholder" />}
      </div>
      <div className="server-info">
        {editingName ? (
          <input
            className="server-name-input"
            autoFocus
            value={draftName}
            onChange={(e) => setDraftName(e.target.value)}
            onBlur={commitRename}
            onKeyDown={(e) => {
              if (e.key === "Enter") (e.target as HTMLInputElement).blur();
              if (e.key === "Escape") {
                setDraftName(server.name);
                setEditingName(false);
              }
            }}
          />
        ) : (
          <div
            className="server-name"
            title="Click to rename"
            onClick={() => {
              setDraftName(server.name);
              setEditingName(true);
            }}
          >
            {server.name}
          </div>
        )}
        <div className="server-address">{server.address}</div>
        {pinging && <div className="server-motd hint-inline">Pinging…</div>}
        {!pinging && failed && <div className="server-motd server-offline">Offline or unreachable</div>}
        {!pinging && status && (
          <div className="server-motd">
            {status.motd.length > 0
              ? splitMotdLines(status.motd)
                  .slice(0, 2)
                  .map((line, li) => (
                  <div key={li} className="server-motd-line">
                    {line.map((run, i) => (
                      <span
                        key={i}
                        style={{
                          color: run.color ?? undefined,
                          fontWeight: run.bold ? 700 : undefined,
                          fontStyle: run.italic ? "italic" : undefined,
                          textDecoration:
                            [run.underlined && "underline", run.strikethrough && "line-through"]
                              .filter(Boolean)
                              .join(" ") || undefined,
                        }}
                      >
                        {run.text}
                      </span>
                    ))}
                  </div>
                ))
              : "A Minecraft Server"}
          </div>
        )}
      </div>
      <div className="server-actions">
        <div className="server-actions-buttons">
          <button className="primary-btn small" disabled={joinDisabled} onClick={onJoin}>
            Join
          </button>
          <button className="icon-btn" title="Remove" onClick={onRemove}>
            ✕
          </button>
        </div>
        {status && status.online !== null && status.max !== null && (
          <div className="server-players">
            {status.online}/{status.max} players
          </div>
        )}
      </div>
    </div>
  );
}
