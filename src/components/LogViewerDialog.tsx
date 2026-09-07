import { useEffect, useState } from "react";
import { api } from "../api";

interface Props {
  instanceId: string;
  fileName: string;
  onClose: () => void;
}

export function LogViewerDialog({ instanceId, fileName, onClose }: Props) {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    api
      .readLogFile(instanceId, fileName)
      .then(setContent)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [instanceId, fileName]);

  async function handleCopy() {
    await navigator.clipboard.writeText(content);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal wide" onClick={(e) => e.stopPropagation()}>
        <h3>{fileName}</h3>
        {loading && <div className="placeholder">Loading…</div>}
        {!loading && error && <div className="error-text">{error}</div>}
        {!loading && !error && <div className="log-console file-viewer-body">{content}</div>}
        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Close
          </button>
          <button className="ghost-btn" onClick={handleCopy} disabled={loading || !!error}>
            {copied ? "Copied!" : "Copy"}
          </button>
        </div>
      </div>
    </div>
  );
}
