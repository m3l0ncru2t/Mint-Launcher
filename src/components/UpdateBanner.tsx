import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

type Status = "idle" | "downloading" | "installing" | "error";

export function UpdateBanner() {
  const [update, setUpdate] = useState<Update | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    check()
      .then((found) => {
        if (found) setUpdate(found);
      })
      .catch(() => {
        // No update endpoint reachable, or nothing published yet - not
        // worth interrupting startup over.
      });
  }, []);

  async function handleUpdate() {
    if (!update) return;
    setError(null);
    setStatus("downloading");
    let total = 0;
    let downloaded = 0;
    try {
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total > 0) setProgress(Math.min(100, Math.round((downloaded / total) * 100)));
        } else if (event.event === "Finished") {
          setStatus("installing");
        }
      });
      await relaunch();
    } catch (e) {
      setStatus("error");
      setError(String(e));
    }
  }

  if (!update || dismissed) return null;

  return (
    <div className="update-banner">
      <div className="update-banner-text">
        <strong>Update available</strong> — v{update.version} is ready to install.
        {error && <div className="error-text">{error}</div>}
      </div>
      <div className="update-banner-actions">
        {status === "idle" && (
          <>
            <button className="ghost-btn small" onClick={() => setDismissed(true)}>
              Later
            </button>
            <button className="primary-btn small" onClick={handleUpdate}>
              Update &amp; Restart
            </button>
          </>
        )}
        {status === "downloading" && <span className="hint-inline">Downloading… {progress}%</span>}
        {status === "installing" && <span className="hint-inline">Installing…</span>}
        {status === "error" && (
          <button className="ghost-btn small" onClick={handleUpdate}>
            Retry
          </button>
        )}
      </div>
    </div>
  );
}
