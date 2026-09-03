import { useState } from "react";
import { SkinCapeDialog } from "./SkinCapeDialog";
import type { GameProfile } from "../types";

interface Props {
  profile: GameProfile | null;
  onClose: () => void;
}

export function SettingsDialog({ profile, onClose }: Props) {
  const [showSkinCape, setShowSkinCape] = useState(false);

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>Settings</h3>
        <div className="subtitle">Configure Mint Launcher</div>

        {profile?.userType === "msa" && (
          <div className="form-field">
            <label>Account</label>
            <button type="button" className="ghost-btn" onClick={() => setShowSkinCape(true)}>
              Skin & Cape
            </button>
          </div>
        )}

        <div className="modal-actions">
          <button className="primary-btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>

      {showSkinCape && profile && (
        <SkinCapeDialog uuid={profile.uuid} onClose={() => setShowSkinCape(false)} />
      )}
    </div>
  );
}
