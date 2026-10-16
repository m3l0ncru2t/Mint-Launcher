import type { MouseEvent } from "react";

/** onMouseEnter handler for text cut off with "…": shows the full text as a
 * tooltip, but only while it's actually truncated. */
export function showFullTextIfTruncated(e: MouseEvent<HTMLElement>) {
  const el = e.currentTarget;
  el.title = el.scrollWidth > el.clientWidth ? (el.textContent ?? "") : "";
}
