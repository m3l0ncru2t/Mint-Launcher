import { useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { api } from "../api";
import type { PortableUpdateInfo } from "../types";

type Status = "idle" | "downloading" | "installing" | "error";

export function UpdateBanner() {
  const [portable, setPortable] = useState(false);
  const [update, setUpdate] = useState<Update | null>(null);
  const [portableUpdate, setPortableUpdate] = useState<PortableUpdateInfo | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [status, setStatus] = useState<Status>("idle");
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // The installed-app updater (Tauri's plugin-updater) downloads and runs
    // the NSIS/MSI installer, which assumes an installed app at a fixed
    // location. A portable build has no installer to run, so it checks
    // GitHub releases directly instead - see check_portable_update in
    // src-tauri/src/commands/updater.rs.
    api
      .isPortable()
      .then((isPortable) => {
        setPortable(isPortable);
        if (isPortable) {
          return api.checkPortableUpdate().then((found) => {
            if (found) setPortableUpdate(found);
          });
        }
        return check().then((found) => {
          if (found) setUpdate(found);
        });
      })
      .catch(() => {
        // No update endpoint reachable, or nothing published yet - not
        // worth interrupting startup over.
      });
  }, []);

  async function handleUpdate() {
    setError(null);
    setStatus("downloading");
    try {
      if (portable) {
        if (!portableUpdate) return;
        await api.installPortableUpdate(portableUpdate.downloadUrl);
        // The backend swaps the exe and relaunches the app itself once this
        // process exits, so there's nothing left to do on success here.
        return;
      }

      if (!update) return;
      let total = 0;
      let downloaded = 0;
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

  const version = portable ? portableUpdate?.version : update?.version;
  if (!version || dismissed) return null;

  return (
    <div className="update-banner">
      <div className="update-banner-text">
        <strong>Update available</strong> — v{version} is ready to install.
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
        {status === "downloading" && (
          <span className="hint-inline">{portable ? "Downloading…" : `Downloading… ${progress}%`}</span>
        )}
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
