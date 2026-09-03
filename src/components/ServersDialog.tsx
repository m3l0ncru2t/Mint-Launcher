import { useState } from "react";
import { api } from "../api";
import type { Instance, ServerEntry } from "../types";

interface Props {
  instance: Instance;
  onClose: () => void;
  onUpdated: () => void;
  onJoin: (address: string) => void;
  joinDisabled: boolean;
}

export function ServersDialog({ instance, onClose, onUpdated, onJoin, joinDisabled }: Props) {
  const [servers, setServers] = useState<ServerEntry[]>(instance.servers);
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function persist(next: ServerEntry[]) {
    setSaving(true);
    setError(null);
    try {
      const updated = await api.saveServers(instance.id, next);
      setServers(updated.servers);
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
          {servers.length === 0 && <div className="placeholder">No servers saved yet.</div>}
          {servers.map((s, i) => (
            <div key={`${s.address}-${i}`} className="server-row">
              <div className="server-info">
                <div className="server-name">{s.name}</div>
                <div className="server-address">{s.address}</div>
              </div>
              <button
                className="primary-btn small"
                disabled={joinDisabled}
                onClick={() => onJoin(s.address)}
              >
                Join
              </button>
              <button className="icon-btn" title="Remove" onClick={() => handleRemove(i)}>
                ✕
              </button>
            </div>
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
