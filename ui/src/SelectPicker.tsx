import { useRef, useState } from "react";
import { AnchoredPopover } from "./AnchoredPopover";
import { Icon } from "./App";

export function SelectPicker<T extends string | number>({ label, value, options, onChange }: {
  label: string;
  value: T;
  options: readonly { value: T; label: string }[];
  onChange(value: T): void;
}) {
  const anchor = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const selected = options.find((option) => option.value === value);
  return (
    <>
      <button
        ref={anchor}
        type="button"
        className="btn select-trigger"
        aria-label={label}
        aria-description={selected?.label}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault();
            setOpen(true);
          }
        }}
      >
        <span>{selected?.label}</span>
        <Icon d="m6 9 6 6 6-6" size={12} />
      </button>
      {open && (
        <AnchoredPopover anchor={anchor} label={label} variant="select-panel" onClose={() => setOpen(false)}>
          <div className="popover-heading"><strong>{label}</strong></div>
          <div className="choice-menu" role="menu" aria-label={label}>
            {options.map((option) => (
              <button
                key={option.value}
                type="button"
                className="picker-item"
                role="menuitemradio"
                aria-checked={option.value === value}
                onClick={() => { onChange(option.value); setOpen(false); }}
              >
                <span>{option.label}</span>
                {option.value === value && <Icon d="m5 12 5 5 9-11" size={13} />}
              </button>
            ))}
          </div>
        </AnchoredPopover>
      )}
    </>
  );
}
