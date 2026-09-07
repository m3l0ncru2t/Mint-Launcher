import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { SkinCapeDialog } from "./SkinCapeDialog";
import { BACKGROUND_THEMES } from "../themes";
import type { CustomBackgroundInfo, GameProfile, Instance, Settings } from "../types";

interface Props {
  profile: GameProfile | null;
  settings: Settings;
  onSettingsChange: (settings: Settings) => void;
  onClose: () => void;
  instances: Instance[];
}

export function SettingsDialog({ profile, settings, onSettingsChange, onClose, instances }: Props) {
  const [showSkinCape, setShowSkinCape] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [customBackgrounds, setCustomBackgrounds] = useState<CustomBackgroundInfo[]>([]);
  const [customThumbs, setCustomThumbs] = useState<Record<string, string>>({});
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameDraft, setRenameDraft] = useState("");
  const fileInputRef = useRef<HTMLInputElement>(null);
  const opacitySaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const activeThemeKey = settings.backgroundTheme ?? "default";
  const activePreset = BACKGROUND_THEMES.find((t) => t.id === activeThemeKey);
  const activeOpacity =
    settings.themeOpacity[activeThemeKey] ?? activePreset?.defaultOpacity ?? { sidebar: 0.82, modsPanel: 0.82 };

  useEffect(() => {
    api
      .listCustomBackgrounds()
      .then(async (backgrounds) => {
        setCustomBackgrounds(backgrounds);
        const entries = await Promise.all(
          backgrounds.map(async (b) => [b.id, await api.getCustomBackground(b.id).catch(() => null)] as const),
        );
        setCustomThumbs(Object.fromEntries(entries.filter(([, url]) => url) as [string, string][]));
      })
      .catch(() => {});
  }, []);

  async function selectPreset(id: string) {
    setBusy(true);
    setError(null);
    try {
      await api.setBackgroundTheme(id === "default" ? null : id);
      onSettingsChange({ ...settings, backgroundTheme: id === "default" ? null : id });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function selectCustom(id: string) {
    setBusy(true);
    setError(null);
    try {
      await api.setBackgroundTheme(id);
      onSettingsChange({ ...settings, backgroundTheme: id });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleAddCustomFile(file: File) {
    setBusy(true);
    setError(null);
    try {
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as string);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
      });
      const base64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
      const id = await api.addCustomBackground(base64, "Custom");
      setCustomBackgrounds((prev) => [...prev, { id, name: "Custom" }]);
      setCustomThumbs((prev) => ({ ...prev, [id]: dataUrl }));
      onSettingsChange({ ...settings, backgroundTheme: id });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleRemoveCustom(id: string, e: React.MouseEvent) {
    e.stopPropagation();
    setBusy(true);
    setError(null);
    try {
      await api.removeCustomBackground(id);
      setCustomBackgrounds((prev) => prev.filter((b) => b.id !== id));
      setCustomThumbs((prev) => {
        const next = { ...prev };
        delete next[id];
        return next;
      });
      if (settings.backgroundTheme === id) {
        onSettingsChange({ ...settings, backgroundTheme: null });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function startRename(bg: CustomBackgroundInfo, e: React.MouseEvent) {
    e.stopPropagation();
    setRenamingId(bg.id);
    setRenameDraft(bg.name);
  }

  async function commitRename(id: string) {
    const name = renameDraft.trim();
    setRenamingId(null);
    if (!name) return;
    try {
      await api.renameCustomBackground(id, name);
      setCustomBackgrounds((prev) => prev.map((b) => (b.id === id ? { ...b, name } : b)));
    } catch (e) {
      setError(String(e));
    }
  }

  function handleOpacityChange(kind: "sidebar" | "modsPanel", sliderValue: number) {
    const opacity = 1 - sliderValue / 100;
    const nextEntry = { ...activeOpacity, [kind]: opacity };
    onSettingsChange({
      ...settings,
      themeOpacity: { ...settings.themeOpacity, [activeThemeKey]: nextEntry },
    });
    if (opacitySaveTimer.current) clearTimeout(opacitySaveTimer.current);
    opacitySaveTimer.current = setTimeout(() => {
      api.setThemeOpacity(activeThemeKey, nextEntry.sidebar, nextEntry.modsPanel).catch((e) => setError(String(e)));
    }, 250);
  }

  const sidebarTransparency = Math.round((1 - activeOpacity.sidebar) * 100);
  const modsPanelTransparency = Math.round((1 - activeOpacity.modsPanel) * 100);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>Settings</h3>
        <div className="subtitle">Configure Mint Launcher</div>

        {error && <div className="error-text">{error}</div>}

        <div className="form-field">
          <label>Background</label>
          <div className="theme-swatch-grid">
            {BACKGROUND_THEMES.map((theme) => (
              <div className="theme-swatch-item" key={theme.id}>
                <button
                  type="button"
                  className={`theme-swatch${activeThemeKey === theme.id || (theme.id === "default" && !settings.backgroundTheme) ? " selected" : ""}`}
                  style={{ backgroundImage: theme.css }}
                  disabled={busy}
                  title={theme.label}
                  onClick={() => selectPreset(theme.id)}
                />
                <span className="theme-swatch-label">{theme.label}</span>
              </div>
            ))}
            {customBackgrounds.map((bg) => (
              <div className="theme-swatch-item" key={bg.id}>
                <div className="theme-swatch-wrapper">
                  <button
                    type="button"
                    className={`theme-swatch${activeThemeKey === bg.id ? " selected" : ""}`}
                    style={customThumbs[bg.id] ? { backgroundImage: `url("${customThumbs[bg.id]}")` } : undefined}
                    disabled={busy}
                    title={bg.name}
                    onClick={() => selectCustom(bg.id)}
                  />
                  <button
                    type="button"
                    className="theme-swatch-remove"
                    title="Remove this image"
                    disabled={busy}
                    onClick={(e) => handleRemoveCustom(bg.id, e)}
                  >
                    ✕
                  </button>
                </div>
                {renamingId === bg.id ? (
                  <input
                    type="text"
                    className="theme-swatch-rename-input"
                    autoFocus
                    maxLength={24}
                    value={renameDraft}
                    onChange={(e) => setRenameDraft(e.target.value)}
                    onBlur={() => commitRename(bg.id)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") commitRename(bg.id);
                      if (e.key === "Escape") setRenamingId(null);
                    }}
                  />
                ) : (
                  <span
                    className="theme-swatch-label theme-swatch-label-editable"
                    title="Click to rename"
                    onClick={(e) => startRename(bg, e)}
                  >
                    {bg.name}
                  </span>
                )}
              </div>
            ))}
            <div className="theme-swatch-item">
              <button
                type="button"
                className="theme-swatch theme-swatch-custom"
                disabled={busy}
                title="Add a custom image"
                onClick={() => fileInputRef.current?.click()}
              >
                +
              </button>
              <span className="theme-swatch-label">Add image</span>
            </div>
          </div>
          <input
            ref={fileInputRef}
            type="file"
            accept="image/png,image/jpeg,image/gif,image/webp"
            style={{ display: "none" }}
            onChange={(e) => {
              const file = e.target.files?.[0];
              e.target.value = "";
              if (file) handleAddCustomFile(file);
            }}
          />
        </div>

        <div className="form-field">
          <label>Sidebar transparency</label>
          <input
            type="range"
            min={0}
            max={100}
            step={1}
            value={sidebarTransparency}
            onChange={(e) => handleOpacityChange("sidebar", Number(e.target.value))}
          />
          <div className="hint">Saved per theme - higher lets it show through the sidebar more.</div>
        </div>

        <div className="form-field">
          <label>Addon list transparency</label>
          <input
            type="range"
            min={0}
            max={100}
            step={1}
            value={modsPanelTransparency}
            onChange={(e) => handleOpacityChange("modsPanel", Number(e.target.value))}
          />
          <div className="hint">Saved per theme - higher lets it show through the mods/resource pack list more.</div>
        </div>

        <div className="form-field">
          <label>Instance view</label>
          <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <input
              type="checkbox"
              checked={settings.spaciousInstanceView}
              disabled={busy}
              onChange={async (e) => {
                const enabled = e.target.checked;
                setBusy(true);
                setError(null);
                try {
                  await api.setSpaciousInstanceView(enabled);
                  onSettingsChange({ ...settings, spaciousInstanceView: enabled });
                } catch (err) {
                  setError(String(err));
                } finally {
                  setBusy(false);
                }
              }}
            />
            Spacious layout
          </label>
          <div className="hint">
            Removes the padding around an instance's console/mods area for more room, instead of the framed/boxed
            look. Applies to every instance, local and remote alike.
          </div>
        </div>

        {profile?.userType === "msa" && (
          <div className="form-field">
            <label>Account</label>
            <button type="button" className="ghost-btn" onClick={() => setShowSkinCape(true)}>
              Skin & Cape
            </button>
          </div>
        )}

        <div className="form-field">
          <label>Experimental</label>
          <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <input
              type="checkbox"
              checked={settings.experimentalServerInstances}
              disabled={busy}
              onChange={async (e) => {
                const enabled = e.target.checked;
                setBusy(true);
                setError(null);
                try {
                  await api.setExperimentalServerInstances(enabled);
                  onSettingsChange({ ...settings, experimentalServerInstances: enabled });
                } catch (err) {
                  setError(String(err));
                } finally {
                  setBusy(false);
                }
              }}
            />
            Server instances
          </label>
          <div className="hint">
            Adds "+ New Server" and "Import Server" to the sidebar, for hosting a dedicated server from this
            machine alongside your regular instances.
          </div>

          <label style={{ display: "inline-flex", alignItems: "center", gap: 6, marginTop: 10 }}>
            <input
              type="checkbox"
              checked={settings.experimentalConfigsLogsTabs}
              disabled={busy}
              onChange={async (e) => {
                const enabled = e.target.checked;
                setBusy(true);
                setError(null);
                try {
                  await api.setExperimentalConfigsLogsTabs(enabled);
                  onSettingsChange({ ...settings, experimentalConfigsLogsTabs: enabled });
                } catch (err) {
                  setError(String(err));
                } finally {
                  setBusy(false);
                }
              }}
            />
            Configs and Logs tabs
          </label>
          <div className="hint">
            Adds "Configs" and "Logs" tabs to each instance, for browsing/editing mod config files and past game
            logs directly.
          </div>
        </div>

        {settings.experimentalServerInstances && (
          <div className="form-field">
            <label>Remote Admin</label>
            <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
              <input
                type="checkbox"
                checked={settings.remoteAdminEnabled}
                disabled={busy || !settings.remoteAdminInstanceId}
                onChange={async (e) => {
                  const enabled = e.target.checked;
                  setBusy(true);
                  setError(null);
                  try {
                    await api.setRemoteAdminEnabled(enabled);
                    onSettingsChange({ ...settings, remoteAdminEnabled: enabled });
                  } catch (err) {
                    setError(String(err));
                  } finally {
                    setBusy(false);
                  }
                }}
              />
              Let other admins connect to this server remotely
            </label>
            <div className="hint">
              Lets a trusted admin's own Mint Launcher connect over a private network (Tailscale, WireGuard - never
              the public internet) to see the console, manage mods, and start/stop/restart. They're let in based on
              being an operator on the server below - no separate password to manage.
            </div>

            <div style={{ marginTop: 10 }}>
              <label htmlFor="remote-admin-instance">Shared server</label>
              <select
                id="remote-admin-instance"
                value={settings.remoteAdminInstanceId ?? ""}
                disabled={busy}
                onChange={async (e) => {
                  const instanceId = e.target.value || null;
                  setBusy(true);
                  setError(null);
                  try {
                    await api.setRemoteAdminInstance(instanceId);
                    onSettingsChange({ ...settings, remoteAdminInstanceId: instanceId });
                  } catch (err) {
                    setError(String(err));
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                <option value="">Select a server…</option>
                {instances
                  .filter((i) => i.kind === "server")
                  .map((i) => (
                    <option key={i.id} value={i.id}>
                      {i.name}
                    </option>
                  ))}
              </select>
            </div>

            <div style={{ marginTop: 10 }}>
              <label htmlFor="remote-admin-port">Port</label>
              <input
                id="remote-admin-port"
                type="number"
                value={settings.remoteAdminPort}
                disabled={busy}
                onChange={async (e) => {
                  const port = Number(e.target.value);
                  if (!Number.isInteger(port) || port <= 0 || port > 65535) return;
                  setBusy(true);
                  setError(null);
                  try {
                    await api.setRemoteAdminPort(port);
                    onSettingsChange({ ...settings, remoteAdminPort: port });
                  } catch (err) {
                    setError(String(err));
                  } finally {
                    setBusy(false);
                  }
                }}
              />
              <div className="hint">
                Give admins this machine's Tailscale/VPN address and this port to connect with - never forward this
                port on your router.
              </div>
            </div>
          </div>
        )}

        <div className="modal-actions">
          <button className="primary-btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>

      {showSkinCape && profile && (
        <SkinCapeDialog uuid={profile.uuid} onClose={() => setShowSkinCape(false)} />
      )}
    </div>
  );
}
