import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import type { ModLoader, ServerDetection } from "../types";

interface Props {
  onClose: () => void;
  onImported: (id: string) => void;
}

const LOADER_LABELS: Record<ModLoader, string> = {
  vanilla: "Vanilla",
  fabric: "Fabric",
  forge: "Forge",
  quilt: "Quilt",
};

/// Unlike ImportExternalDialog (another launcher's install), a bare server
/// folder is imported in place - nothing is copied, and version/mod loader
/// are auto-detected from the folder itself (see detect_server_instance)
/// rather than asked for manually, since they're already implied by
/// whatever's actually installed there.
export function ImportServerDialog({ onClose, onImported }: Props) {
  const [sourcePath, setSourcePath] = useState("");
  const [name, setName] = useState("");
  const [detecting, setDetecting] = useState(false);
  const [detection, setDetection] = useState<ServerDetection | null>(null);
  const [detectError, setDetectError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [eulaAccepted, setEulaAccepted] = useState(false);

  useEffect(() => {
    if (!sourcePath) {
      setDetection(null);
      setDetectError(null);
      return;
    }
    let cancelled = false;
    setDetecting(true);
    setDetection(null);
    setDetectError(null);
    api
      .detectServerInstance(sourcePath)
      .then((d) => !cancelled && setDetection(d))
      .catch((e) => !cancelled && setDetectError(String(e)))
      .finally(() => !cancelled && setDetecting(false));
    return () => {
      cancelled = true;
    };
  }, [sourcePath]);

  async function handleBrowse() {
    const path = await open({ directory: true, multiple: false });
    if (!path) return;
    setSourcePath(path);
    if (!name.trim()) {
      const guess = path.split(/[\\/]/).filter(Boolean).pop();
      if (guess) setName(guess);
    }
  }

  async function handleImport() {
    if (!sourcePath || !name.trim() || !detection || !eulaAccepted) return;
    setImporting(true);
    setError(null);
    try {
      const instance = await api.importServerFolder(sourcePath, name.trim(), eulaAccepted);
      onImported(instance.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
    }
  }

  const canImport = !importing && !!sourcePath && !!name.trim() && !!detection && eulaAccepted;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>Import Server</h3>
        <div className="subtitle">
          Point at an existing dedicated server folder - it's imported in place (nothing is copied), and its
          Minecraft version and mod loader are detected automatically.
        </div>

        <div className="form-field">
          <label>Server folder</label>
          <button className="ghost-btn" style={{ width: "100%" }} onClick={handleBrowse} disabled={importing}>
            {sourcePath || "Browse for a folder…"}
          </button>
        </div>

        <div className="form-field">
          <label>Name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My server"
            disabled={importing}
          />
        </div>

        {sourcePath && (
          <div className="form-field">
            <label>Detected</label>
            {detecting && <div className="hint">Scanning folder…</div>}
            {!detecting && detection && (
              <div className="hint">
                Minecraft {detection.versionId} · {LOADER_LABELS[detection.loader]}
                {detection.loaderVersion ? ` ${detection.loaderVersion}` : ""}
              </div>
            )}
            {!detecting && detectError && <div className="error-text">{detectError}</div>}
          </div>
        )}

        <div className="form-field">
          <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <input
              type="checkbox"
              checked={eulaAccepted}
              onChange={(e) => setEulaAccepted(e.target.checked)}
              disabled={importing}
            />
            I accept the{" "}
            <a href="https://www.minecraft.net/en-us/eula" target="_blank" rel="noreferrer">
              Minecraft EULA
            </a>
          </label>
        </div>

        {error && <div className="error-text">{error}</div>}

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Cancel
          </button>
          <button className="primary-btn" onClick={handleImport} disabled={!canImport}>
            {importing ? "Importing…" : "Import"}
          </button>
        </div>
      </div>
    </div>
  );
}
