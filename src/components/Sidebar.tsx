import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import appIcon from "../assets/app-icon.png";
import { AccountSwitcher } from "./AccountSwitcher";
import { ACTIVE_STAGES } from "./InstanceDetail";
import { InstanceIcon } from "./InstanceIcon";
import { PlayerAvatar } from "./PlayerAvatar";
import { RemoteServerIcon } from "./RemoteServerIcon";
import type { GameProfile, Instance, LaunchProgressEvent, RemoteServerLink, RunningInstance } from "../types";

interface Props {
  instances: Instance[];
  runningByInstance: Record<string, RunningInstance>;
  progressByInstance: Record<string, LaunchProgressEvent>;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNewInstance: () => void;
  onImportInstance: () => void;
  showServerEntryPoints: boolean;
  onNewServer: () => void;
  onImportServer: () => void;
  onReorder: (orderedIds: string[]) => void;
  onOpenInstanceSettings: (instance: Instance) => void;
  remoteServers: RemoteServerLink[];
  selectedRemoteId: string | null;
  onSelectRemote: (id: string) => void;
  onImportRemoteServer: () => void;
  profile: GameProfile | null;
  onProfileChange: (profile: GameProfile) => void;
  onSignOut: () => void;
  onOpenSettings: () => void;
}

export function Sidebar({
  instances,
  runningByInstance,
  progressByInstance,
  selectedId,
  onSelect,
  onNewInstance,
  onImportInstance,
  showServerEntryPoints,
  onNewServer,
  onImportServer,
  onReorder,
  onOpenInstanceSettings,
  remoteServers,
  selectedRemoteId,
  onSelectRemote,
  onImportRemoteServer,
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
  // Grouped by kind rather than shown in raw sort order - client and server
  // instances share the same underlying list/sort-order sequence (see
  // `InstanceKind`), so without this a server created between two client
  // instances would otherwise land in the middle of them. Reordering still
  // works the same as before within a group; dragging across the divider
  // just snaps back on the next render, since kind is what decides the
  // group, not drop position.
  const clientInstances = instances.filter((i) => i.kind !== "server");
  const serverInstances = instances.filter((i) => i.kind === "server");

  function renderRow(inst: Instance) {
    const running = runningByInstance[inst.id];
    const progress = progressByInstance[inst.id];
    const isBusy = !running && progress ? ACTIVE_STAGES.has(progress.stage) : false;
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
              {running.accountUsername ? (
                <>
                  <PlayerAvatar
                    uuid={running.accountUuid ?? ""}
                    username={running.accountUsername}
                    className="instance-row-avatar"
                    size={14}
                  />
                  Running as {running.accountUsername}
                </>
              ) : (
                "Running"
              )}
            </div>
          ) : isBusy ? (
            <div className="instance-row-loading">
              {inst.versionId}
              <span className="spinner-small" />
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
  }

  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <img src={appIcon} alt="" className="logo" />
        <h1>Mint Launcher</h1>
        <button className="donate-btn" onClick={() => openUrl("https://buymeacoffee.com/mintymc")} title="Buy me a coffee">
          Donate
        </button>
      </div>

      <div className="instance-list">
        {instances.length === 0 && (
          <div className="empty-hint">No instances yet. Create one to get started.</div>
        )}
        {clientInstances.map(renderRow)}
        {serverInstances.length > 0 && (
          <div className="sidebar-divider">
            <span>Servers</span>
          </div>
        )}
        {serverInstances.map(renderRow)}
        {remoteServers.length > 0 && (
          <div className="sidebar-divider">
            <span>Remote Servers</span>
          </div>
        )}
        {remoteServers.map((link) => (
          <div
            key={link.id}
            className={`instance-row${link.id === selectedRemoteId ? " selected" : ""}`}
            onClick={() => onSelectRemote(link.id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") onSelectRemote(link.id);
            }}
            role="button"
            tabIndex={0}
          >
            <RemoteServerIcon link={link} className="instance-icon" />
            <div className="instance-row-text">
              <div className="instance-row-name">{link.name}</div>
              <div className="instance-row-meta">
                {link.host}:{link.port}
              </div>
            </div>
          </div>
        ))}
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
        {showServerEntryPoints && (
          <>
            <button className="new-instance-btn" onClick={onNewServer}>
              + New Server
            </button>
            <button
              className="ghost-btn small"
              title="Import an existing dedicated server folder"
              onClick={onImportServer}
            >
              Import Server
            </button>
            <button
              className="ghost-btn small"
              title="Connect to a server another admin's Mint Launcher is sharing"
              onClick={onImportRemoteServer}
            >
              Import Remote Server
            </button>
          </>
        )}
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
