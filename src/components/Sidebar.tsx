import { AccountSwitcher } from "./AccountSwitcher";
import type { GameProfile, Instance } from "../types";

interface Props {
  instances: Instance[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNewInstance: () => void;
  profile: GameProfile | null;
  onProfileChange: (profile: GameProfile) => void;
  onSignOut: () => void;
  onOpenSettings: () => void;
}

export function Sidebar({
  instances,
  selectedId,
  onSelect,
  onNewInstance,
  profile,
  onProfileChange,
  onSignOut,
  onOpenSettings,
}: Props) {
  return (
    <div className="sidebar">
      <div className="sidebar-header">
        <div className="logo" />
        <h1>Mint Launcher</h1>
      </div>

      <div className="instance-list">
        {instances.length === 0 && (
          <div className="empty-hint">No instances yet. Create one to get started.</div>
        )}
        {instances.map((inst) => (
          <button
            key={inst.id}
            className={`instance-row${inst.id === selectedId ? " selected" : ""}`}
            onClick={() => onSelect(inst.id)}
          >
            <div className="instance-icon">{inst.name.slice(0, 1).toUpperCase()}</div>
            <div className="instance-row-text">
              <div className="instance-row-name">{inst.name}</div>
              <div className="instance-row-meta">{inst.versionId}</div>
            </div>
          </button>
        ))}
      </div>

      <button className="new-instance-btn" onClick={onNewInstance}>
        + New Instance
      </button>

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
