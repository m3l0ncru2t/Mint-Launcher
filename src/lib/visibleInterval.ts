/** `setInterval` that skips its ticks while the window is hidden/minimized
 * and runs once as soon as it's visible again. Most of this app's polling
 * (player count, TPS, stats, mod lists) exists purely to keep an on-screen
 * view fresh - and for a server, each poll also types a command into the
 * server's own console and adds lines to its log - so there's no point
 * doing any of it for a window nobody's looking at. Returns the stop
 * function (what `clearInterval` would've been). */
export function setVisibleInterval(fn: () => void, ms: number): () => void {
  const id = setInterval(() => {
    if (!document.hidden) fn();
  }, ms);
  const onVisible = () => {
    if (!document.hidden) fn();
  };
  document.addEventListener("visibilitychange", onVisible);
  return () => {
    clearInterval(id);
    document.removeEventListener("visibilitychange", onVisible);
  };
}
