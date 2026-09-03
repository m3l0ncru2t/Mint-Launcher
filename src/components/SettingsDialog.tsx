import { useState } from "react";
import { api } from "../api";
import type { Settings } from "../types";

interface Props {
  settings: Settings;
  onClose: () => void;
  onSaved: (settings: Settings) => void;
}

export function SettingsDialog({ settings, onClose, onSaved }: Props) {
  const [clientId, setClientId] = useState(settings.microsoftClientId ?? "");
  const [saving, setSaving] = useState(false);

  async function handleSave() {
    setSaving(true);
    try {
      const trimmed = clientId.trim() || null;
      await api.setMicrosoftClientId(trimmed);
      onSaved({ ...settings, microsoftClientId: trimmed });
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>Settings</h3>
        <div className="subtitle">Configure Mint Launcher</div>

        <div className="form-field">
          <label>Microsoft Application (client) ID (optional)</label>
          <input
            type="text"
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
            placeholder="Using Mint Launcher's default"
          />
          <div className="hint">
            Microsoft login already works out of the box using Mint Launcher's own app
            registration. Only set this if you want to use your own instead - register a free
            app at portal.azure.com (Azure Active Directory → App registrations → New
            registration), enable "Allow public client flows" under Authentication, and paste
            the Application (client) ID here.
          </div>
        </div>

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Close
          </button>
          <button className="primary-btn" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
