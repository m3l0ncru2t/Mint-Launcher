import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { BrowseResourcePacksDialog } from "./BrowseResourcePacksDialog";
import { ConfirmDialog } from "./ConfirmDialog";
import type { ModUpdateInfo, ShaderPackList } from "../types";

interface Props {
  instanceId: string;
}

/** Modrinth project id of Iris, the shader mod for Fabric/Quilt (it pulls
 * in Sodium, its required dependency, on install). */
const IRIS_PROJECT_ID = "YL57xq9U";

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Shader packs, run by Iris - one active at a time (see shaderpacks.rs). */
export function ShaderPacksPanel({ instanceId }: Props) {
  const [list, setList] = useState<ShaderPackList | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const loadingRef = useRef(false);

  const [updates, setUpdates] = useState<Record<string, ModUpdateInfo>>({});
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updating, setUpdating] = useState<Set<string>>(new Set());
  const [showBrowse, setShowBrowse] = useState(false);
  const [installingIris, setInstallingIris] = useState(false);
  const [confirmDeleteFile, setConfirmDeleteFile] = useState<string | null>(null);

  function load(showLoading: boolean, thenCheckUpdates: boolean) {
    if (loadingRef.current) return;
    loadingRef.current = true;
    if (showLoading) setLoading(true);
    api
      .listShaderpacks(instanceId)
      .then((l) => {
        setList(l);
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
      .checkShaderpackUpdates(instanceId)
      .then((infos) => {
        const byFile: Record<string, ModUpdateInfo> = {};
        for (const info of infos) byFile[info.fileName] = info;
        setUpdates(byFile);
      })
      .catch(() => {
        // Non-fatal - the local list still works without Modrinth reachable.
      })
      .finally(() => setCheckingUpdates(false));
  }

  useEffect(() => {
    setUpdates({});
    load(true, true);
    const onFocus = () => load(false, false);
    window.addEventListener("focus", onFocus);
    // Filled in one by one as each pack's check finishes (see
    // `check_shaderpack_updates`), same as resource packs.
    const unlistenChecked = listen<{ instanceId: string; info: ModUpdateInfo }>(
      "shaderpack-update-checked",
      (event) => {
        if (event.payload.instanceId !== instanceId) return;
        const info = event.payload.info;
        setUpdates((prev) => ({ ...prev, [info.fileName]: info }));
      },
    );
    return () => {
      window.removeEventListener("focus", onFocus);
      unlistenChecked.then((f) => f());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId]);

  async function setActive(fileName: string | null) {
    // Only one pack can be active, so this updates every row at once.
    setList((prev) => prev && { ...prev, packs: prev.packs.map((p) => ({ ...p, enabled: p.fileName === fileName })) });
    try {
      await api.setActiveShaderpack(instanceId, fileName);
    } catch (e) {
      setError(String(e));
      load(true, false);
    }
  }

  async function handleDelete(fileName: string) {
    try {
      if (list?.packs.find((p) => p.fileName === fileName)?.enabled) {
        await api.setActiveShaderpack(instanceId, null);
      }
      await api.deleteShaderpack(instanceId, fileName);
      load(true, false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleUpdate(info: ModUpdateInfo) {
    if (!info.downloadUrl) return;
    setUpdating((prev) => new Set(prev).add(info.fileName));
    try {
      await api.applyShaderpackUpdate(instanceId, info.fileName, info.downloadUrl);
      load(false, true);
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

  async function handleInstallIris() {
    setInstallingIris(true);
    try {
      await api.installMod(instanceId, IRIS_PROJECT_ID);
      load(false, false);
    } catch (e) {
      setError(String(e));
    } finally {
      setInstallingIris(false);
    }
  }

  async function handleOpenFolder() {
    try {
      await api.openFolder(await api.getShaderpacksDir(instanceId));
    } catch (e) {
      setError(String(e));
    }
  }

  const packs = list?.packs ?? [];
  const updateCount = Object.values(updates).filter((u) => u.updateAvailable).length;

  return (
    <>
      <div className="panel-header">
        <h4>Shader Packs{packs.length > 0 ? ` (${packs.length})` : ""}</h4>
        {checkingUpdates && <span className="hint-inline">Checking for updates…</span>}
        {!checkingUpdates && updateCount > 0 && (
          <span className="hint-inline update-count">
            {updateCount} update{updateCount === 1 ? "" : "s"} available
          </span>
        )}
        <div className="panel-actions">
          <button className="primary-btn small" onClick={() => setShowBrowse(true)} disabled={!list?.loaderSupported}>
            Browse Shader Packs
          </button>
          <button className="ghost-btn small" onClick={() => load(true, true)}>
            Refresh
          </button>
          <button className="ghost-btn small" onClick={handleOpenFolder}>
            Open folder
          </button>
        </div>
      </div>

      {list && !list.loaderSupported && (
        <div className="shader-notice">
          Shaders need a Fabric or Quilt instance with the Iris mod - this instance's loader can't run them.
        </div>
      )}
      {list && list.loaderSupported && !list.irisInstalled && (
        <div className="shader-notice">
          <span>Shaders need the Iris mod (and Sodium, which it installs with it).</span>
          <button className="primary-btn small" onClick={handleInstallIris} disabled={installingIris}>
            {installingIris ? "Installing…" : "Install Iris"}
          </button>
        </div>
      )}
      {list && list.irisInstalled && (
        <div className="shader-hint">
          One pack runs at a time. Changes apply the next time the game starts - while it's running, switch packs in
          Options → Video Settings → Shader Packs instead.
        </div>
      )}

      <div className="mods-list">
        {loading && <div className="placeholder">Loading…</div>}
        {!loading && error && <div className="error-text">{error}</div>}
        {!loading && !error && packs.length === 0 && (
          <div className="placeholder">
            No shader packs installed. Browse for one, or drop .zip files into the shaderpacks folder.
          </div>
        )}
        {!loading &&
          !error &&
          packs.map((p) => {
            const update = updates[p.fileName];
            const isUpdating = updating.has(p.fileName);
            return (
              <div key={p.fileName} className={`mod-row${p.enabled ? "" : " disabled"}`}>
                <label className="mod-toggle-wrap">
                  <input
                    type="checkbox"
                    className="mod-toggle"
                    checked={p.enabled}
                    title={p.enabled ? "Turn shaders off" : "Use this shader pack"}
                    onChange={(e) => setActive(e.target.checked ? p.fileName : null)}
                  />
                </label>
                {update?.iconUrl ? (
                  <img src={update.iconUrl} className="mod-row-icon" alt="" />
                ) : (
                  <div className="mod-row-icon placeholder-icon">
                    {(update?.title ?? p.fileName).slice(0, 1).toUpperCase()}
                  </div>
                )}
                <div className="mod-name-block">
                  <span className="mod-name">{update?.title ?? p.fileName}</span>
                  {update?.title && <span className="mod-filename">{p.fileName}</span>}
                </div>
                {p.enabled && <span className="shader-active-badge">Active</span>}
                {update?.updateAvailable && <span className="mod-update-badge">→ {update.latestVersion}</span>}
                <span className="mod-size">{formatSize(p.size)}</span>
                {update?.updateAvailable && (
                  <button className="ghost-btn small" disabled={isUpdating} onClick={() => handleUpdate(update)}>
                    {isUpdating ? <span className="spinner-small" /> : "Update"}
                  </button>
                )}
                <button className="icon-btn" title="Remove" onClick={() => setConfirmDeleteFile(p.fileName)}>
                  ✕
                </button>
              </div>
            );
          })}
      </div>

      {showBrowse && (
        <BrowseResourcePacksDialog
          kind="shader"
          instanceId={instanceId}
          onClose={() => setShowBrowse(false)}
          onInstalled={() => load(true, true)}
        />
      )}

      {confirmDeleteFile && (
        <ConfirmDialog
          title="Remove shader pack?"
          message={`Remove ${confirmDeleteFile}?`}
          confirmLabel="Remove"
          danger
          onConfirm={() => {
            handleDelete(confirmDeleteFile);
            setConfirmDeleteFile(null);
          }}
          onCancel={() => setConfirmDeleteFile(null)}
        />
      )}
    </>
  );
}
