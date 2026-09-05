import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { Select } from "./Select";
import type { AccountSummary, FabricLoaderInfo, Instance, VersionManifestEntry } from "../types";

interface Props {
  instance: Instance;
  onClose: () => void;
  onSaved: (instance: Instance) => void;
  onIconChanged: (instance: Instance) => void;
}

export function InstanceSettingsDialog({ instance, onClose, onSaved, onIconChanged }: Props) {
  const [name, setName] = useState(instance.name);
  const [memoryMb, setMemoryMb] = useState(instance.memoryMb);
  const [javaArgs, setJavaArgs] = useState(instance.javaArgs ?? "");
  const [accountId, setAccountId] = useState(instance.accountId ?? "");
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [iconPreview, setIconPreview] = useState<string | null>(null);
  const [iconBusy, setIconBusy] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const [fabricVersions, setFabricVersions] = useState<FabricLoaderInfo[]>([]);
  const [fabricVersion, setFabricVersion] = useState("");
  const [loadingFabric, setLoadingFabric] = useState(false);
  const [showUnstableFabric, setShowUnstableFabric] = useState(false);
  const [upgrading, setUpgrading] = useState(false);

  const [mcVersions, setMcVersions] = useState<VersionManifestEntry[]>([]);
  const [loadingMcVersions, setLoadingMcVersions] = useState(true);
  const [showSnapshots, setShowSnapshots] = useState(false);
  const [targetVersionId, setTargetVersionId] = useState("");
  const [updatingVersion, setUpdatingVersion] = useState(false);

  useEffect(() => {
    api.listAccounts().then(setAccounts).catch(() => {});
  }, []);

  useEffect(() => {
    api
      .getMinecraftVersions()
      .then(setMcVersions)
      .catch((e) => setError(String(e)))
      .finally(() => setLoadingMcVersions(false));
  }, []);

  const currentMcVersion = useMemo(
    () => mcVersions.find((v) => v.id === instance.versionId),
    [mcVersions, instance.versionId],
  );

  const newerMcVersions = useMemo(() => {
    if (!currentMcVersion) return [];
    return mcVersions.filter(
      (v) => v.releaseTime > currentMcVersion.releaseTime && (showSnapshots || v.type === "release"),
    );
  }, [mcVersions, currentMcVersion, showSnapshots]);

  useEffect(() => {
    if (newerMcVersions.length === 0) {
      setTargetVersionId("");
    } else if (!newerMcVersions.some((v) => v.id === targetVersionId)) {
      setTargetVersionId(newerMcVersions[0].id);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [newerMcVersions]);

  async function handleUpdateVersion() {
    if (!targetVersionId) return;
    setUpdatingVersion(true);
    setError(null);
    try {
      const updated = await api.updateInstanceVersion(instance.id, targetVersionId);
      onSaved(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setUpdatingVersion(false);
    }
  }

  useEffect(() => {
    if (instance.loader !== "vanilla") return;
    let cancelled = false;
    setLoadingFabric(true);
    api
      .getFabricLoaderVersions(instance.versionId)
      .then((list) => !cancelled && setFabricVersions(list))
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoadingFabric(false));
    return () => {
      cancelled = true;
    };
  }, [instance.loader, instance.versionId]);

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

  async function handleUpgrade() {
    if (!fabricVersion) return;
    setUpgrading(true);
    setError(null);
    try {
      const updated = await api.upgradeInstanceLoader(instance.id, "fabric", fabricVersion);
      onSaved(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setUpgrading(false);
    }
  }

  useEffect(() => {
    if (!instance.hasIcon) {
      setIconPreview(null);
      return;
    }
    api.getInstanceIcon(instance.id).then(setIconPreview).catch(() => {});
  }, [instance.id, instance.hasIcon]);

  async function handleIconFile(file: File) {
    setIconBusy(true);
    setError(null);
    try {
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as string);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
      });
      const base64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
      const updated = await api.setInstanceIcon(instance.id, base64);
      setIconPreview(dataUrl);
      onIconChanged(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setIconBusy(false);
    }
  }

  async function handleRemoveIcon() {
    setIconBusy(true);
    setError(null);
    try {
      const updated = await api.removeInstanceIcon(instance.id);
      setIconPreview(null);
      onIconChanged(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setIconBusy(false);
    }
  }

  async function handleSave() {
    if (!name.trim()) {
      setError("Instance name can't be empty");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const updated = await api.updateInstanceSettings(
        instance.id,
        name.trim(),
        memoryMb,
        javaArgs.trim() || null,
        accountId || null,
      );
      onSaved(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const accountOptions = [
    { value: "", label: "Whichever account is signed in" },
    ...accounts.map((a) => ({ value: a.id, label: a.username })),
  ];

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <button className="modal-close-btn" title="Close" onClick={onClose}>
          ✕
        </button>
        <h3>Instance Settings</h3>
        <div className="subtitle">{instance.name}</div>

        {error && <div className="error-text">{error}</div>}

        <div className="form-field">
          <label>Icon</label>
          <div className="instance-icon-editor">
            {iconPreview ? (
              <img className="instance-icon-editor-preview" src={iconPreview} alt="" />
            ) : (
              <div className="instance-icon-editor-preview">{instance.name.slice(0, 1).toUpperCase()}</div>
            )}
            <div className="instance-icon-editor-actions">
              <button
                type="button"
                className="ghost-btn small"
                onClick={() => fileInputRef.current?.click()}
                disabled={iconBusy}
              >
                {iconBusy ? "Working…" : "Upload image"}
              </button>
              {iconPreview && (
                <button type="button" className="ghost-btn small" onClick={handleRemoveIcon} disabled={iconBusy}>
                  Remove
                </button>
              )}
            </div>
            <input
              ref={fileInputRef}
              type="file"
              accept="image/png,image/jpeg,image/gif,image/webp"
              style={{ display: "none" }}
              onChange={(e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (file) handleIconFile(file);
              }}
            />
          </div>
        </div>

        <div className="form-field">
          <label>Name</label>
          <input type="text" value={name} onChange={(e) => setName(e.target.value)} maxLength={64} />
        </div>

        <div className="form-field">
          <label>Memory allocation (MB)</label>
          <input
            type="number"
            min={512}
            max={32768}
            step={512}
            value={memoryMb}
            onChange={(e) => setMemoryMb(Number(e.target.value))}
          />
        </div>

        <div className="form-field">
          <label>Extra JVM arguments (optional)</label>
          <input
            type="text"
            placeholder="e.g. -XX:+UseG1GC"
            value={javaArgs}
            onChange={(e) => setJavaArgs(e.target.value)}
          />
        </div>

        <div className="form-field">
          <label>Account</label>
          <Select value={accountId} onChange={setAccountId} options={accountOptions} />
          <div className="hint">
            Always launch this instance as this account, regardless of whichever one is currently
            signed in - handy for running different instances under different accounts.
          </div>
        </div>

        <div className="form-field">
          <label>Minecraft version</label>
          <div className="hint">Currently on {instance.versionId}.</div>
          <Select
            value={targetVersionId}
            onChange={setTargetVersionId}
            disabled={loadingMcVersions || newerMcVersions.length === 0}
            placeholder={loadingMcVersions ? "Loading…" : "Already on the latest version"}
            options={newerMcVersions.map((v) => ({
              value: v.id,
              label: v.type === "release" ? v.id : `${v.id} (${v.type})`,
            }))}
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
          {instance.loader === "fabric" && (
            <div className="hint">Mods may need updating for the new version once you launch.</div>
          )}
          <button
            type="button"
            className="ghost-btn small"
            onClick={handleUpdateVersion}
            disabled={updatingVersion || !targetVersionId}
          >
            {updatingVersion ? "Updating…" : "Update version"}
          </button>
        </div>

        {instance.loader === "vanilla" && (
          <div className="form-field">
            <label>Mod loader</label>
            <div className="hint">This is a Vanilla instance. Upgrading to Fabric can't be undone here.</div>
            <Select
              value={fabricVersion}
              onChange={setFabricVersion}
              disabled={loadingFabric || visibleFabricVersions.length === 0}
              placeholder={loadingFabric ? "Loading…" : "No Fabric builds available"}
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
            <button
              type="button"
              className="ghost-btn small"
              onClick={handleUpgrade}
              disabled={upgrading || !fabricVersion}
            >
              {upgrading ? "Upgrading…" : "Upgrade to Fabric"}
            </button>
          </div>
        )}

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Cancel
          </button>
          <button className="primary-btn" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
