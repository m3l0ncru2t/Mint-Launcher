import { useRef, useState, type DragEvent } from "react";

/**
 * Lets .jar files be dragged from the file manager straight onto a list.
 *
 * The window has Tauri's native file-drop turned off (`dragDropEnabled:
 * false`, so the sidebar's HTML5 drag-to-reorder keeps working), which means
 * a dropped file arrives here as an ordinary HTML5 `File` with its bytes
 * readable but no filesystem path - callers upload the contents.
 *
 * Only drags carrying files react, so in-app drags (sidebar rows) pass by.
 */
export function useJarDrop(onDrop: (jars: File[], rejected: string[]) => void, disabled = false) {
  const [active, setActive] = useState(false);
  // dragenter/dragleave fire for every child element crossed, so a plain
  // boolean would flicker off while moving over the rows - count depth.
  const depth = useRef(0);

  const carriesFiles = (e: DragEvent) => Array.from(e.dataTransfer.types).includes("Files");

  const props = {
    onDragEnter: (e: DragEvent) => {
      if (disabled || !carriesFiles(e)) return;
      e.preventDefault();
      depth.current += 1;
      setActive(true);
    },
    onDragOver: (e: DragEvent) => {
      if (disabled || !carriesFiles(e)) return;
      // Without this the browser refuses the drop (and WebKit may navigate
      // to the file instead).
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    },
    onDragLeave: (e: DragEvent) => {
      if (disabled || !carriesFiles(e)) return;
      depth.current = Math.max(0, depth.current - 1);
      if (depth.current === 0) setActive(false);
    },
    onDrop: (e: DragEvent) => {
      if (disabled || !carriesFiles(e)) return;
      e.preventDefault();
      depth.current = 0;
      setActive(false);
      const files = Array.from(e.dataTransfer.files);
      const jars = files.filter((f) => f.name.toLowerCase().endsWith(".jar"));
      const rejected = files.filter((f) => !jars.includes(f)).map((f) => f.name);
      if (files.length > 0) onDrop(jars, rejected);
    },
  };

  return { active, props };
}
