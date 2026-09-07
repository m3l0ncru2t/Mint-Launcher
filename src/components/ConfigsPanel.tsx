import { useEffect, useState } from "react";
import { api } from "../api";
import { ConfirmDialog } from "./ConfirmDialog";
import { ConfigEditorDialog } from "./ConfigEditorDialog";
import type { ConfigEntry } from "../types";

interface Props {
  instanceId: string;
}

function formatSize(bytes: number): string {
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

export function ConfigsPanel({ instanceId }: Props) {
  const [files, setFiles] = useState<ConfigEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [editFile, setEditFile] = useState<string | null>(null);
  const [confirmDeleteFile, setConfirmDeleteFile] = useState<string | null>(null);

  function load() {
    setLoading(true);
    api
      .listConfigFiles(instanceId)
      .then((list) => {
        setFiles(list);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }

  useEffect(load, [instanceId]);

  async function handleDelete(fileName: string) {
    try {
      await api.deleteConfigFile(instanceId, fileName);
      load();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleOpenFolder() {
    try {
      const dir = await api.getConfigDir(instanceId);
      await api.openFolder(dir);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <>
      <div className="panel-header">
        <h4>Configs{files.length > 0 ? ` (${files.length})` : ""}</h4>
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
            No config files yet - mods usually create these the first time they run.
          </div>
        )}
        {!loading &&
          !error &&
          files.map((f) => (
            <div
              key={f.fileName}
              className="mod-row"
              onClick={() => !f.isDir && setEditFile(f.fileName)}
              style={f.isDir ? { cursor: "default" } : undefined}
            >
              <div className="mod-name-block">
                <span className="mod-name">{f.fileName}</span>
                {f.isDir && <span className="mod-filename">Folder - not browsable here yet</span>}
              </div>
              <span className="mod-size">{formatSize(f.size)}</span>
              <button
                className="icon-btn"
                title="Delete"
                onClick={(e) => {
                  e.stopPropagation();
                  setConfirmDeleteFile(f.fileName);
                }}
              >
                ✕
              </button>
            </div>
          ))}
      </div>

      {editFile && (
        <ConfigEditorDialog instanceId={instanceId} fileName={editFile} onClose={() => setEditFile(null)} />
      )}

      {confirmDeleteFile && (
        <ConfirmDialog
          title="Delete config?"
          message={`Delete ${confirmDeleteFile}? This can't be undone.`}
          confirmLabel="Delete"
          danger
          onConfirm={() => {
            handleDelete(confirmDeleteFile);
            setConfirmDeleteFile(null);
          }}
          onCancel={() => setConfirmDeleteFile(null)}
        />
      )}
    </>
  );
}
