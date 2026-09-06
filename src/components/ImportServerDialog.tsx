import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../api";
import { Select } from "./Select";
import type { FabricLoaderInfo, ImportProgressEvent, ModLoader, VersionManifestEntry } from "../types";

interface Props {
  onClose: () => void;
  onImported: (id: string) => void;
}

const LOADERS: { id: ModLoader; label: string; enabled: boolean }[] = [
  { id: "vanilla", label: "Vanilla", enabled: true },
  { id: "fabric", label: "Fabric", enabled: true },
  { id: "forge", label: "Forge", enabled: false },
  { id: "quilt", label: "Quilt", enabled: false },
];

/// Unlike ImportExternalDialog, a bare server folder carries none of the
/// launcher metadata `importer::scan` relies on to auto-detect version/
/// loader - so this asks for them manually, same fields as CreateServerDialog.
export function ImportServerDialog({ onClose, onImported }: Props) {
  const [sourcePath, setSourcePath] = useState("");
  const [name, setName] = useState("");
  const [versions, setVersions] = useState<VersionManifestEntry[]>([]);
  const [versionId, setVersionId] = useState("");
  const [showSnapshots, setShowSnapshots] = useState(false);
  const [loader, setLoader] = useState<ModLoader>("vanilla");
  const [loading, setLoading] = useState(true);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [eulaAccepted, setEulaAccepted] = useState(false);
  const [progress, setProgress] = useState<ImportProgressEvent | null>(null);

  const [fabricVersions, setFabricVersions] = useState<FabricLoaderInfo[]>([]);
  const [fabricVersion, setFabricVersion] = useState("");
  const [loadingFabric, setLoadingFabric] = useState(false);
  const [showUnstableFabric, setShowUnstableFabric] = useState(false);

  useEffect(() => {
    api
      .getMinecraftVersions()
      .then((v) => {
        setVersions(v);
        const firstRelease = v.find((entry) => entry.type === "release");
        if (firstRelease) setVersionId(firstRelease.id);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    const unlisten = listen<ImportProgressEvent>("import-progress", (event) => {
      setProgress(event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const visibleVersions = useMemo(
    () => versions.filter((v) => showSnapshots || v.type === "release"),
    [versions, showSnapshots],
  );

  useEffect(() => {
    if (loader !== "fabric" || !versionId) {
      setFabricVersions([]);
      return;
    }
    let cancelled = false;
    setLoadingFabric(true);
    api
      .getFabricLoaderVersions(versionId)
      .then((list) => !cancelled && setFabricVersions(list))
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoadingFabric(false));
    return () => {
      cancelled = true;
    };
  }, [loader, versionId]);

  const visibleFabricVersions = useMemo(
    () => fabricVersions.filter((v) => showUnstableFabric || v.stable),
    [fabricVersions, showUnstableFabric],
  );

  useEffect(() => {
    if (visibleFabricVersions.length === 0) {
      setFabricVersion("");
    } else if (!visibleFabricVersions.some((v) => v.version === fabricVersion)) {
      setFabricVersion(visibleFabricVersions[0].version);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [visibleFabricVersions]);

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
    if (!sourcePath || !name.trim() || !versionId || !eulaAccepted) return;
    if (loader === "fabric" && !fabricVersion) return;
    setImporting(true);
    setError(null);
    setProgress(null);
    try {
      const instance = await api.importServerFolder(
        sourcePath,
        name.trim(),
        versionId,
        loader,
        loader === "fabric" ? fabricVersion : null,
        eulaAccepted,
      );
      onImported(instance.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setImporting(false);
      setProgress(null);
    }
  }

  const canImport =
    !importing &&
    !!sourcePath &&
    !!name.trim() &&
    !!versionId &&
    eulaAccepted &&
    (loader !== "fabric" || !!fabricVersion);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>Import Server</h3>
        <div className="subtitle">
          Point at an existing dedicated server folder (world, mods, server.properties, etc.) to bring it in as a
          server instance.
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

        <div className="form-field">
          <label>Version</label>
          <Select
            value={versionId}
            onChange={setVersionId}
            disabled={loading || importing}
            options={visibleVersions.map((v) => ({ value: v.id, label: v.id }))}
          />
          <div className="hint">
            <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
              <input
                type="checkbox"
                checked={showSnapshots}
                onChange={(e) => setShowSnapshots(e.target.checked)}
              />
              Show snapshots
            </label>
          </div>
        </div>

        <div className="form-field">
          <label>Mod loader</label>
          <div className="loader-options">
            {LOADERS.map((l) => (
              <button
                key={l.id}
                type="button"
                disabled={!l.enabled || importing}
                className={`loader-option${loader === l.id ? " selected" : ""}`}
                onClick={() => setLoader(l.id)}
              >
                {l.label}
                {!l.enabled && " (soon)"}
              </button>
            ))}
          </div>
        </div>

        {loader === "fabric" && (
          <div className="form-field">
            <label>Fabric loader version</label>
            <Select
              value={fabricVersion}
              onChange={setFabricVersion}
              disabled={loadingFabric || importing || visibleFabricVersions.length === 0}
              placeholder={loadingFabric ? "Loading…" : "No builds available"}
              options={visibleFabricVersions.map((v) => ({
                value: v.version,
                label: v.stable ? v.version : `${v.version} (unstable)`,
              }))}
            />
            <div className="hint">
              <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
                <input
                  type="checkbox"
                  checked={showUnstableFabric}
                  onChange={(e) => setShowUnstableFabric(e.target.checked)}
                />
                Show unstable builds
              </label>
            </div>
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

        {importing && progress && progress.total > 0 && (
          <div className="progress-bar-track">
            <div
              className="progress-bar-fill"
              style={{ width: `${Math.min(100, Math.round((progress.current / progress.total) * 100))}%` }}
            />
          </div>
        )}

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
