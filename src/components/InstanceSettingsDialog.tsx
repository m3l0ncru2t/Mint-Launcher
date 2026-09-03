import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { Select } from "./Select";
import type { AccountSummary, Instance } from "../types";

interface Props {
  instance: Instance;
  onClose: () => void;
  onSaved: (instance: Instance) => void;
  onIconChanged: (instance: Instance) => void;
}

export function InstanceSettingsDialog({ instance, onClose, onSaved, onIconChanged }: Props) {
  const [name, setName] = useState(instance.name);
  const [memoryMb, setMemoryMb] = useState(instance.memoryMb);
  const [javaArgs, setJavaArgs] = useState(instance.javaArgs ?? "");
  const [accountId, setAccountId] = useState(instance.accountId ?? "");
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [iconPreview, setIconPreview] = useState<string | null>(null);
  const [iconBusy, setIconBusy] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    api.listAccounts().then(setAccounts).catch(() => {});
  }, []);

  useEffect(() => {
    if (!instance.hasIcon) {
      setIconPreview(null);
      return;
    }
    api.getInstanceIcon(instance.id).then(setIconPreview).catch(() => {});
  }, [instance.id, instance.hasIcon]);

  async function handleIconFile(file: File) {
    setIconBusy(true);
    setError(null);
    try {
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as string);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
      });
      const base64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
      const updated = await api.setInstanceIcon(instance.id, base64);
      setIconPreview(dataUrl);
      onIconChanged(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setIconBusy(false);
    }
  }

  async function handleRemoveIcon() {
    setIconBusy(true);
    setError(null);
    try {
      const updated = await api.removeInstanceIcon(instance.id);
      setIconPreview(null);
      onIconChanged(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setIconBusy(false);
    }
  }

  async function handleSave() {
    if (!name.trim()) {
      setError("Instance name can't be empty");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      const updated = await api.updateInstanceSettings(
        instance.id,
        name.trim(),
        memoryMb,
        javaArgs.trim() || null,
        accountId || null,
      );
      onSaved(updated);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  const accountOptions = [
    { value: "", label: "Whichever account is signed in" },
    ...accounts.map((a) => ({ value: a.id, label: a.username })),
  ];

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <button className="modal-close-btn" title="Close" onClick={onClose}>
          ✕
        </button>
        <h3>Instance Settings</h3>
        <div className="subtitle">{instance.name}</div>

        {error && <div className="error-text">{error}</div>}

        <div className="form-field">
          <label>Icon</label>
          <div className="instance-icon-editor">
            {iconPreview ? (
              <img className="instance-icon-editor-preview" src={iconPreview} alt="" />
            ) : (
              <div className="instance-icon-editor-preview">{instance.name.slice(0, 1).toUpperCase()}</div>
            )}
            <div className="instance-icon-editor-actions">
              <button
                type="button"
                className="ghost-btn small"
                onClick={() => fileInputRef.current?.click()}
                disabled={iconBusy}
              >
                {iconBusy ? "Working…" : "Upload image"}
              </button>
              {iconPreview && (
                <button type="button" className="ghost-btn small" onClick={handleRemoveIcon} disabled={iconBusy}>
                  Remove
                </button>
              )}
            </div>
            <input
              ref={fileInputRef}
              type="file"
              accept="image/png,image/jpeg,image/gif,image/webp"
              style={{ display: "none" }}
              onChange={(e) => {
                const file = e.target.files?.[0];
                e.target.value = "";
                if (file) handleIconFile(file);
              }}
            />
          </div>
        </div>

        <div className="form-field">
          <label>Name</label>
          <input type="text" value={name} onChange={(e) => setName(e.target.value)} maxLength={64} />
        </div>

        <div className="form-field">
          <label>Memory allocation (MB)</label>
          <input
            type="number"
            min={512}
            max={32768}
            step={512}
            value={memoryMb}
            onChange={(e) => setMemoryMb(Number(e.target.value))}
          />
        </div>

        <div className="form-field">
          <label>Extra JVM arguments (optional)</label>
          <input
            type="text"
            placeholder="e.g. -XX:+UseG1GC"
            value={javaArgs}
            onChange={(e) => setJavaArgs(e.target.value)}
          />
        </div>

        <div className="form-field">
          <label>Account</label>
          <Select value={accountId} onChange={setAccountId} options={accountOptions} />
          <div className="hint">
            Always launch this instance as this account, regardless of whichever one is currently
            signed in - handy for running different instances under different accounts.
          </div>
        </div>

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Cancel
          </button>
          <button className="primary-btn" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
