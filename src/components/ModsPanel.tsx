import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api } from "../api";
import { BrowseModsDialog } from "./BrowseModsDialog";
import { ConfirmDialog } from "./ConfirmDialog";
import { ModInfoDialog } from "./ModInfoDialog";
import type { ModFile, ModUpdateInfo } from "../types";
import { setVisibleInterval } from "../lib/visibleInterval";
import { modKey, normalizeGroupData, type ModGroup, type ModGroupData } from "../lib/modGroups";

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
  const pendingLoadRef = useRef<{ showLoading: boolean; thenCheckUpdates: boolean } | null>(null);

  const [updates, setUpdates] = useState<Record<string, ModUpdateInfo>>({});
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updating, setUpdating] = useState<Set<string>>(new Set());
  const [showBrowse, setShowBrowse] = useState(false);
  const [infoFile, setInfoFile] = useState<string | null>(null);
  const [confirmDeleteFile, setConfirmDeleteFile] = useState<string | null>(null);

  // --- groups ---
  const [groupData, setGroupData] = useState<ModGroupData>({ groups: [], assignments: {} });
  const [newGroupName, setNewGroupName] = useState<string | null>(null);
  const [renamingGroup, setRenamingGroup] = useState<{ id: string; name: string } | null>(null);
  const [confirmDeleteGroup, setConfirmDeleteGroup] = useState<ModGroup | null>(null);

  useEffect(() => {
    setGroupData({ groups: [], assignments: {} });
    api
      .getModGroups(instanceId)
      .then((raw) => setGroupData(normalizeGroupData(raw)))
      .catch(() => {});
  }, [instanceId]);

  function updateGroups(next: ModGroupData) {
    setGroupData(next);
    api.saveModGroups(instanceId, next).catch((e) => setError(String(e)));
  }

  function createGroup() {
    const name = (newGroupName ?? "").trim();
    setNewGroupName(null);
    if (!name) return;
    const id = `g${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
    updateGroups({ ...groupData, groups: [...groupData.groups, { id, name, collapsed: false }] });
  }

  function renameGroup() {
    if (!renamingGroup) return;
    const name = renamingGroup.name.trim();
    setRenamingGroup(null);
    if (!name) return;
    updateGroups({
      ...groupData,
      groups: groupData.groups.map((g) => (g.id === renamingGroup.id ? { ...g, name } : g)),
    });
  }

  function deleteGroup(id: string) {
    const assignments = Object.fromEntries(Object.entries(groupData.assignments).filter(([, g]) => g !== id));
    updateGroups({ groups: groupData.groups.filter((g) => g.id !== id), assignments });
  }

  function toggleCollapsed(id: string) {
    updateGroups({
      ...groupData,
      groups: groupData.groups.map((g) => (g.id === id ? { ...g, collapsed: !g.collapsed } : g)),
    });
  }

  function moveGroup(id: string, delta: number) {
    const groups = [...groupData.groups];
    const i = groups.findIndex((g) => g.id === id);
    const j = i + delta;
    if (i < 0 || j < 0 || j >= groups.length) return;
    [groups[i], groups[j]] = [groups[j], groups[i]];
    updateGroups({ ...groupData, groups });
  }

  function assignMod(fileName: string, groupId: string) {
    const assignments = { ...groupData.assignments };
    if (groupId) assignments[modKey(fileName)] = groupId;
    else delete assignments[modKey(fileName)];
    updateGroups({ ...groupData, assignments });
  }

  async function setGroupEnabled(groupMods: ModFile[], enabled: boolean) {
    for (const m of groupMods) {
      if (m.enabled !== enabled) await handleToggle(m.fileName, enabled);
    }
  }

  function load(showLoading: boolean, thenCheckUpdates: boolean) {
    if (loadingRef.current) {
      // Toggling several mods in quick succession can fire more reloads than
      // can run at once - rather than silently dropping the extra ones
      // (which left the toggled/renamed file out of sync with `updates`,
      // hiding its icon until a manual refresh), remember the strongest
      // request and run it once the in-flight load finishes.
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
      .checkModUpdates(instanceId)
      .then((list) => {
        // The `mod-update-checked` listener below has typically already
        // filled `updates` in with every one of these by the time this
        // resolves - this just re-asserts the complete, authoritative set,
        // a no-op in practice but a safety net for anything a listener
        // missed (e.g. one that (re)subscribed slightly late).
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
    // Backend checks every mod concurrently and emits one of these as each
    // finishes (see `commands::instances::check_mod_updates`), rather than
    // going quiet until every last one is done - lets a big modpack's
    // update badges/icons fill in one by one instead of all appearing at
    // once after a long wait.
    const unlistenChecked = listen<{ instanceId: string; info: ModUpdateInfo }>("mod-update-checked", (event) => {
      if (event.payload.instanceId !== instanceId) return;
      const info = event.payload.info;
      setUpdates((prev) => ({ ...prev, [info.fileName]: info }));
    });
    // A remote admin (see RemoteModsTab) can upload/delete/toggle mods on
    // this same instance from another machine entirely - nothing here would
    // otherwise notice until the window happened to regain focus, so this
    // instance's own mod list could sit stale indefinitely while it's the
    // one actually being looked at. Quiet (no spinner, no Modrinth re-check)
    // since it's just keeping the list itself in sync, not a user action.
    const stopInterval = setVisibleInterval(() => load(false, false), 5000);
    return () => {
      window.removeEventListener("focus", onFocus);
      stopInterval();
      unlistenChecked.then((f) => f());
    };
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
      const dir = await api.getModsDir(instanceId);
      await api.openFolder(dir);
    } catch (e) {
      setError(String(e));
    }
  }

  const updateCount = Object.values(updates).filter((u) => u.updateAvailable).length;
  // A mod Modrinth knows about but has genuinely no build of for this
  // instance's current game version/loader - most relevant right after
  // changing the Minecraft or Fabric version (see InstanceSettingsDialog's
  // post-update scan), where "no update" alone doesn't say whether a mod is
  // fine or just abandoned for the new target.
  const incompatibleCount = Object.values(updates).filter((u) => u.projectId && !u.compatible).length;
  const knownGroups = new Set(groupData.groups.map((g) => g.id));
  const groupOf = (m: ModFile) => {
    const g = groupData.assignments[modKey(m.fileName)];
    return g && knownGroups.has(g) ? g : "";
  };
  const ungrouped = mods.filter((m) => !groupOf(m));
  const primaryMods = ungrouped.filter((m) => !m.isDependency);
  const dependencyMods = ungrouped.filter((m) => m.isDependency);

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
          <div className="mod-row-icon placeholder-icon">{(update?.title ?? m.fileName).slice(0, 1).toUpperCase()}</div>
        )}
        <div className="mod-name-block">
          <span className="mod-name">{update?.title ?? m.fileName}</span>
          {update?.title && <span className="mod-filename">{m.fileName}</span>}
        </div>
        {update?.updateAvailable && <span className="mod-update-badge">→ {update.latestVersion}</span>}
        {update && !update.updateAvailable && update.projectId && !update.compatible && (
          <span className="mod-incompatible-badge" title="No build of this mod is published for the current game version/loader yet">
            Incompatible
          </span>
        )}
        {groupData.groups.length > 0 && (
          <select
            className="mod-group-select"
            value={groupOf(m)}
            title="Move to group"
            onClick={(e) => e.stopPropagation()}
            onChange={(e) => assignMod(m.fileName, e.target.value)}
          >
            <option value="">No group</option>
            {groupData.groups.map((g) => (
              <option key={g.id} value={g.id}>
                {g.name}
              </option>
            ))}
          </select>
        )}
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
            {isUpdating ? <span className="spinner-small" /> : "Update"}
          </button>
        )}
        <button
          className="icon-btn"
          title="Remove"
          onClick={(e) => {
            e.stopPropagation();
            setConfirmDeleteFile(m.fileName);
          }}
        >
          ✕
        </button>
      </div>
    );
  }

  return (
    <>
      <div className="panel-header">
        <h4>Mods{mods.length > 0 ? ` (${mods.length})` : ""}</h4>
        {checkingUpdates && <span className="hint-inline">Checking for updates…</span>}
        {!checkingUpdates && updateCount > 0 && (
          <span className="hint-inline update-count">{updateCount} update{updateCount === 1 ? "" : "s"} available</span>
        )}
        {!checkingUpdates && incompatibleCount > 0 && (
          <span className="hint-inline mod-incompatible-badge">
            {incompatibleCount} incompatible with the current version
          </span>
        )}
        <div className="panel-actions">
          <button className="primary-btn small" onClick={() => setShowBrowse(true)}>
            Browse Mods
          </button>
          <button className="ghost-btn small" onClick={() => setNewGroupName("")} title="Make a group to organise mods into">
            New group
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
        {newGroupName !== null && (
          <div className="mod-group-new">
            <input
              autoFocus
              placeholder="Group name (e.g. Performance, QoL, Visuals)"
              value={newGroupName}
              maxLength={40}
              onChange={(e) => setNewGroupName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") createGroup();
                if (e.key === "Escape") setNewGroupName(null);
              }}
            />
            <button className="primary-btn small" onClick={createGroup}>
              Create
            </button>
            <button className="ghost-btn small" onClick={() => setNewGroupName(null)}>
              Cancel
            </button>
          </div>
        )}
        {!loading &&
          !error &&
          groupData.groups.map((g, gi) => {
            const groupMods = mods.filter((m) => groupOf(m) === g.id);
            const enabledCount = groupMods.filter((m) => m.enabled).length;
            return (
              <div key={g.id} className="mod-group">
                <div className="mod-group-header" onClick={() => toggleCollapsed(g.id)}>
                  <span className="mod-group-caret">{g.collapsed ? "▸" : "▾"}</span>
                  {renamingGroup?.id === g.id ? (
                    <input
                      className="mod-group-rename"
                      autoFocus
                      value={renamingGroup.name}
                      maxLength={40}
                      onClick={(e) => e.stopPropagation()}
                      onChange={(e) => setRenamingGroup({ id: g.id, name: e.target.value })}
                      onBlur={renameGroup}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") renameGroup();
                        if (e.key === "Escape") setRenamingGroup(null);
                      }}
                    />
                  ) : (
                    <span className="mod-group-name">{g.name}</span>
                  )}
                  <span className="mod-group-count">
                    {groupMods.length === 0 ? "empty" : `${enabledCount}/${groupMods.length} on`}
                  </span>
                  <div className="mod-group-actions" onClick={(e) => e.stopPropagation()}>
                    {groupMods.length > 0 && (
                      <button
                        className="ghost-btn small"
                        title={enabledCount === groupMods.length ? "Disable every mod in this group" : "Enable every mod in this group"}
                        onClick={() => setGroupEnabled(groupMods, enabledCount !== groupMods.length)}
                      >
                        {enabledCount === groupMods.length ? "Disable all" : "Enable all"}
                      </button>
                    )}
                    <button className="icon-btn" title="Move up" disabled={gi === 0} onClick={() => moveGroup(g.id, -1)}>
                      ↑
                    </button>
                    <button
                      className="icon-btn"
                      title="Move down"
                      disabled={gi === groupData.groups.length - 1}
                      onClick={() => moveGroup(g.id, 1)}
                    >
                      ↓
                    </button>
                    <button className="icon-btn" title="Rename group" onClick={() => setRenamingGroup({ id: g.id, name: g.name })}>
                      ✎
                    </button>
                    <button className="icon-btn" title="Delete group (its mods stay installed)" onClick={() => setConfirmDeleteGroup(g)}>
                      ✕
                    </button>
                  </div>
                </div>
                {!g.collapsed &&
                  (groupMods.length === 0 ? (
                    <div className="placeholder mod-group-empty">Use the group menu on a mod to move it here.</div>
                  ) : (
                    groupMods.map(renderRow)
                  ))}
              </div>
            );
          })}
        {!loading && !error && groupData.groups.length > 0 && ungrouped.length > 0 && (
          <div className="mods-subheader">Ungrouped</div>
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

      {confirmDeleteGroup && (
        <ConfirmDialog
          title="Delete group?"
          message={`Delete the "${confirmDeleteGroup.name}" group? Its mods stay installed and go back to the main list.`}
          confirmLabel="Delete group"
          danger
          onConfirm={() => {
            deleteGroup(confirmDeleteGroup.id);
            setConfirmDeleteGroup(null);
          }}
          onCancel={() => setConfirmDeleteGroup(null)}
        />
      )}

      {confirmDeleteFile && (
        <ConfirmDialog
          title="Remove mod?"
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
