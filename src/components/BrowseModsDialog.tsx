import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { ModProjectInfoDialog } from "./ModProjectInfoDialog";
import type { ModSearchResult } from "../types";

interface Props {
  instanceId: string;
  onClose: () => void;
  onInstalled: () => void;
}

function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function BrowseModsDialog({ instanceId, onClose, onInstalled }: Props) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ModSearchResult[]>([]);
  const [totalHits, setTotalHits] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [installing, setInstalling] = useState<string | null>(null);
  const [installedMsg, setInstalledMsg] = useState<Record<string, string>>({});
  const [infoResult, setInfoResult] = useState<ModSearchResult | null>(null);
  const loadingMoreRef = useRef(false);

  useEffect(() => {
    const handle = setTimeout(() => {
      setLoading(true);
      setError(null);
      api
        .searchMods(instanceId, query, 0)
        .then((page) => {
          setResults(page.hits);
          setTotalHits(page.totalHits);
        })
        .catch((e) => setError(String(e)))
        .finally(() => setLoading(false));
    }, 350);
    return () => clearTimeout(handle);
  }, [instanceId, query]);

  function handleScroll(e: React.UIEvent<HTMLDivElement>) {
    const el = e.currentTarget;
    if (el.scrollHeight - el.scrollTop - el.clientHeight > 150) return;
    if (loadingMoreRef.current || loading || results.length >= totalHits) return;

    loadingMoreRef.current = true;
    setLoadingMore(true);
    api
      .searchMods(instanceId, query, results.length)
      .then((page) => {
        setResults((prev) => [...prev, ...page.hits]);
        setTotalHits(page.totalHits);
      })
      .catch((e) => setError(String(e)))
      .finally(() => {
        loadingMoreRef.current = false;
        setLoadingMore(false);
      });
  }

  async function handleInstall(projectId: string) {
    setInstalling(projectId);
    setError(null);
    try {
      const summary = await api.installMod(instanceId, projectId);
      const parts = summary.installed.map((m) => m.title);
      const message = parts.length > 0 ? `Installed ${parts.join(", ")}` : "Already up to date";
      setInstalledMsg((prev) => ({ ...prev, [projectId]: message }));
      onInstalled();
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
        <h3>Browse Mods</h3>
        <div className="subtitle">Mods compatible with this instance's version and loader.</div>

        <div className="form-field">
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search mods…"
            autoFocus
          />
        </div>

        {error && <div className="error-text">{error}</div>}

        <div className="mod-search-results" onScroll={handleScroll}>
          {loading && <div className="placeholder">Searching…</div>}
          {!loading && results.length === 0 && <div className="placeholder">No mods found.</div>}
          {!loading &&
            results.map((r) => (
              <div key={r.projectId} className="mod-search-row" onClick={() => setInfoResult(r)}>
                {r.iconUrl ? (
                  <img src={r.iconUrl} className="mod-search-icon" alt="" />
                ) : (
                  <div className="mod-search-icon placeholder-icon" />
                )}
                <div className="mod-search-info">
                  <div className="mod-search-title">{r.title}</div>
                  <div className="mod-search-desc">{r.description}</div>
                  <div className="mod-search-meta">
                    by {r.author} · {formatDownloads(r.downloads)} downloads
                  </div>
                  {installedMsg[r.projectId] && (
                    <div className="mod-search-installed">{installedMsg[r.projectId]}</div>
                  )}
                </div>
                <button
                  className="primary-btn small"
                  disabled={installing === r.projectId || !!installedMsg[r.projectId]}
                  onClick={(e) => {
                    e.stopPropagation();
                    handleInstall(r.projectId);
                  }}
                >
                  {installedMsg[r.projectId]
                    ? "Installed"
                    : installing === r.projectId
                      ? "Installing…"
                      : "Install"}
                </button>
              </div>
            ))}
          {!loading && loadingMore && <div className="placeholder">Loading more…</div>}
        </div>
      </div>

      {infoResult && (
        <ModProjectInfoDialog
          instanceId={instanceId}
          result={infoResult}
          installing={installing === infoResult.projectId}
          installedMsg={installedMsg[infoResult.projectId]}
          onInstall={handleInstall}
          onClose={() => setInfoResult(null)}
        />
      )}
    </div>
  );
}
