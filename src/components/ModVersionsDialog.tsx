import { useEffect, useState } from "react";
import type { InstallSummary, ModVersionInfo } from "../types";

interface Props {
  /** The mod's name, for the heading. */
  title: string;
  /** e.g. "26.3" - what "compatible" means in the list. */
  gameVersionLabel?: string;
  loadVersions: (allGameVersions: boolean) => Promise<ModVersionInfo[]>;
  installVersion: (versionId: string) => Promise<InstallSummary>;
  /** Called after a version was installed or switched to, so the caller can refresh its list. */
  onInstalled: (message: string) => void;
  onClose: () => void;
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
}

/** "1.21.4, 1.21.5" for a couple; "1.20.1 – 1.21.5" (oldest–newest as Modrinth lists them) for a long run. */
function gameVersionsText(versions: string[]): string {
  if (versions.length <= 3) return versions.join(", ");
  return `${versions[0]} – ${versions[versions.length - 1]}`;
}

/**
 * Lists a mod's versions so a specific one can be installed instead of always the newest - e.g. to stay on
 * an older build after a buggy update. Only versions for the instance's loader; just the ones for its
 * Minecraft version unless "Show all Minecraft versions" is ticked. Choosing a different version of a mod
 * that's already installed replaces it.
 */
export function ModVersionsDialog({ title, gameVersionLabel, loadVersions, installVersion, onInstalled, onClose }: Props) {
  const [showAll, setShowAll] = useState(false);
  const [versions, setVersions] = useState<ModVersionInfo[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setVersions(null);
    setError(null);
    loadVersions(showAll)
      .then((list) => !cancelled && setVersions(list))
      .catch((e) => {
        if (cancelled) return;
        // A remote host running an older Mint has no versions endpoint
        setError(String(e).includes("404") ? "The server's Mint needs updating before versions can be picked here." : String(e));
      });
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [showAll, reloadKey]);

  const installedVersion = versions?.find((v) => v.installed) ?? null;

  async function handleInstall(v: ModVersionInfo) {
    setInstalling(v.id);
    setError(null);
    setMessage(null);
    try {
      const summary = await installVersion(v.id);
      // The mod itself plus any required dependencies that weren't there yet
      const deps = Math.max(0, summary.installed.length - 1);
      const msg = (installedVersion ? `Switched to ${v.versionNumber}` : `Installed ${v.versionNumber}`)
        + (deps > 0 ? ` (plus ${deps} dependenc${deps === 1 ? "y" : "ies"})` : "");
      setMessage(msg);
      onInstalled(msg);
      // Refresh the "Installed" marker
      setReloadKey((k) => k + 1);
    } catch (e) {
      setError(String(e));
    } finally {
      setInstalling(null);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal wide" onClick={(e) => e.stopPropagation()}>
        <button className="modal-close-btn" title="Close" onClick={onClose}>
          ✕
        </button>
        <h3>{title} – versions</h3>
        <div className="subtitle">
          {installedVersion ? `Installed: ${installedVersion.versionNumber}. ` : ""}
          Pick a version to install{installedVersion ? " or switch to" : ""}.
        </div>

        <label className="mod-versions-toggle">
          <input type="checkbox" checked={showAll} onChange={(e) => setShowAll(e.target.checked)} />
          Show versions for other Minecraft versions
        </label>

        {error && <div className="error-text">{error}</div>}
        {message && <div className="mod-search-installed">{message}</div>}

        <div className="mod-search-results">
          {!versions && !error && <div className="placeholder">Loading versions…</div>}
          {versions && versions.length === 0 && (
            <div className="placeholder">
              {showAll
                ? "No versions of this mod for this loader."
                : `No versions for ${gameVersionLabel ?? "this Minecraft version"} - tick the box above to see others.`}
            </div>
          )}
          {versions?.map((v) => {
            const busy = installing === v.id;
            return (
              <div key={v.id} className={`mod-version-row${v.installed ? " installed" : ""}`}>
                <div className="mod-version-info">
                  <div className="mod-version-title">
                    <span className="mod-version-number">{v.versionNumber}</span>
                    {v.versionType && v.versionType !== "release" && (
                      <span className={`mod-version-type ${v.versionType}`}>{v.versionType}</span>
                    )}
                    {v.installed && <span className="mod-version-installed">✓ Installed</span>}
                  </div>
                  {v.name && v.name !== v.versionNumber && <div className="mod-version-name">{v.name}</div>}
                  <div className="mod-version-meta">
                    {formatDate(v.datePublished)}
                    {v.gameVersions.length > 0 && ` · for ${gameVersionsText(v.gameVersions)}`}
                  </div>
                  {!v.compatible && (
                    <div className="mod-incompatible-badge">
                      Not made for {gameVersionLabel ?? "this Minecraft version"} - it may crash the game
                    </div>
                  )}
                </div>
                <button
                  className={v.compatible ? "primary-btn small" : "ghost-btn small"}
                  disabled={v.installed || installing !== null}
                  onClick={() => handleInstall(v)}
                >
                  {v.installed ? "Installed" : busy ? "Installing…" : installedVersion ? "Switch" : "Install"}
                </button>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
