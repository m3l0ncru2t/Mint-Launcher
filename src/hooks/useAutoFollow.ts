import { useCallback, useEffect, useRef, useState } from "react";

// How close to the bottom (px) still counts as "at the bottom" - scroll
// position rarely lands on the exact pixel, and a reflow can leave a
// sub-pixel gap even when the user hasn't scrolled at all.
const BOTTOM_THRESHOLD_PX = 40;

/** Auto-follow that behaves like a normal chat/console window: stays pinned
 * to the bottom as new content arrives, but turns itself off the moment the
 * user scrolls up to read back through history (so new lines don't yank
 * them back down), and turns itself back on the moment they scroll back to
 * the bottom - without needing to hit the button again. The button next to
 * these panels still works as an explicit override either way. */
export function useAutoFollow<T extends HTMLElement>(trigger: unknown) {
  const ref = useRef<T>(null);
  const [autoFollow, setAutoFollow] = useState(true);
  // Scrolling to the bottom ourselves (below) also fires a native `scroll`
  // event - without this flag, that self-triggered event would look
  // indistinguishable from the user scrolling to the bottom, and every
  // follow-up snap would just re-confirm autoFollow=true, never letting a
  // real upward scroll turn it off cleanly on the same tick.
  const followingRef = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!autoFollow || !el) return;
    const bottom = el.scrollHeight - el.clientHeight;
    // Skip when already there - besides being a no-op, assigning scrollTop
    // to the value it already holds may not fire a `scroll` event at all,
    // which would leave `followingRef` stuck `true` and swallow the user's
    // very next real scroll as if it were this one.
    if (el.scrollTop === bottom) return;
    followingRef.current = true;
    el.scrollTop = bottom;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [trigger, autoFollow]);

  const onScroll = useCallback(() => {
    if (followingRef.current) {
      followingRef.current = false;
      return;
    }
    const el = ref.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight <= BOTTOM_THRESHOLD_PX;
    setAutoFollow(atBottom);
  }, []);

  return { ref, autoFollow, setAutoFollow, onScroll };
}
