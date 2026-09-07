import { useState } from "react";
import { api } from "../api";

interface Props {
  onClose: () => void;
  onImported: (id: string) => void;
}

const DEFAULT_PORT = 25580;

/// Connects to another machine's Mint Launcher over the network (a private
/// tunnel like Tailscale, not the public internet) rather than importing a
/// local folder - see remote_connect. Proves identity using the currently
/// signed-in Microsoft account via the same join/hasJoined handshake any
/// vanilla client does when joining a server, so there's no separate
/// password to enter here - only host/port.
export function ImportRemoteServerDialog({ onClose, onImported }: Props) {
  const [host, setHost] = useState("");
  const [port, setPort] = useState(String(DEFAULT_PORT));
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const portNumber = Number(port);
  const canConnect = !connecting && !!host.trim() && Number.isInteger(portNumber) && portNumber > 0 && portNumber < 65536;

  async function handleConnect() {
    if (!canConnect) return;
    setConnecting(true);
    setError(null);
    try {
      const link = await api.remoteConnect(host.trim(), portNumber);
      onImported(link.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setConnecting(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>Import Remote Server</h3>
        <div className="subtitle">
          Connect to a server someone else's Mint Launcher is sharing - reach it over a private network (Tailscale,
          WireGuard, etc.), not the public internet. You'll need to be signed in with the Microsoft account that's an
          operator on that server.
        </div>

        <div className="form-field">
          <label>Host address</label>
          <input
            type="text"
            value={host}
            onChange={(e) => setHost(e.target.value)}
            placeholder="100.x.x.x or a Tailscale hostname"
            disabled={connecting}
          />
        </div>

        <div className="form-field">
          <label>Port</label>
          <input
            type="number"
            value={port}
            onChange={(e) => setPort(e.target.value)}
            placeholder={String(DEFAULT_PORT)}
            disabled={connecting}
          />
        </div>

        {error && <div className="error-text">{error}</div>}

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Cancel
          </button>
          <button className="primary-btn" onClick={handleConnect} disabled={!canConnect}>
            {connecting ? "Connecting…" : "Connect"}
          </button>
        </div>
      </div>
    </div>
  );
}
