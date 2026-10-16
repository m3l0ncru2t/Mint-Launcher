import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { setVisibleInterval } from "../lib/visibleInterval";
import type { ScreenshotFile } from "../types";

interface Props {
  instanceId: string;
}

// Thumbnails are decoded on the backend; a few at a time keeps a folder of
// hundreds from flooding it when the tab first opens.
const MAX_CONCURRENT_THUMBS = 4;
let activeThumbs = 0;
const thumbQueue: (() => void)[] = [];
function queueThumb<T>(task: () => Promise<T>): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const run = () => {
      activeThumbs++;
      task()
        .then(resolve, reject)
        .finally(() => {
          activeThumbs--;
          thumbQueue.shift()?.();
        });
    };
    if (activeThumbs < MAX_CONCURRENT_THUMBS) run();
    else thumbQueue.push(run);
  });
}

// Per launcher run, so switching tabs back doesn't reload every thumbnail.
const thumbCache = new Map<string, string>();

/** One grid tile - asks for its thumbnail once it scrolls into view. */
function ScreenshotThumb({ instanceId, shot, onOpen }: { instanceId: string; shot: ScreenshotFile; onOpen: () => void }) {
  const key = `${instanceId}/${shot.fileName}/${shot.modifiedAt}`;
  const [src, setSrc] = useState<string | null>(thumbCache.get(key) ?? null);
  const [failed, setFailed] = useState(false);
  const ref = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (src || !ref.current) return;
    let cancelled = false;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        observer.disconnect();
        queueThumb(() => api.getScreenshotThumbnail(instanceId, shot.fileName))
          .then((url) => {
            thumbCache.set(key, url);
            if (!cancelled) setSrc(url);
          })
          .catch(() => !cancelled && setFailed(true));
      },
      { rootMargin: "200px" },
    );
    observer.observe(ref.current);
    return () => {
      cancelled = true;
      observer.disconnect();
    };
  }, [key, src, instanceId, shot.fileName]);

  return (
    <button ref={ref} className="screenshot-thumb" onClick={onOpen} title={shot.fileName}>
      {src ? (
        <img src={src} alt="" />
      ) : (
        <div className="screenshot-thumb-placeholder">{failed ? "Couldn't load" : ""}</div>
      )}
      <span className="screenshot-thumb-date">{formatDate(shot.modifiedAt)}</span>
    </button>
  );
}

