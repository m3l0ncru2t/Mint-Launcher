import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { ResourcePackProjectInfoDialog } from "./ResourcePackProjectInfoDialog";
import type { ModSearchResult } from "../types";

interface Props {
  instanceId: string;
  onClose: () => void;
  onInstalled: () => void;
  /** Resource packs (default) or Iris shader packs - same Modrinth flow. */
  kind?: "resourcepack" | "shader";
}

const KINDS = {
  resourcepack: {
    title: "Browse Resource Packs",
    subtitle: "Resource packs compatible with this instance's Minecraft version.",
    placeholder: "Search resource packs…",
    empty: "No resource packs found.",
    search: api.searchResourcepacks,
    install: api.installResourcepack,
  },
  shader: {
    title: "Browse Shader Packs",
    subtitle: "Iris shader packs for this instance's Minecraft version. Installing one makes it the active pack.",
    placeholder: "Search shader packs…",
    empty: "No shader packs found.",
    search: api.searchShaderpacks,
    install: api.installShaderpack,
  },
};

function formatDownloads(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

export function BrowseResourcePacksDialog({ instanceId, onClose, onInstalled, kind = "resourcepack" }: Props) {
  const k = KINDS[kind];
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
      k
        .search(instanceId, query, 0)
        .then((page) => {
          setResults(page.hits);
          setTotalHits(page.totalHits);
        })
        .catch((e) => setError(String(e)))
        .finally(() => setLoading(false));
    }, 350);
    return () => clearTimeout(handle);
  }, [instanceId, query, k]);

  function handleScroll(e: React.UIEvent<HTMLDivElement>) {
    const el = e.currentTarget;
    if (el.scrollHeight - el.scrollTop - el.clientHeight > 150) return;
    if (loadingMoreRef.current || loading || results.length >= totalHits) return;

    loadingMoreRef.current = true;
    setLoadingMore(true);
    k
      .search(instanceId, query, results.length)
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
      const info = await k.install(instanceId, projectId);
      setInstalledMsg((prev) => ({ ...prev, [projectId]: `Installed ${info.title}` }));
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
        <h3>{k.title}</h3>
        <div className="subtitle">{k.subtitle}</div>

        <div className="form-field">
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder={k.placeholder}
            autoFocus
          />
        </div>

        {error && <div className="error-text">{error}</div>}

        <div className="mod-search-results" onScroll={handleScroll}>
          {loading && <div className="placeholder">Searching…</div>}
          {!loading && results.length === 0 && <div className="placeholder">{k.empty}</div>}
          {!loading &&
            results.map((r) => {
              const sessionMsg = installedMsg[r.projectId];
              const outdated = r.installed && r.upToDate === false && !sessionMsg;
              const statusText = sessionMsg ?? (r.installed ? (outdated ? "Installed · update available" : "✓ Installed and up to date") : null);
              const alreadyInstalled = !!sessionMsg || r.installed;
              return (
                <div key={r.projectId} className="mod-search-row" onClick={() => setInfoResult(r)}>
                  {r.iconUrl ? (
                    <img src={r.iconUrl} className="mod-search-icon" alt="" />
                  ) : (
                    <div className="mod-search-icon placeholder-icon">{r.title.slice(0, 1).toUpperCase()}</div>
                  )}
                  <div className="mod-search-info">
                    <div className="mod-search-title">{r.title}</div>
                    <div className="mod-search-desc">{r.description}</div>
                    <div className="mod-search-meta">
                      by {r.author} · {formatDownloads(r.downloads)} downloads
                    </div>
                    {statusText && (
                      <div className={`mod-search-installed${outdated ? " outdated" : ""}`}>{statusText}</div>
                    )}
                  </div>
                  <button
                    className="primary-btn small"
                    disabled={installing === r.projectId || alreadyInstalled}
                    onClick={(e) => {
                      e.stopPropagation();
                      handleInstall(r.projectId);
                    }}
                  >
                    {alreadyInstalled ? "Installed" : installing === r.projectId ? "Installing…" : "Install"}
                  </button>
                </div>
              );
            })}
          {!loading && loadingMore && <div className="placeholder">Loading more…</div>}
        </div>
      </div>

      {infoResult && (
        <ResourcePackProjectInfoDialog
          instanceId={instanceId}
          kind={kind}
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
