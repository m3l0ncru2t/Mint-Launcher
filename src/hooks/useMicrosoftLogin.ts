import { useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api } from "../api";
import type { GameProfile, LoginUrlInfo } from "../types";

// Drives the "opens a browser, waits for the redirect" Microsoft sign-in
// flow - shared by the login screen and the account switcher's "add
// account" action so both get the same waiting-for-browser state.
//
// Closing the browser tab instead of finishing sign-in never sends anything
// back, so the backend command only gives up after a multi-minute timeout.
// `cancel()` resets the UI immediately and - critically - unregisters this
// attempt's event listener right away, instead of leaving it alive until
// the abandoned command's own timeout fires. Otherwise a cancelled
// attempt's listener stays subscribed in the background, and the next
// attempt's event would fire it too (each cancel+retry opening one more
// browser tab than the last).
export function useMicrosoftLogin() {
  const [busy, setBusy] = useState(false);
  const [waitingForBrowser, setWaitingForBrowser] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const cancelledRef = useRef(false);
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const inFlightRef = useRef(false);

  async function login(): Promise<GameProfile | null> {
    // A double-click (or any other re-entrant call before `busy` has
    // re-rendered) would otherwise start a second sign-in attempt while one
    // is already running. Each attempt registers its own listener for the
    // same "microsoft-login-open" event, and Tauri delivers an emitted
    // event to every listener currently subscribed - so with two listeners
    // alive, each backend attempt's event opens a browser tab from *both*
    // listeners, and completing sign-in in one of the resulting duplicate
    // tabs can hand Microsoft's token endpoint an authorization code that a
    // sibling tab already superseded (surfaces as an opaque AADSTS70000
    // "invalid code" error). Refusing to start a second attempt while one
    // is in flight prevents the duplicate listeners from ever existing.
    if (inFlightRef.current) return null;
    inFlightRef.current = true;

    setError(null);
    setWaitingForBrowser(false);
    setBusy(true);
    cancelledRef.current = false;
    const unlisten = await listen<LoginUrlInfo>("microsoft-login-open", (event) => {
      if (cancelledRef.current) return;
      setWaitingForBrowser(true);
      openUrl(event.payload.url);
    });
    unlistenRef.current = unlisten;
    try {
      const profile = await api.loginMicrosoft();
      return cancelledRef.current ? null : profile;
    } catch (e) {
      if (!cancelledRef.current) setError(String(e));
      return null;
    } finally {
      // Only clear the ref if a later cancel() hasn't already done so and
      // moved on to a newer attempt's listener.
      if (unlistenRef.current === unlisten) {
        unlisten();
        unlistenRef.current = null;
      }
      inFlightRef.current = false;
      if (!cancelledRef.current) {
        setBusy(false);
        setWaitingForBrowser(false);
      }
    }
  }

  function cancel() {
    cancelledRef.current = true;
    unlistenRef.current?.();
    unlistenRef.current = null;
    inFlightRef.current = false;
    setBusy(false);
    setWaitingForBrowser(false);
    setError(null);
  }

  return { login, cancel, busy, waitingForBrowser, error, setError };
}