function formatDate(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? "" : d.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/** The instance's `screenshots` folder as a thumbnail grid, with a full-size
 * viewer. Thumbnails are made (and cached) by the backend as each tile
 * scrolls into view; the full image is only loaded when opened. The list
 * refreshes while visible so shots taken in game show up without reopening
 * the tab. */
export function ScreenshotsPanel({ instanceId }: Props) {
  const [shots, setShots] = useState<ScreenshotFile[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [viewing, setViewing] = useState<number | null>(null);
  const [confirmDelete, setConfirmDelete] = useState(false);

  function refresh() {
    api
      .listScreenshots(instanceId)
      .then((list) => {
        setShots(list);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }

  useEffect(() => {
    setShots(null);
    setViewing(null);
    refresh();
    const stop = setVisibleInterval(refresh, 10000);
    return () => stop();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [instanceId]);

  const current = viewing !== null && shots ? shots[viewing] : null;

  useEffect(() => {
    if (viewing === null || !shots) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setViewing(null);
      if (e.key === "ArrowLeft") setViewing((v) => (v !== null && v > 0 ? v - 1 : v));
      if (e.key === "ArrowRight") setViewing((v) => (v !== null && v < shots.length - 1 ? v + 1 : v));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [viewing, shots]);

  const [copyState, setCopyState] = useState<"idle" | "copying" | "copied">("idle");
  useEffect(() => {
    setConfirmDelete(false);
    setCopyState("idle");
  }, [viewing]);

  async function handleCopy() {
    if (!current) return;
    setCopyState("copying");
    try {
      await api.copyScreenshot(instanceId, current.fileName);
      setCopyState("copied");
      setTimeout(() => setCopyState((s) => (s === "copied" ? "idle" : s)), 1500);
    } catch (e) {
      setCopyState("idle");
      setError(String(e));
    }
  }

  function runFileAction(action: (id: string, fileName: string) => Promise<void>) {
    if (current) action(instanceId, current.fileName).catch((e) => setError(String(e)));
  }

  // Full-size image for the viewer, fetched only for the one being looked at.
  const [fullSrc, setFullSrc] = useState<string | null>(null);
  useEffect(() => {
    setFullSrc(null);
    if (!current) return;
    let cancelled = false;
    api
      .readScreenshot(instanceId, current.fileName)
      .then((url) => !cancelled && setFullSrc(url))
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [instanceId, current?.fileName]);

  async function handleDelete() {
    if (!current || !shots) return;
    try {
      await api.deleteScreenshot(instanceId, current.fileName);
      const next = shots.filter((s) => s.fileName !== current.fileName);
      setShots(next);
      setViewing(next.length === 0 ? null : Math.min(viewing!, next.length - 1));
    } catch (e) {
      setError(String(e));
    }
  }

  async function openFolder() {
    try {
      await api.openFolder(await api.getScreenshotsDir(instanceId));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <>
      <div className="panel-header">
        <h4>Screenshots{shots && shots.length > 0 ? ` (${shots.length})` : ""}</h4>
        <div className="panel-actions">
          <button className="ghost-btn small" onClick={openFolder}>
            Open folder
          </button>
        </div>
      </div>
      {error && <div className="error-text">{error}</div>}
      <div className="screenshot-grid-scroll">
        {shots === null ? (
          <div className="placeholder">Loading…</div>
        ) : shots.length === 0 ? (
          <div className="placeholder">No screenshots yet - press F2 in game to take one.</div>
        ) : (
          <div className="screenshot-grid">
            {shots.map((shot, i) => (
              <ScreenshotThumb key={shot.fileName} instanceId={instanceId} shot={shot} onOpen={() => setViewing(i)} />
            ))}
          </div>
        )}
      </div>

      {current && shots && (
        <div className="modal-backdrop screenshot-viewer" onClick={() => setViewing(null)}>
          <div className="screenshot-viewer-inner" onClick={(e) => e.stopPropagation()}>
            {fullSrc ? (
              <img src={fullSrc} alt={current.fileName} />
            ) : (
              <div className="screenshot-viewer-loading">Loading…</div>
            )}
            {error && <div className="error-text screenshot-viewer-error">{error}</div>}
            <div className="screenshot-viewer-bar">
              <button className="ghost-btn small" disabled={viewing === 0} onClick={() => setViewing(viewing! - 1)}>
                ← Newer
              </button>
              <span className="screenshot-viewer-name">
                {current.fileName} · {formatDate(current.modifiedAt)} · {viewing! + 1}/{shots.length}
              </span>
              <button
                className="ghost-btn small"
                disabled={viewing === shots.length - 1}
                onClick={() => setViewing(viewing! + 1)}
              >
                Older →
              </button>
              <button className="ghost-btn small" onClick={handleCopy} disabled={copyState === "copying"} title="Copy the image to paste into Discord, etc.">
                {copyState === "copying" ? "Copying…" : copyState === "copied" ? "Copied!" : "Copy"}
              </button>
              <button className="ghost-btn small" onClick={() => runFileAction(api.openScreenshot)} title="Open in your image app to edit">
                Open
              </button>
              <button className="ghost-btn small" onClick={() => runFileAction(api.showScreenshotInFolder)} title="Show the file in your file manager">
                Show in folder
              </button>
              {confirmDelete ? (
                <button className="danger-btn small" onClick={handleDelete}>
                  Confirm delete
                </button>
              ) : (
                <button className="danger-btn small" onClick={() => setConfirmDelete(true)}>
                  Delete
                </button>
              )}
              <button className="ghost-btn small" onClick={() => setViewing(null)}>
                Close
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
