import { useEffect, useRef, useState } from "react";

export interface SelectOption {
  value: string;
  label: string;
}

interface Props {
  value: string;
  onChange: (value: string) => void;
  options: SelectOption[];
  disabled?: boolean;
  placeholder?: string;
}

/**
 * A dependency-free custom dropdown. Native <select> popups render as
 * separate GTK windows under WebKitGTK, and with compositing disabled
 * (needed to work around a blank-window bug in VMs) their position
 * calculation breaks, so the popup can render outside the app window
 * entirely. Rendering the menu ourselves keeps it inside our own DOM.
 */
export function Select({ value, onChange, options, disabled, placeholder }: Props) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function handleClickOutside(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [open]);

  const selected = options.find((o) => o.value === value);

  return (
    <div className="custom-select" ref={ref}>
      <button
        type="button"
        className="custom-select-trigger"
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
      >
        <span>{selected?.label ?? placeholder ?? "Select…"}</span>
        <span className="custom-select-arrow">▾</span>
      </button>
      {open && !disabled && (
        <div className="custom-select-menu">
          {options.length === 0 && <div className="custom-select-empty">No options</div>}
          {options.map((o) => (
            <button
              type="button"
              key={o.value}
              className={`custom-select-option${o.value === value ? " selected" : ""}`}
              onClick={() => {
                onChange(o.value);
                setOpen(false);
              }}
            >
              {o.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
