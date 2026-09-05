import { useState } from "react";
import appIcon from "../assets/app-icon.png";
import { AccountSwitcher } from "./AccountSwitcher";
import { InstanceIcon } from "./InstanceIcon";
import type { GameProfile, Instance, RunningInstance } from "../types";

interface Props {
  instances: Instance[];
  runningByInstance: Record<string, RunningInstance>;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNewInstance: () => void;
  onImportInstance: () => void;
  onReorder: (orderedIds: string[]) => void;
  onOpenInstanceSettings: (instance: Instance) => void;
  profile: GameProfile | null;
  onProfileChange: (profile: GameProfile) => void;
  onSignOut: () => void;
  onOpenSettings: () => void;
}

export function Sidebar({
  instances,
  runningByInstance,
  selectedId,
  onSelect,
  onNewInstance,
  onImportInstance,
  onReorder,
  onOpenInstanceSettings,
  profile,
  onProfileChange,
  onSignOut,
  onOpenSettings,
}: Props) {
  const [draggedId, setDraggedId] = useState<string | null>(null);
  const [dragOverId, setDragOverId] = useState<string | null>(null);

  function handleDrop(targetId: string) {
    if (draggedId && draggedId !== targetId) {
      const ids = instances.map((i) => i.id);
      const from = ids.indexOf(draggedId);
      const to = ids.indexOf(targetId);
      ids.splice(from, 1);
      ids.splice(to, 0, draggedId);
      onReorder(ids);
    }
    setDraggedId(null);
    setDragOverId(null);
  }
  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <img src={appIcon} alt="" className="logo" />
        <h1>Mint Launcher</h1>
      </div>

      <div className="instance-list">
        {instances.length === 0 && (
          <div className="empty-hint">No instances yet. Create one to get started.</div>
        )}
        {instances.map((inst) => {
          const running = runningByInstance[inst.id];
          return (
            <div
              key={inst.id}
              className={`instance-row${inst.id === selectedId ? " selected" : ""}${
                inst.id === draggedId ? " dragging" : ""
              }${inst.id === dragOverId && inst.id !== draggedId ? " drag-over" : ""}`}
              draggable
              onDragStart={(e) => {
                setDraggedId(inst.id);
                e.dataTransfer.effectAllowed = "move";
                // WebKit (this app's Linux webview) only reliably fires `drop`
                // on a drag that actually carries data - effectAllowed alone
                // isn't enough there, unlike Chromium/Firefox.
                e.dataTransfer.setData("text/plain", inst.id);
              }}
              onDragEnd={() => {
                setDraggedId(null);
                setDragOverId(null);
              }}
              onDragOver={(e) => {
                e.preventDefault();
                if (draggedId && draggedId !== inst.id) setDragOverId(inst.id);
              }}
              onDragLeave={() => setDragOverId((id) => (id === inst.id ? null : id))}
              onDrop={(e) => {
                e.preventDefault();
                handleDrop(inst.id);
              }}
              onClick={() => onSelect(inst.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") onSelect(inst.id);
              }}
              role="button"
              tabIndex={0}
            >
              <span className="drag-handle" title="Drag to reorder">
                ⠿
              </span>
              <InstanceIcon instance={inst} className="instance-icon" />
              <div className="instance-row-text">
                <div className="instance-row-name">{inst.name}</div>
                {running ? (
                  <div className="instance-row-running">
                    <span className="running-dot" />
                    Running as {running.accountUsername}
                  </div>
                ) : (
                  <div className="instance-row-meta">{inst.versionId}</div>
                )}
              </div>
              <button
                className="icon-btn"
                title="Instance settings"
                onClick={(e) => {
                  e.stopPropagation();
                  onOpenInstanceSettings(inst);
                }}
              >
                ⚙
              </button>
            </div>
          );
        })}
      </div>

      <div className="sidebar-footer-actions">
        <button className="new-instance-btn" onClick={onNewInstance}>
          + New Instance
        </button>
        <button
          className="ghost-btn small"
          title="Restore a Mint Launcher backup, or import from the official launcher, MultiMC/Prism/PolyMC, CurseForge, or Modrinth App"
          onClick={onImportInstance}
        >
          Import Instance
        </button>
      </div>

      <div className="account-widget">
        {profile ? (
          <AccountSwitcher profile={profile} onProfileChange={onProfileChange} onSignOut={onSignOut} />
        ) : (
          <div className="account-info">
            <div className="account-name">Not signed in</div>
          </div>
        )}
        <button className="icon-btn" title="Settings" onClick={onOpenSettings}>
          ⚙
        </button>
      </div>
    </div>
  );
}
