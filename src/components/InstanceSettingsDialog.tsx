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

const DIFFICULTIES = ["peaceful", "easy", "normal", "hard"];
const GAMEMODES = ["survival", "creative", "adventure", "spectator"];

// Mirrors what a freshly-generated server.properties already defaults to, so
// a server that's never been launched yet (no file on disk) still shows
// sensible values instead of a wall of empty fields.
const SERVER_PROPERTY_DEFAULTS: Record<string, string> = {
  motd: "A Minecraft Server",
  "max-players": "20",
  difficulty: "easy",
  gamemode: "survival",
  pvp: "true",
  "online-mode": "true",
  "white-list": "false",
  "server-port": "25565",
  "view-distance": "10",
  "simulation-distance": "10",
  "allow-flight": "false",
  "spawn-protection": "16",
};

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

  const [serverProps, setServerProps] = useState<Record<string, string>>({});
  const [loadingServerProps, setLoadingServerProps] = useState(false);
  const [savingServerProps, setSavingServerProps] = useState(false);
  const [serverPropsError, setServerPropsError] = useState<string | null>(null);

  useEffect(() => {
    if (instance.kind !== "server") return;
    setLoadingServerProps(true);
    api
      .getServerProperties(instance.id)
      .then((loaded) => setServerProps({ ...SERVER_PROPERTY_DEFAULTS, ...loaded }))
      .catch((e) => setServerPropsError(String(e)))
      .finally(() => setLoadingServerProps(false));
  }, [instance.id, instance.kind]);

  function setProp(key: string, value: string) {
    setServerProps((prev) => ({ ...prev, [key]: value }));
  }

  async function handleSaveServerProps() {
    setSavingServerProps(true);
    setServerPropsError(null);
    try {
      await api.saveServerProperties(instance.id, serverProps);
    } catch (e) {
      setServerPropsError(String(e));
    } finally {
      setSavingServerProps(false);
    }
  }

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

        {instance.externalDir && (
          <div className="form-field">
            <label>Linked folder</label>
            <div className="hint">{instance.externalDir}</div>
            <div className="hint">
              This server runs directly from this folder - nothing was copied into Mint, so mods, worlds, and
              settings changes here all land in the original files.
            </div>
          </div>
        )}

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

        {instance.kind === "server" && (
          <div className="form-field">
            <label>Server settings</label>
            {loadingServerProps && <div className="hint">Loading server.properties…</div>}
            {!loadingServerProps && (
              <div className="server-properties-grid">
                <label className="server-property">
                  <span>Message of the day</span>
                  <input
                    type="text"
                    value={serverProps.motd ?? ""}
                    onChange={(e) => setProp("motd", e.target.value)}
                  />
                </label>
                <label className="server-property">
                  <span>Max players</span>
                  <input
                    type="number"
                    min={1}
                    value={serverProps["max-players"] ?? ""}
                    onChange={(e) => setProp("max-players", e.target.value)}
                  />
                </label>
                <label className="server-property">
                  <span>Difficulty</span>
                  <select
                    value={serverProps.difficulty ?? "easy"}
                    onChange={(e) => setProp("difficulty", e.target.value)}
                  >
                    {DIFFICULTIES.map((d) => (
                      <option key={d} value={d}>
                        {d}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="server-property">
                  <span>Default game mode</span>
                  <select
                    value={serverProps.gamemode ?? "survival"}
                    onChange={(e) => setProp("gamemode", e.target.value)}
                  >
                    {GAMEMODES.map((g) => (
                      <option key={g} value={g}>
                        {g}
                      </option>
                    ))}
                  </select>
                </label>
                <label className="server-property">
                  <span>Server port</span>
                  <input
                    type="number"
                    value={serverProps["server-port"] ?? ""}
                    onChange={(e) => setProp("server-port", e.target.value)}
                  />
                </label>
                <label className="server-property">
                  <span>Spawn protection radius</span>
                  <input
                    type="number"
                    min={0}
                    value={serverProps["spawn-protection"] ?? ""}
                    onChange={(e) => setProp("spawn-protection", e.target.value)}
                  />
                </label>
                <label className="server-property">
                  <span>View distance</span>
                  <input
                    type="number"
                    min={3}
                    max={32}
                    value={serverProps["view-distance"] ?? ""}
                    onChange={(e) => setProp("view-distance", e.target.value)}
                  />
                </label>
                <label className="server-property">
                  <span>Simulation distance</span>
                  <input
                    type="number"
                    min={3}
                    max={32}
                    value={serverProps["simulation-distance"] ?? ""}
                    onChange={(e) => setProp("simulation-distance", e.target.value)}
                  />
                </label>
                <label className="server-property-checkbox">
                  <input
                    type="checkbox"
                    checked={serverProps.pvp !== "false"}
                    onChange={(e) => setProp("pvp", String(e.target.checked))}
                  />
                  Allow PvP
                </label>
                <label className="server-property-checkbox">
                  <input
                    type="checkbox"
                    checked={serverProps["allow-flight"] === "true"}
                    onChange={(e) => setProp("allow-flight", String(e.target.checked))}
                  />
                  Allow flight
                </label>
                <label className="server-property-checkbox">
                  <input
                    type="checkbox"
                    checked={serverProps["white-list"] === "true"}
                    onChange={(e) => setProp("white-list", String(e.target.checked))}
                  />
                  Require whitelist
                </label>
                <label className="server-property-checkbox">
                  <input
                    type="checkbox"
                    checked={serverProps["online-mode"] !== "false"}
                    onChange={(e) => setProp("online-mode", String(e.target.checked))}
                  />
                  Verify Minecraft accounts
                </label>
              </div>
            )}
            <div className="hint">
              Turning off "Verify Minecraft accounts" lets anyone connect with any username - only do this on a
              private/offline network, never on a public server. Changes apply the next time this server is
              (re)started.
            </div>
            {serverPropsError && <div className="error-text">{serverPropsError}</div>}
            <button
              type="button"
              className="ghost-btn small"
              onClick={handleSaveServerProps}
              disabled={savingServerProps || loadingServerProps}
            >
              {savingServerProps ? "Saving…" : "Save server settings"}
            </button>
          </div>
        )}

        {instance.kind !== "server" && (
          <div className="form-field">
            <label>Account</label>
            <Select value={accountId} onChange={setAccountId} options={accountOptions} />
            <div className="hint">
              Always launch this instance as this account, regardless of whichever one is currently
              signed in - handy for running different instances under different accounts.
            </div>
          </div>
        )}

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
