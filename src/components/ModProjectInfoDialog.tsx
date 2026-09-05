import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import type { ModProjectDetails, ModSearchResult } from "../types";

interface Props {
  instanceId: string;
  result: ModSearchResult;
  installing: boolean;
  installedMsg?: string;
  onInstall: (projectId: string) => void;
  onClose: () => void;
}

function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function sideLabel(side: string): string | null {
  if (!side || side === "unknown") return null;
  if (side === "required") return "Required";
  if (side === "optional") return "Optional";
  if (side === "unsupported") return "Unsupported";
  return side;
}

export function ModProjectInfoDialog({ instanceId, result, installing, installedMsg, onInstall, onClose }: Props) {
  const [details, setDetails] = useState<ModProjectDetails | null>(null);
  const [error, setError] = useState<string | null>(null);

  const outdated = result.installed && result.upToDate === false && !installedMsg;
  const alreadyInstalled = !!installedMsg || result.installed;
  const statusText = installedMsg ?? (result.installed ? (outdated ? "Installed · update available" : "✓ Installed and up to date") : null);

  useEffect(() => {
    api
      .getProjectInfo(instanceId, result.projectId)
      .then(setDetails)
      .catch((e) => setError(String(e)));
  }, [instanceId, result.projectId]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal wide" onClick={(e) => e.stopPropagation()}>
        <button className="modal-close-btn" title="Close" onClick={onClose}>
          ✕
        </button>

        <div className="mod-info-header">
          {result.iconUrl ? (
            <img src={result.iconUrl} className="mod-info-icon" alt="" />
          ) : (
            <div className="mod-info-icon placeholder-icon" />
          )}
          <div>
            <h3>{result.title}</h3>
            <div className="subtitle">by {details?.author ?? result.author}</div>
          </div>
        </div>

        <div className="mod-info-desc">{details?.description ?? result.description}</div>

        {statusText && (
          <div className={`mod-search-installed${outdated ? " outdated" : ""}`}>{statusText}</div>
        )}

        {error && <div className="error-text">{error}</div>}

        <div className="mod-info-stats">
          <div className="mod-info-stat">
            <span className="mod-info-stat-label">Downloads</span>
            <span>{formatDownloads(details?.downloads ?? result.downloads)}</span>
          </div>
          {details?.latestVersion && (
            <div className="mod-info-stat">
              <span className="mod-info-stat-label">Compatible version</span>
              <span>{details.latestVersion}</span>
            </div>
          )}
          {details && sideLabel(details.clientSide) && (
            <div className="mod-info-stat">
              <span className="mod-info-stat-label">Client</span>
              <span>{sideLabel(details.clientSide)}</span>
            </div>
          )}
          {details && sideLabel(details.serverSide) && (
            <div className="mod-info-stat">
              <span className="mod-info-stat-label">Server</span>
              <span>{sideLabel(details.serverSide)}</span>
            </div>
          )}
        </div>

        {details && details.categories.length > 0 && (
          <div className="mod-info-tags">
            {details.categories.map((c) => (
              <span key={c} className="mod-info-tag">
                {c}
              </span>
            ))}
          </div>
        )}

        <div className="modal-actions">
          <button className="ghost-btn" onClick={() => openUrl(details?.projectUrl ?? `https://modrinth.com/mod/${result.slug}`)}>
            View on Modrinth
          </button>
          <button
            className="primary-btn"
            disabled={installing || alreadyInstalled}
            onClick={() => onInstall(result.projectId)}
          >
            {alreadyInstalled ? "Installed" : installing ? "Installing…" : "Install"}
          </button>
        </div>
      </div>
    </div>
  );
}
