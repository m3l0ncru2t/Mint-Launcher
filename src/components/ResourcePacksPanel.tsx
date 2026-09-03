import { useEffect, useRef, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import { BrowseResourcePacksDialog } from "./BrowseResourcePacksDialog";
import { ConfirmDialog } from "./ConfirmDialog";
import { ResourcePackInfoDialog } from "./ResourcePackInfoDialog";
import type { ModUpdateInfo, ResourcePackFile } from "../types";

interface Props {
  instanceId: string;
}

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ResourcePacksPanel({ instanceId }: Props) {
  const [packs, setPacks] = useState<ResourcePackFile[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const loadingRef = useRef(false);
  const pendingLoadRef = useRef<{ showLoading: boolean; thenCheckUpdates: boolean } | null>(null);

  const [updates, setUpdates] = useState<Record<string, ModUpdateInfo>>({});
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updating, setUpdating] = useState<Set<string>>(new Set());
  const [showBrowse, setShowBrowse] = useState(false);
  const [infoFile, setInfoFile] = useState<string | null>(null);
  const [confirmDeleteFile, setConfirmDeleteFile] = useState<string | null>(null);

  function load(showLoading: boolean, thenCheckUpdates: boolean) {
    if (loadingRef.current) {
      // Toggling several packs in quick succession can fire more reloads
      // than can run at once - rather than silently dropping the extra ones
      // (which left `updates` out of sync, hiding icons until a manual
      // refresh), remember the strongest request and run it once the
      // in-flight load finishes.
      const prev = pendingLoadRef.current;
      pendingLoadRef.current = {
        showLoading: showLoading || (prev?.showLoading ?? false),
        thenCheckUpdates: thenCheckUpdates || (prev?.thenCheckUpdates ?? false),
      };
      return;
    }
    loadingRef.current = true;
    if (showLoading) setLoading(true);
    api
      .listResourcepacks(instanceId)
      .then((list) => {
        setPacks(list);
        setError(null);
        if (thenCheckUpdates) checkUpdates();
      })
      .catch((e) => setError(String(e)))
      .finally(() => {
        loadingRef.current = false;
        if (showLoading) setLoading(false);
        const pending = pendingLoadRef.current;
        if (pending) {
          pendingLoadRef.current = null;
          load(pending.showLoading, pending.thenCheckUpdates);
        }
      });
  }

  function checkUpdates() {
    setCheckingUpdates(true);
    api
      .checkResourcepackUpdates(instanceId)
      .then((list) => {
        const byFile: Record<string, ModUpdateInfo> = {};
        for (const info of list) byFile[info.fileName] = info;
        setUpdates(byFile);
      })
      .catch(() => {
        // Non-fatal - local resource pack list still works without Modrinth reachable.
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
    setPacks((prev) => prev.map((p) => (p.fileName === fileName ? { ...p, enabled } : p)));
    try {
      await api.toggleResourcepack(instanceId, fileName, enabled);
    } catch (e) {
      setError(String(e));
      load(true, false);
    }
  }

  async function handleDelete(fileName: string) {
    try {
      await api.deleteResourcepack(instanceId, fileName);
      load(true, false);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleUpdate(info: ModUpdateInfo) {
    if (!info.downloadUrl) return;
    setUpdating((prev) => new Set(prev).add(info.fileName));
    try {
      await api.applyResourcepackUpdate(instanceId, info.fileName, info.downloadUrl);
      // showLoading=false - the per-row spinner already shows this update is
      // in progress, so there's no need to flash the whole list to a
      // "Loading…" placeholder just to refresh one row's data.
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

  async function handleOpenFolder() {
    try {
      const dir = await api.getResourcepacksDir(instanceId);
      await openPath(dir);
    } catch (e) {
      setError(String(e));
    }
  }

  const updateCount = Object.values(updates).filter((u) => u.updateAvailable).length;

  return (
    <>
      <div className="panel-header">
        <h4>Resource Packs{packs.length > 0 ? ` (${packs.length})` : ""}</h4>
        {checkingUpdates && <span className="hint-inline">Checking for updates…</span>}
        {!checkingUpdates && updateCount > 0 && (
          <span className="hint-inline update-count">
            {updateCount} update{updateCount === 1 ? "" : "s"} available
          </span>
        )}
        <div className="panel-actions">
          <button className="primary-btn small" onClick={() => setShowBrowse(true)}>
            Browse Resource Packs
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
        {!loading && !error && packs.length === 0 && (
          <div className="placeholder">
            No resource packs installed. Drop .zip files (or unzipped pack folders) into the resourcepacks folder -
            the list updates on its own.
          </div>
        )}
        {!loading &&
          !error &&
          packs.map((p) => {
            const update = updates[p.fileName];
            const isUpdating = updating.has(p.fileName);
            return (
              <div key={p.fileName} className={`mod-row${p.enabled ? "" : " disabled"}`} onClick={() => setInfoFile(p.fileName)}>
                <label className="mod-toggle-wrap" onClick={(e) => e.stopPropagation()}>
                  <input
                    type="checkbox"
                    className="mod-toggle"
                    checked={p.enabled}
                    title={p.enabled ? "Disable resource pack" : "Enable resource pack"}
                    onChange={(e) => handleToggle(p.fileName, e.target.checked)}
                  />
                </label>
                {update?.iconUrl ? (
                  <img src={update.iconUrl} className="mod-row-icon" alt="" />
                ) : (
                  <div className="mod-row-icon placeholder-icon" />
                )}
                <div className="mod-name-block">
                  <span className="mod-name">{update?.title ?? p.fileName}</span>
                  {update?.title && <span className="mod-filename">{p.fileName}</span>}
                </div>
                {update?.updateAvailable && <span className="mod-update-badge">→ {update.latestVersion}</span>}
                <span className="mod-size">{formatSize(p.size)}</span>
                {update?.updateAvailable && (
                  <button
                    className="ghost-btn small"
                    disabled={isUpdating}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleUpdate(update);
                    }}
                  >
                    {isUpdating ? <span className="spinner-small" /> : "Update"}
                  </button>
                )}
                <button
                  className="icon-btn"
                  title="Remove"
                  onClick={(e) => {
                    e.stopPropagation();
                    setConfirmDeleteFile(p.fileName);
                  }}
                >
                  ✕
                </button>
              </div>
            );
          })}
      </div>

      {showBrowse && (
        <BrowseResourcePacksDialog
          instanceId={instanceId}
          onClose={() => setShowBrowse(false)}
          onInstalled={() => load(true, true)}
        />
      )}

      {infoFile && (
        <ResourcePackInfoDialog
          instanceId={instanceId}
          fileName={infoFile}
          enabled={packs.find((p) => p.fileName === infoFile)?.enabled ?? false}
          onClose={() => setInfoFile(null)}
        />
      )}

      {confirmDeleteFile && (
        <ConfirmDialog
          title="Remove resource pack?"
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
