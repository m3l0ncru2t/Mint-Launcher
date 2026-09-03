import { AccountSwitcher } from "./AccountSwitcher";
import { InstanceIcon } from "./InstanceIcon";
import type { GameProfile, Instance } from "../types";

interface Props {
  instances: Instance[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  onNewInstance: () => void;
  onImportInstance: () => void;
  onImportFromLauncher: () => void;
  importError: string | null;
  onOpenInstanceSettings: (instance: Instance) => void;
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
  onImportInstance,
  onImportFromLauncher,
  importError,
  onOpenInstanceSettings,
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
          <div
            key={inst.id}
            className={`instance-row${inst.id === selectedId ? " selected" : ""}`}
            onClick={() => onSelect(inst.id)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") onSelect(inst.id);
            }}
            role="button"
            tabIndex={0}
          >
            <InstanceIcon instance={inst} className="instance-icon" />
            <div className="instance-row-text">
              <div className="instance-row-name">{inst.name}</div>
              <div className="instance-row-meta">{inst.versionId}</div>
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
        ))}
      </div>

      {importError && <div className="error-text sidebar-error">{importError}</div>}
      <div className="sidebar-footer-actions">
        <button className="new-instance-btn" onClick={onNewInstance}>
          + New Instance
        </button>
        <button className="ghost-btn small" title="Import an exported instance backup" onClick={onImportInstance}>
          Import
        </button>
        <button
          className="ghost-btn small"
          title="Import from the official launcher, MultiMC/Prism/PolyMC, or CurseForge"
          onClick={onImportFromLauncher}
        >
          Import from Launcher
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
