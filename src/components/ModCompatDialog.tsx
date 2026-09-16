import type { ModUpdateInfo } from "../types";

interface Props {
  instanceName: string;
  versionId: string;
  results: ModUpdateInfo[];
  onClose: () => void;
}

/** Shown right after changing an instance's Minecraft version or Fabric
 * loader build (see App.tsx's InstanceSettingsDialog onSaved) - a scan
 * against the *new* version/loader (check_mod_updates always checks
 * whatever's currently saved on the instance, so this just runs it again
 * right after the change lands) told against the version you just moved to,
 * rather than making you notice the same badges later by opening the Mods
 * tab yourself. Read-only: updating still happens from the Mods tab itself,
 * which already shows the identical badges once opened. */
export function ModCompatDialog({ instanceName, versionId, results, onClose }: Props) {
  const incompatible = results.filter((r) => r.projectId && !r.compatible);
  const updatable = results.filter((r) => r.updateAvailable);
  const fine = results.length - incompatible.length - updatable.length;

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>Mod check: {instanceName}</h3>
        <div className="subtitle">
          Checked {results.length} mod{results.length === 1 ? "" : "s"} against {versionId}
          {fine > 0 ? ` - ${fine} need${fine === 1 ? "s" : ""} nothing.` : "."}
        </div>
        {results.length === 0 ? (
          <div className="hint" style={{ marginTop: 8 }}>
            No mods installed.
          </div>
        ) : incompatible.length === 0 && updatable.length === 0 ? (
          <div className="hint" style={{ marginTop: 8 }}>
            Every mod is already up to date for {versionId}.
          </div>
        ) : (
          <div className="mods-list" style={{ marginTop: 8, maxHeight: "40vh", overflowY: "auto" }}>
            {incompatible.map((r) => (
              <div key={r.fileName} className="mod-row" style={{ cursor: "default" }}>
                <div className="mod-name-block">
                  <span className="mod-name">{r.title ?? r.fileName}</span>
                  <span className="mod-filename">{r.fileName}</span>
                </div>
                <span className="mod-incompatible-badge">No build for {versionId}</span>
              </div>
            ))}
            {updatable.map((r) => (
              <div key={r.fileName} className="mod-row" style={{ cursor: "default" }}>
                <div className="mod-name-block">
                  <span className="mod-name">{r.title ?? r.fileName}</span>
                  <span className="mod-filename">{r.fileName}</span>
                </div>
                <span className="mod-update-badge">→ {r.latestVersion}</span>
              </div>
            ))}
          </div>
        )}
        {(incompatible.length > 0 || updatable.length > 0) && (
          <div className="hint" style={{ marginTop: 8 }}>
            Open the Mods tab to update or remove them.
          </div>
        )}
        <div className="modal-actions">
          <button className="primary-btn" onClick={onClose}>
            Got it
          </button>
        </div>
      </div>
    </div>
  );
}
