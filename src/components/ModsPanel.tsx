import { useEffect, useRef, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { BrowseModsDialog } from "./BrowseModsDialog";
import { ModInfoDialog } from "./ModInfoDialog";
import type { ModFile, ModUpdateInfo } from "../types";

interface Props {
  instanceId: string;
}

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ModsPanel({ instanceId }: Props) {
  const [mods, setMods] = useState<ModFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const loadingRef = useRef(false);

  const [updates, setUpdates] = useState<Record<string, ModUpdateInfo>>({});
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updating, setUpdating] = useState<Set<string>>(new Set());
  const [showBrowse, setShowBrowse] = useState(false);
  const [infoFile, setInfoFile] = useState<string | null>(null);

  function load(showLoading: boolean, thenCheckUpdates: boolean) {
    if (loadingRef.current) return;
    loadingRef.current = true;
    if (showLoading) setLoading(true);
    api
      .listMods(instanceId)
      .then((list) => {
        setMods(list);
        setError(null);
        if (thenCheckUpdates) checkUpdates();
      })
      .catch((e) => setError(String(e)))
      .finally(() => {
        loadingRef.current = false;
        if (showLoading) setLoading(false);
      });
  }

  function checkUpdates() {
    setCheckingUpdates(true);
    api
      .checkModUpdates(instanceId)
      .then((list) => {
        const byFile: Record<string, ModUpdateInfo> = {};
        for (const info of list) byFile[info.fileName] = info;
        setUpdates(byFile);
      })
      .catch(() => {
        // Non-fatal - local mod list still works without Modrinth reachable.
      })
      .finally(() => setCheckingUpdates(false));
  }

  useEffect(() => {
    setUpdates({});
    load(true, true);
    const onFocus = () => load(false, false);
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId]);

  async function handleToggle(fileName: string, enabled: boolean) {
    const newFileName = enabled ? fileName.replace(/\.disabled$/i, "") : `${fileName}.disabled`;

    // Toggling renames the file (adds/removes ".disabled"), which would
    // otherwise orphan its entry in the updates map (keyed by file name) and
    // drop the icon/title until the next check - carry it over locally
    // instead of waiting on a network round-trip.
    setMods((prev) =>
      prev.map((m) => (m.fileName === fileName ? { ...m, fileName: newFileName, enabled } : m)),
    );
    setUpdates((prev) => {
      const info = prev[fileName];
      if (!info) return prev;
      const next = { ...prev };
      delete next[fileName];
      next[newFileName] = { ...info, fileName: newFileName };
      return next;
    });

    try {
      await api.toggleMod(instanceId, fileName, enabled);
      // Only bother re-checking for real updates when enabling - a disabled
      // mod's update availability isn't relevant.
      load(false, enabled);
    } catch (e) {
      setError(String(e));
      load(true, true);
    }
  }

  async function handleDelete(fileName: string) {
    if (!confirm(`Remove ${fileName}?`)) return;
    try {
      await api.deleteMod(instanceId, fileName);
      load(true, false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleUpdate(info: ModUpdateInfo) {
    if (!info.downloadUrl) return;
    setUpdating((prev) => new Set(prev).add(info.fileName));
    try {
      await api.applyModUpdate(instanceId, info.fileName, info.downloadUrl);
      load(true, true);
    } catch (e) {
      setError(String(e));
    } finally {
      setUpdating((prev) => {
        const next = new Set(prev);
        next.delete(info.fileName);
        return next;
      });
    }
  }

  async function handleOpenFolder() {
    try {
      const dir = await api.getModsDir(instanceId);
      await openPath(dir);
    } catch (e) {
      setError(String(e));
    }
  }

  const updateCount = Object.values(updates).filter((u) => u.updateAvailable).length;
  const primaryMods = mods.filter((m) => !m.isDependency);
  const dependencyMods = mods.filter((m) => m.isDependency);

  function renderRow(m: ModFile) {
    const update = updates[m.fileName];
    const isUpdating = updating.has(m.fileName);
    return (
      <div
        key={m.fileName}
        className={`mod-row${m.enabled ? "" : " disabled"}`}
        onClick={() => setInfoFile(m.fileName)}
      >
        <label className="mod-toggle-wrap" onClick={(e) => e.stopPropagation()}>
          <input
            type="checkbox"
            className="mod-toggle"
            checked={m.enabled}
            title={m.enabled ? "Disable mod" : "Enable mod"}
            onChange={(e) => handleToggle(m.fileName, e.target.checked)}
          />
        </label>
        {update?.iconUrl ? (
          <img src={update.iconUrl} className="mod-row-icon" alt="" />
        ) : (
          <div className="mod-row-icon placeholder-icon" />
        )}
        <div className="mod-name-block">
          <span className="mod-name">{update?.title ?? m.fileName}</span>
          {update?.title && <span className="mod-filename">{m.fileName}</span>}
        </div>
        {update?.updateAvailable && <span className="mod-update-badge">→ {update.latestVersion}</span>}
        <span className="mod-size">{formatSize(m.size)}</span>
        {update?.updateAvailable && (
          <button
            className="ghost-btn small"
            disabled={isUpdating}
            onClick={(e) => {
              e.stopPropagation();
              handleUpdate(update);
            }}
          >
            {isUpdating ? "Updating…" : "Update"}
          </button>
        )}
        <button
          className="icon-btn"
          title="Remove"
          onClick={(e) => {
            e.stopPropagation();
            handleDelete(m.fileName);
          }}
        >
          ✕
        </button>
      </div>
    );
  }

  return (
    <div className="mods-panel">
      <div className="panel-header">
        <h4>Mods{mods.length > 0 ? ` (${mods.length})` : ""}</h4>
        {checkingUpdates && <span className="hint-inline">Checking for updates…</span>}
        {!checkingUpdates && updateCount > 0 && (
          <span className="hint-inline update-count">{updateCount} update{updateCount === 1 ? "" : "s"} available</span>
        )}
        <div className="panel-actions">
          <button className="primary-btn small" onClick={() => setShowBrowse(true)}>
            Browse Mods
          </button>
          <button className="ghost-btn small" onClick={() => load(true, true)}>
            Refresh
          </button>
          <button className="ghost-btn small" onClick={handleOpenFolder}>
            Open folder
          </button>
        </div>
      </div>
      <div className="mods-list">
        {loading && <div className="placeholder">Loading…</div>}
        {!loading && error && <div className="error-text">{error}</div>}
        {!loading && !error && mods.length === 0 && (
          <div className="placeholder">
            No mods installed. Drop .jar files into the mods folder - the list updates on its own.
          </div>
        )}
        {!loading && !error && primaryMods.map(renderRow)}
        {!loading && !error && dependencyMods.length > 0 && (
          <>
            <div className="mods-subheader">Dependencies</div>
            {dependencyMods.map(renderRow)}
          </>
        )}
      </div>

      {showBrowse && (
        <BrowseModsDialog
          instanceId={instanceId}
          onClose={() => setShowBrowse(false)}
          onInstalled={() => load(true, true)}
        />
      )}

      {infoFile && (
        <ModInfoDialog instanceId={instanceId} fileName={infoFile} onClose={() => setInfoFile(null)} />
      )}
    </div>
  );
}
