import { useEffect, useState } from "react";
import { api } from "../api";
import { LogViewerDialog } from "./LogViewerDialog";
import type { LogEntry } from "../types";

interface Props {
  instanceId: string;
}

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function LogsPanel({ instanceId }: Props) {
  const [files, setFiles] = useState<LogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [viewFile, setViewFile] = useState<string | null>(null);

  function load() {
    setLoading(true);
    api
      .listLogFiles(instanceId)
      .then((list) => {
        setFiles(list);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }

  useEffect(load, [instanceId]);

  async function handleOpenFolder() {
    try {
      const dir = await api.getLogsDir(instanceId);
      await api.openFolder(dir);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <>
      <div className="panel-header">
        <h4>Logs{files.length > 0 ? ` (${files.length})` : ""}</h4>
        <div className="panel-actions">
          <button className="ghost-btn small" onClick={load}>
            Refresh
          </button>
          <button className="ghost-btn small" onClick={handleOpenFolder}>
            Open folder
          </button>
        </div>
      </div>
      <div className="mods-list">
        {loading && <div className="placeholder">Loading…</div>}
        {!loading && error && <div className="error-text">{error}</div>}
        {!loading && !error && files.length === 0 && (
          <div className="placeholder">
            No log files yet - one shows up here the first time this instance runs.
          </div>
        )}
        {!loading &&
          !error &&
          files.map((f) => (
            <div key={f.fileName} className="mod-row" onClick={() => setViewFile(f.fileName)}>
              <div className="mod-name-block">
                <span className="mod-name">{f.fileName}</span>
                {f.modifiedAt && (
                  <span className="mod-filename">{new Date(f.modifiedAt).toLocaleString()}</span>
                )}
              </div>
              <span className="mod-size">{formatSize(f.size)}</span>
            </div>
          ))}
      </div>

      {viewFile && (
        <LogViewerDialog instanceId={instanceId} fileName={viewFile} onClose={() => setViewFile(null)} />
      )}
    </>
  );
}
