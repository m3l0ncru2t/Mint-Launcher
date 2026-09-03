interface Props {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/** A styled stand-in for window.confirm(), which renders as a jarring
 * generic browser dialog under WebKitGTK. */
export function ConfirmDialog({
  title,
  message,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  danger,
  onConfirm,
  onCancel,
}: Props) {
  return (
    <div className="modal-backdrop" onClick={onCancel}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <h3>{title}</h3>
        <div className="subtitle">{message}</div>
        <div className="modal-actions">
          <button className="ghost-btn" onClick={onCancel}>
            {cancelLabel}
          </button>
          <button className={danger ? "danger-btn" : "primary-btn"} onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
