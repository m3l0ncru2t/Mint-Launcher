import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { useMicrosoftLogin } from "../hooks/useMicrosoftLogin";
import { PlayerAvatar } from "./PlayerAvatar";
import type { AccountSummary, GameProfile } from "../types";

interface Props {
  profile: GameProfile;
  onProfileChange: (profile: GameProfile) => void;
  onSignOut: () => void;
}

export function AccountSwitcher({ profile, onProfileChange, onSignOut }: Props) {
  const [open, setOpen] = useState(false);
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const ref = useRef<HTMLDivElement>(null);
  const msLogin = useMicrosoftLogin();

  function loadAccounts() {
    api.listAccounts().then(setAccounts).catch(() => {});
  }

  useEffect(() => {
    if (open) loadAccounts();
  }, [open]);

  useEffect(() => {
    if (!open) return;
    function handleClickOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  async function handleSwitch(id: string) {
    if (id === profile.uuid || busyId) return;
    setBusyId(id);
    setError(null);
    try {
      const next = await api.switchAccount(id);
      onProfileChange(next);
      setOpen(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusyId(null);
    }
  }

  async function handleRemove(id: string, e: React.MouseEvent) {
    e.stopPropagation();
    try {
      await api.removeAccount(id);
      loadAccounts();
      if (id === profile.uuid) onSignOut();
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleAdd() {
    setBusyId("__add__");
    setError(null);
    const next = await msLogin.login();
    setBusyId(null);
    if (next) {
      onProfileChange(next);
      loadAccounts();
      setOpen(false);
    }
  }

  return (
    <div className="account-switcher" ref={ref}>
      <button type="button" className="account-switcher-trigger" onClick={() => setOpen((o) => !o)}>
        <PlayerAvatar uuid={profile.uuid} username={profile.username} />
        <div className="account-info">
          <div className="account-name">{profile.username}</div>
          <div className="account-type">{profile.userType === "msa" ? "Microsoft" : "Offline"}</div>
        </div>
        <span className="custom-select-arrow">▾</span>
      </button>

      {open && (
        <div className="account-menu">
          {accounts.length === 0 && (
            <div className="custom-select-empty">No saved Microsoft accounts</div>
          )}
          {accounts.map((a) => (
            <div
              key={a.id}
              className={`account-menu-row${a.id === profile.uuid ? " selected" : ""}`}
              onClick={() => handleSwitch(a.id)}
            >
              <span className="account-menu-name">{a.username}</span>
              {busyId === a.id && <span className="hint-inline">Switching…</span>}
              <button className="icon-btn" title="Remove account" onClick={(e) => handleRemove(a.id, e)}>
                ✕
              </button>
            </div>
          ))}

          <button
            type="button"
            className="account-menu-add"
            onClick={
              msLogin.waitingForBrowser
                ? () => {
                    msLogin.cancel();
                    setBusyId(null);
                  }
                : handleAdd
            }
            disabled={busyId === "__add__" && !msLogin.waitingForBrowser}
          >
            {msLogin.waitingForBrowser
              ? "Waiting for browser… (cancel)"
              : busyId === "__add__"
                ? "Starting…"
                : "+ Add Microsoft account"}
          </button>

          {(error || msLogin.error) && <div className="error-text">{error ?? msLogin.error}</div>}

          <div className="account-menu-divider" />
          <button type="button" className="account-menu-signout" onClick={onSignOut}>
            Sign out
          </button>
        </div>
      )}
    </div>
  );
}
