import { useEffect, useState } from "react";
import { api } from "../api";

interface Props {
  instanceId: string;
  fileName: string;
  onClose: () => void;
}

export function ConfigEditorDialog({ instanceId, fileName, onClose }: Props) {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    api
      .readConfigFile(instanceId, fileName)
      .then(setContent)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [instanceId, fileName]);

  async function handleSave() {
    setSaving(true);
    setError(null);
    try {
      await api.writeConfigFile(instanceId, fileName, content);
      setDirty(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal wide" onClick={(e) => e.stopPropagation()}>
        <h3>{fileName}</h3>
        {loading && <div className="placeholder">Loading…</div>}
        {!loading && error && <div className="error-text">{error}</div>}
        {!loading && !error && (
          <textarea
            className="config-editor-textarea"
            value={content}
            spellCheck={false}
            onChange={(e) => {
              setContent(e.target.value);
              setDirty(true);
            }}
          />
        )}
        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Close
          </button>
          <button className="primary-btn" onClick={handleSave} disabled={saving || loading || !!error || !dirty}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
