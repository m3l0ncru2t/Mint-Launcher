import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { SkinCapeDialog } from "./SkinCapeDialog";
import { BACKGROUND_THEMES } from "../themes";
import type { CustomBackgroundInfo, GameProfile, Settings } from "../types";

interface Props {
  profile: GameProfile | null;
  settings: Settings;
  onSettingsChange: (settings: Settings) => void;
  onClose: () => void;
}

export function SettingsDialog({ profile, settings, onSettingsChange, onClose }: Props) {
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

        {profile?.userType === "msa" && (
          <div className="form-field">
            <label>Account</label>
            <button type="button" className="ghost-btn" onClick={() => setShowSkinCape(true)}>
              Skin & Cape
            </button>
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
