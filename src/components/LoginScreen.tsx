import { useEffect, useState } from "react";
import appIcon from "../assets/app-icon.png";
import { api } from "../api";
import { useMicrosoftLogin } from "../hooks/useMicrosoftLogin";
import { PlayerAvatar } from "./PlayerAvatar";
import { UpdateBanner } from "./UpdateBanner";
import type { AccountSummary, GameProfile, Settings } from "../types";

interface Props {
  settings: Settings;
  onLoggedIn: (profile: GameProfile) => void;
}

export function LoginScreen({ settings, onLoggedIn }: Props) {
  const [tab, setTab] = useState<"offline" | "microsoft">("microsoft");
  const [username, setUsername] = useState(settings.offlineUsername ?? "");
  const [offlineError, setOfflineError] = useState<string | null>(null);
  const [offlineBusy, setOfflineBusy] = useState(false);
  const msLogin = useMicrosoftLogin();

  const [savedAccounts, setSavedAccounts] = useState<AccountSummary[]>([]);
  const [signingInId, setSigningInId] = useState<string | null>(null);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [savedAccountError, setSavedAccountError] = useState<string | null>(null);

  function loadSavedAccounts() {
    api.listAccounts().then(setSavedAccounts).catch(() => {});
  }

  useEffect(() => {
    loadSavedAccounts();
  }, []);

  async function handleOfflineLogin() {
    setOfflineError(null);
    setOfflineBusy(true);
    try {
      const profile = await api.loginOffline(username);
      onLoggedIn(profile);
    } catch (e) {
      setOfflineError(String(e));
    } finally {
      setOfflineBusy(false);
    }
  }

  async function handleSignInSaved(id: string) {
    setSigningInId(id);
    setSavedAccountError(null);
    try {
      const profile = await api.switchAccount(id);
      onLoggedIn(profile);
    } catch (e) {
      setSavedAccountError(String(e));
    } finally {
      setSigningInId(null);
    }
  }

  // Lets a broken/stuck saved account (e.g. one that keeps hitting a stale
  // sign-in error - see AADSTS70000 reports) be cleared from right here,
  // instead of needing to sign in successfully first to reach the account
  // switcher's own remove button.
  async function handleRemoveSaved(id: string, e: React.MouseEvent) {
    e.stopPropagation();
    setRemovingId(id);
    setSavedAccountError(null);
    try {
      await api.removeAccount(id);
      loadSavedAccounts();
    } catch (e) {
      setSavedAccountError(String(e));
    } finally {
      setRemovingId(null);
    }
  }

  async function handleMicrosoftLogin() {
    const profile = await msLogin.login();
    if (profile) onLoggedIn(profile);
  }

  return (
    <div className="login-screen">
      <UpdateBanner />
      <div className="login-card">
        <img src={appIcon} alt="" className="logo" />
        <h2>Mint Launcher</h2>
        <div className="subtitle">Sign in to start playing</div>

        <div className="login-tabs">
          <button className={tab === "offline" ? "active" : ""} onClick={() => setTab("offline")}>
            Offline
          </button>
          <button className={tab === "microsoft" ? "active" : ""} onClick={() => setTab("microsoft")}>
            Microsoft
          </button>
        </div>

        {tab === "offline" && (
          <>
            <div className="form-field">
              <input
                type="text"
                placeholder="Username"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                maxLength={16}
              />
              <div className="hint">
                Local play only - no real account, skins, or online servers that require auth.
              </div>
            </div>
            <button
              className="primary-btn"
              style={{ width: "100%" }}
              onClick={handleOfflineLogin}
              disabled={offlineBusy || !username.trim()}
            >
              {offlineBusy ? "Signing in…" : "Play Offline"}
            </button>
            {offlineError && <div className="error-text">{offlineError}</div>}
          </>
        )}

        {tab === "microsoft" && (
          <>
            {savedAccounts.length > 0 && !msLogin.waitingForBrowser && (
              <div className="saved-accounts">
                {savedAccounts.map((a) => (
                  <div
                    key={a.id}
                    className={`saved-account-row${signingInId !== null || removingId !== null ? " disabled" : ""}`}
                    onClick={() => {
                      if (signingInId !== null || removingId !== null) return;
                      handleSignInSaved(a.id);
                    }}
                  >
                    <PlayerAvatar uuid={a.id} username={a.username} />
                    <span className="saved-account-name">{a.username}</span>
                    {signingInId === a.id && <span className="hint-inline">Signing in…</span>}
                    {removingId === a.id ? (
                      <span className="hint-inline">Removing…</span>
                    ) : (
                      <button
                        type="button"
                        className="icon-btn"
                        title="Remove account"
                        onClick={(e) => handleRemoveSaved(a.id, e)}
                        disabled={signingInId !== null || removingId !== null}
                      >
                        ✕
                      </button>
                    )}
                  </div>
                ))}
              </div>
            )}
            {savedAccountError && <div className="error-text">{savedAccountError}</div>}

            {msLogin.waitingForBrowser ? (
              <div className="device-code-box">
                <div className="hint">A browser window has opened - finish signing in there.</div>
                <div className="hint">Waiting for you to sign in…</div>
                <button className="ghost-btn small" style={{ marginTop: 10 }} onClick={msLogin.cancel}>
                  Cancel
                </button>
              </div>
            ) : (
              <button
                className={savedAccounts.length > 0 ? "ghost-btn" : "primary-btn"}
                style={{ width: "100%" }}
                onClick={handleMicrosoftLogin}
                disabled={msLogin.busy || signingInId !== null}
              >
                {msLogin.busy
                  ? "Starting…"
                  : savedAccounts.length > 0
                    ? "+ Use another account"
                    : "Sign in with Microsoft"}
              </button>
            )}
            {msLogin.error && <div className="error-text">{msLogin.error}</div>}
          </>
        )}
      </div>
    </div>
  );
}
