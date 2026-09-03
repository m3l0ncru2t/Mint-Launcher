import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import type { ResourcePackDetails } from "../types";

interface Props {
  instanceId: string;
  fileName: string;
  enabled: boolean;
  onClose: () => void;
}

function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ResourcePackInfoDialog({ instanceId, fileName, enabled, onClose }: Props) {
  const [details, setDetails] = useState<ResourcePackDetails | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    api
      .getResourcepackInfo(instanceId, fileName)
      .then(setDetails)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [instanceId, fileName]);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal wide" onClick={(e) => e.stopPropagation()}>
        {loading && <div className="placeholder">Loading…</div>}
        {!loading && error && <div className="error-text">{error}</div>}

        {!loading && !error && details && !details.found && (
          <>
            <h3>{details.fileName}</h3>
            <div className="subtitle">
              {formatSize(details.size)} · {enabled ? "Enabled" : "Disabled"}
            </div>
            <div className="placeholder">
              This resource pack couldn't be matched to anything on Modrinth, so no extra info is available.
            </div>
          </>
        )}

        {!loading && !error && details && details.found && (
          <>
            <div className="mod-info-header">
              {details.iconUrl ? (
                <img src={details.iconUrl} className="mod-info-icon" alt="" />
              ) : (
                <div className="mod-info-icon placeholder-icon" />
              )}
              <div>
                <h3>{details.title}</h3>
                <div className="subtitle">{details.author ? `by ${details.author}` : details.fileName}</div>
              </div>
            </div>

            {details.description && <div className="mod-info-desc">{details.description}</div>}

            <div className="mod-info-stats">
              {details.currentVersion && (
                <div className="mod-info-stat">
                  <span className="mod-info-stat-label">Version</span>
                  <span>{details.currentVersion}</span>
                </div>
              )}
              {details.downloads != null && (
                <div className="mod-info-stat">
                  <span className="mod-info-stat-label">Downloads</span>
                  <span>{formatDownloads(details.downloads)}</span>
                </div>
              )}
              <div className="mod-info-stat">
                <span className="mod-info-stat-label">File</span>
                <span>{formatSize(details.size)}</span>
              </div>
              <div className="mod-info-stat">
                <span className="mod-info-stat-label">Status</span>
                <span>{enabled ? "Enabled" : "Disabled"}</span>
              </div>
            </div>

            {details.categories.length > 0 && (
              <div className="mod-info-tags">
                {details.categories.map((c) => (
                  <span key={c} className="mod-info-tag">
                    {c}
                  </span>
                ))}
              </div>
            )}

            <div className="mod-info-filename">{details.fileName}</div>
          </>
        )}

        <div className="modal-actions">
          {details?.projectUrl && (
            <button className="ghost-btn" onClick={() => openUrl(details.projectUrl!)}>
              View on Modrinth
            </button>
          )}
          <button className="ghost-btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
