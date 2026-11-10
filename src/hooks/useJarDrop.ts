import { useEffect, useRef, useState, type DragEvent } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

/** A dropped .jar: a File (Windows/macOS HTML5 drops) or a path on disk (Linux native drops). */
export interface DroppedJar {
  name: string;
  file?: File;
  path?: string;
}

const isJar = (name: string) => name.toLowerCase().endsWith(".jar");
const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path;

/**
 * Lets .jar files be dragged from the file manager straight onto a list. Attach `ref` and spread `props` onto
 * the list element.
 *
 * Two routes, because the platforms' webviews differ:
 * - Windows/macOS: the window has Tauri's native drop off (it breaks HTML5 drag-and-drop on Windows, which the
 *   sidebar's reordering uses), so drops arrive as ordinary HTML5 events carrying `File`s.
 * - Linux: WebKitGTK reports a file-manager drag as "text/uri-list" and hands the page neither the file nor its
 *   address on drop, so `tauri.linux.conf.json` turns native drop on and the paths come from
 *   `onDragDropEvent`. The HTML5 handlers still have to accept the drag, or the file manager cancels it.
 */
export function useJarDrop(onDrop: (jars: DroppedJar[], rejected: string[]) => void, disabled = false) {
  const [active, setActive] = useState(false);
  // dragenter/dragleave fire for every child element crossed, so a plain
  // boolean would flicker off while moving over the rows - count depth.
  const depth = useRef(0);
  const ref = useRef<HTMLDivElement>(null);
  // The native listener is registered once; these keep it seeing the latest callback and state.
  const onDropRef = useRef(onDrop);
  onDropRef.current = onDrop;
  const disabledRef = useRef(disabled);
  disabledRef.current = disabled;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "leave" || payload.type === "drop") {
          depth.current = 0;
          setActive(false);
        }
        if (payload.type !== "drop" || disabledRef.current || !ref.current) return;
        // A native drop is reported wherever it lands, so only take ones over this list. On Linux the
        // position is GTK's widget coordinates, which are CSS pixels - Tauri labels them physical but
        // doesn't scale them there.
        const { x, y } = payload.position;
        const rect = ref.current.getBoundingClientRect();
        if (x < rect.left || x > rect.right || y < rect.top || y > rect.bottom) return;
        const jars = payload.paths.filter(isJar).map((path) => ({ name: baseName(path), path }));
        const rejected = payload.paths.filter((path) => !isJar(path)).map(baseName);
        if (payload.paths.length > 0) onDropRef.current(jars, rejected);
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not running inside Tauri (e.g. a plain browser preview) - HTML5 drops still work.
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // "Files" everywhere except WebKitGTK, which only says "text/uri-list" for the same drag.
  // In-app drags (sidebar rows) carry text/plain only, so they pass by.
  const acceptsDrag = (e: DragEvent) => {
    const types = Array.from(e.dataTransfer.types);
    return types.includes("Files") || types.includes("text/uri-list");
  };

  const props = {
    onDragEnter: (e: DragEvent) => {
      if (disabled || !acceptsDrag(e)) return;
      e.preventDefault();
      depth.current += 1;
      setActive(true);
    },
    onDragOver: (e: DragEvent) => {
      if (disabled || !acceptsDrag(e)) return;
      // Without this the drop is refused (the file manager cancels it, and WebKit may try to open the file).
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    },
    onDragLeave: (e: DragEvent) => {
      if (disabled || !acceptsDrag(e)) return;
      depth.current = Math.max(0, depth.current - 1);
      if (depth.current === 0) setActive(false);
    },
    onDrop: (e: DragEvent) => {
      if (disabled || !acceptsDrag(e)) return;
      e.preventDefault();
      depth.current = 0;
      setActive(false);
      // On Linux the native drop above already has it (and this one carries no files).
      const files = Array.from(e.dataTransfer.files);
      if (files.length === 0) return;
      const jars = files.filter((f) => isJar(f.name)).map((file) => ({ name: file.name, file }));
      const rejected = files.filter((f) => !isJar(f.name)).map((f) => f.name);
      onDrop(jars, rejected);
    },
  };

  return { active, ref, props };
}
