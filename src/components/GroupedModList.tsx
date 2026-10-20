import { useEffect, useRef, useState, type ReactNode } from "react";
import { ConfirmDialog } from "./ConfirmDialog";
import { modKey, normalizeGroupData, type ModGroup, type ModGroupData } from "../lib/modGroups";
import { setVisibleInterval } from "../lib/visibleInterval";
import type { ModFile } from "../types";

interface Props {
  /** Changes when a different instance/server is shown - reloads groups. */
  storeKey: string;
  /** Read/write the groups JSON - local Tauri commands or the remote host. */
  loadGroups: () => Promise<unknown>;
  saveGroups: (data: ModGroupData) => Promise<unknown>;
  mods: ModFile[];
  /** Renders one mod row; `groupSelect` is the "move to group" menu (or
   * null when there are no groups yet) for the row to place. */
  renderRow: (m: ModFile, groupSelect: ReactNode) => ReactNode;
  onToggle: (fileName: string, enabled: boolean) => Promise<unknown>;
  /** The panel header's "New group" button sets this; cleared when done. */
  creating: boolean;
  onCreatingDone: () => void;
  onError: (message: string) => void;
  /** A search is narrowing `mods`: hide groups with no matches, and show
   * collapsed groups' matches anyway. */
  filtering?: boolean;
}

/** The mods list split into the user's groups (shared by the local Mods
 * panel and the remote Mods tab), then the ungrouped mods and dependencies.
 * Groups are saved alongside the mods (see get_mod_groups / the host's
 * /mod-groups), so several people can edit them - hence the quiet re-read
 * every so often. */
export function GroupedModList({
  storeKey,
  loadGroups,
  saveGroups,
  mods,
  renderRow,
  onToggle,
  creating,
  onCreatingDone,
  onError,
  filtering = false,
}: Props) {
  const [groupData, setGroupData] = useState<ModGroupData>({ groups: [], assignments: {} });
  const [newGroupName, setNewGroupName] = useState("");
  const [renamingGroup, setRenamingGroup] = useState<{ id: string; name: string } | null>(null);
  const [confirmDeleteGroup, setConfirmDeleteGroup] = useState<ModGroup | null>(null);
  // Bumped on every local change: a background re-read that started before
  // the latest change is stale and must not overwrite it.
  const changeVersion = useRef(0);

  useEffect(() => {
    setGroupData({ groups: [], assignments: {} });
    let cancelled = false;
    const refresh = () => {
      const startedAt = changeVersion.current;
      loadGroups()
        .then((raw) => {
          if (!cancelled && startedAt === changeVersion.current) setGroupData(normalizeGroupData(raw));
        })
        .catch(() => {});
    };
    refresh();
    const stop = setVisibleInterval(refresh, 10000);
    return () => {
      cancelled = true;
      stop();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [storeKey]);

  useEffect(() => {
    if (creating) setNewGroupName("");
  }, [creating]);

  function updateGroups(next: ModGroupData) {
    changeVersion.current++;
    setGroupData(next);
    saveGroups(next).catch((e) => onError(String(e)));
  }

  function createGroup() {
    const name = newGroupName.trim();
    onCreatingDone();
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
      if (m.enabled !== enabled) await onToggle(m.fileName, enabled);
    }
  }

  const knownGroups = new Set(groupData.groups.map((g) => g.id));
  const groupOf = (m: ModFile) => {
    const g = groupData.assignments[modKey(m.fileName)];
    return g && knownGroups.has(g) ? g : "";
  };
  const ungrouped = mods.filter((m) => !groupOf(m));
  const primaryMods = ungrouped.filter((m) => !m.isDependency);
  const dependencyMods = ungrouped.filter((m) => m.isDependency);

  function row(m: ModFile) {
    const select =
      groupData.groups.length > 0 ? (
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
      ) : null;
    return renderRow(m, select);
  }

  return (
    <>
      {creating && (
        <div className="mod-group-new">
          <input
            autoFocus
            placeholder="Group name (e.g. Performance, QoL, Visuals)"
            value={newGroupName}
            maxLength={40}
            onChange={(e) => setNewGroupName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") createGroup();
              if (e.key === "Escape") onCreatingDone();
            }}
          />
          <button className="primary-btn small" onClick={createGroup}>
            Create
          </button>
          <button className="ghost-btn small" onClick={onCreatingDone}>
            Cancel
          </button>
        </div>
      )}
      {groupData.groups.map((g, gi) => {
        const groupMods = mods.filter((m) => groupOf(m) === g.id);
        const enabledCount = groupMods.filter((m) => m.enabled).length;
        if (filtering && groupMods.length === 0) return null;
        const collapsed = g.collapsed && !filtering;
        return (
          <div key={g.id} className="mod-group">
            <div className="mod-group-header" onClick={() => toggleCollapsed(g.id)}>
              <span className="mod-group-caret">{collapsed ? "▸" : "▾"}</span>
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
                    title={
                      enabledCount === groupMods.length
                        ? "Disable every mod in this group"
                        : "Enable every mod in this group"
                    }
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
                <button
                  className="icon-btn"
                  title="Rename group"
                  onClick={() => setRenamingGroup({ id: g.id, name: g.name })}
                >
                  ✎
                </button>
                <button
                  className="icon-btn"
                  title="Delete group (its mods stay installed)"
                  onClick={() => setConfirmDeleteGroup(g)}
                >
                  ✕
                </button>
              </div>
            </div>
            {!collapsed &&
              (groupMods.length === 0 ? (
                <div className="placeholder mod-group-empty">Use the group menu on a mod to move it here.</div>
              ) : (
                groupMods.map(row)
              ))}
          </div>
        );
      })}
      {groupData.groups.length > 0 && ungrouped.length > 0 && <div className="mods-subheader">Ungrouped</div>}
      {primaryMods.map(row)}
      {dependencyMods.length > 0 && (
        <>
          <div className="mods-subheader">Dependencies</div>
          {dependencyMods.map(row)}
        </>
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
    </>
  );
}
