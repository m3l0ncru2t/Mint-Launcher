import { useEffect, useRef, useState } from "react";
import { api } from "../api";
import { invalidatePlayerAvatar } from "./PlayerAvatar";
import type { ProfileDetails } from "../types";

interface Props {
  uuid: string;
  onClose: () => void;
}

// The front of the head sits at (8,8)-(16,16) on the 64x64 skin texture -
// cropped and scaled up via CSS background positioning for a quick,
// recognizable preview without needing full 3D skin rendering.
const FACE_DISPLAY_SIZE = 96;
const FACE_SCALE = FACE_DISPLAY_SIZE / 8;

export function SkinCapeDialog({ uuid, onClose }: Props) {
  const [details, setDetails] = useState<ProfileDetails | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [variant, setVariant] = useState<"classic" | "slim">("classic");
  const fileInputRef = useRef<HTMLInputElement>(null);

  function load() {
    setLoading(true);
    api
      .getProfileDetails()
      .then((d) => {
        setDetails(d);
        const active = d.skins.find((s) => s.state === "ACTIVE");
        if (active) setVariant(active.variant.toLowerCase() as "classic" | "slim");
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }

  useEffect(load, []);

  async function handleUpload(file: File) {
    setBusy(true);
    setError(null);
    try {
      const dataUrl = await new Promise<string>((resolve, reject) => {
        const reader = new FileReader();
        reader.onload = () => resolve(reader.result as string);
        reader.onerror = () => reject(reader.error);
        reader.readAsDataURL(file);
      });
      const base64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
      setDetails(await api.uploadSkin(variant, base64));
      invalidatePlayerAvatar(uuid);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleResetSkin() {
    setBusy(true);
    setError(null);
    try {
      await api.resetSkin();
      load();
      invalidatePlayerAvatar(uuid);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleSetCape(capeId: string | null) {
    setBusy(true);
    setError(null);
    try {
      if (capeId) {
        setDetails(await api.setCape(capeId));
      } else {
        await api.removeCape();
        load();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const activeSkin = details?.skins.find((s) => s.state === "ACTIVE");
  const activeCape = details?.capes.find((c) => c.state === "ACTIVE");

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <button className="modal-close-btn" title="Close" onClick={onClose}>
          ✕
        </button>
        <h3>Skin & Cape</h3>
        <div className="subtitle">Changes apply to your Microsoft account and show up everywhere you play.</div>

        {error && <div className="error-text">{error}</div>}

        {loading ? (
          <div className="placeholder">Loading…</div>
        ) : (
          <>
            <div className="form-field">
              <label>Skin</label>
              <div className="skin-preview-row">
                <div
                  className="skin-face-preview"
                  style={
                    activeSkin
                      ? {
                          backgroundImage: `url(${activeSkin.url})`,
                          backgroundSize: `${FACE_SCALE * 64}px ${FACE_SCALE * 64}px`,
                          backgroundPosition: `-${FACE_SCALE * 8}px -${FACE_SCALE * 8}px`,
                        }
                      : undefined
                  }
                />
                <div className="skin-preview-actions">
                  <div className="variant-toggle">
                    <button
                      type="button"
                      className={`ghost-btn small${variant === "classic" ? " selected" : ""}`}
                      onClick={() => setVariant("classic")}
                    >
                      Classic
                    </button>
                    <button
                      type="button"
                      className={`ghost-btn small${variant === "slim" ? " selected" : ""}`}
                      onClick={() => setVariant("slim")}
                    >
                      Slim
                    </button>
                  </div>
                  <div className="skin-preview-buttons">
                    <button
                      type="button"
                      className="ghost-btn small"
                      onClick={() => fileInputRef.current?.click()}
                      disabled={busy}
                    >
                      Upload skin
                    </button>
                    <button type="button" className="ghost-btn small" onClick={handleResetSkin} disabled={busy}>
                      Reset
                    </button>
                  </div>
                </div>
                <input
                  ref={fileInputRef}
                  type="file"
                  accept="image/png"
                  style={{ display: "none" }}
                  onChange={(e) => {
                    const file = e.target.files?.[0];
                    e.target.value = "";
                    if (file) handleUpload(file);
                  }}
                />
              </div>
              <div className="hint">Classic has wide arms (Steve), Slim has narrow arms (Alex).</div>
            </div>

            <div className="form-field">
              <label>Cape</label>
              {details && details.capes.length === 0 ? (
                <div className="hint">This account doesn't own any capes.</div>
              ) : (
                <div className="cape-list">
                  <button
                    type="button"
                    className={`cape-option${!activeCape ? " selected" : ""}`}
                    onClick={() => handleSetCape(null)}
                    disabled={busy}
                  >
                    No Cape
                  </button>
                  {details?.capes.map((c) => (
                    <button
                      type="button"
                      key={c.id}
                      className={`cape-option${c.state === "ACTIVE" ? " selected" : ""}`}
                      onClick={() => handleSetCape(c.id)}
                      disabled={busy}
                    >
                      {c.alias}
                    </button>
                  ))}
                </div>
              )}
            </div>
          </>
        )}

        <div className="modal-actions">
          <button className="ghost-btn" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
