import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { Select } from "./Select";
import type { FabricLoaderInfo, ModLoader, VersionManifestEntry } from "../types";

interface Props {
  onClose: () => void;
  onCreated: (id: string) => void;
}

const LOADERS: { id: ModLoader; label: string; enabled: boolean }[] = [
  { id: "vanilla", label: "Vanilla", enabled: true },
  { id: "fabric", label: "Fabric", enabled: true },
  { id: "forge", label: "Forge", enabled: false },
  { id: "quilt", label: "Quilt", enabled: false },
];

export function CreateInstanceDialog({ onClose, onCreated }: Props) {
  const [name, setName] = useState("");
  const [versions, setVersions] = useState<VersionManifestEntry[]>([]);
  const [versionId, setVersionId] = useState("");
  const [showSnapshots, setShowSnapshots] = useState(false);
  const [loader, setLoader] = useState<ModLoader>("vanilla");
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  async function handleCreate() {
    if (!name.trim() || !versionId) return;
    if (loader === "fabric" && !fabricVersion) return;
    setCreating(true);
    setError(null);
    try {
      const instance = await api.createInstance(
        name.trim(),
        versionId,
        loader,
        loader === "fabric" ? fabricVersion : null,
      );
      onCreated(instance.id);
    } catch (e) {
      setError(String(e));
    } finally {
      setCreating(false);
    }
  }

  const canCreate =
    !creating && !!name.trim() && !!versionId && (loader !== "fabric" || !!fabricVersion);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>New Instance</h3>
        <div className="subtitle">A separate world, mods, and settings sandbox.</div>

        <div className="form-field">
          <label>Name</label>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My survival world"
            autoFocus
          />
        </div>

        <div className="form-field">
          <label>Version</label>
          <Select
            value={versionId}
            onChange={setVersionId}
            disabled={loading}
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
                disabled={!l.enabled}
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
              disabled={loadingFabric || visibleFabricVersions.length === 0}
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
            {!loadingFabric && fabricVersions.length === 0 && (
              <div className="hint">No Fabric builds published for this Minecraft version.</div>
            )}
          </div>
        )}

        {error && <div className="error-text">{error}</div>}

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Cancel
          </button>
          <button className="primary-btn" onClick={handleCreate} disabled={!canCreate}>
            {creating ? "Creating…" : "Create"}
          </button>
        </div>
      </div>
    </div>
  );
}
