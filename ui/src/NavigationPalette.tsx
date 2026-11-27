import { useEffect, useId, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import type { State } from "./store";
import { navigationEntries, selectedIndex, type NavigationEntry } from "./navigation";

export function NavigationPalette({ state, onPick, onClose }: { state: State; onPick(entry: NavigationEntry): void; onClose(): void }) {
  const [query, setQuery] = useState("");
  // By id: sessions re-sort by recency while the palette is open.
  const [active, setActive] = useState<string | null>(null);
  const dialog = useRef<HTMLDivElement>(null);
  const listId = useId();
  const rows = navigationEntries(Object.values(state.sessions), state, query);
  const selected = selectedIndex(rows, active);
  const optionId = (i: number) => `${listId}-${i}`;

  useLayoutEffect(() => {
    dialog.current?.querySelector("input")?.focus();
  }, []);
  useEffect(() => {
    document.getElementById(optionId(selected))?.scrollIntoView({ block: "nearest" });
  }, [selected, query]);

  const onKey = (e: KeyboardEvent) => {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      onClose();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (rows.length) setActive(rows[(selected + (e.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length].id);
    } else if (e.key === "Enter" && e.target instanceof HTMLInputElement) {
      e.preventDefault();
      if (rows[selected]) onPick(rows[selected]);
    } else if (e.key === "Tab") {
      const controls = dialog.current?.querySelectorAll<HTMLElement>("input, button[aria-label=Close]");
      if (!controls?.length) return;
      const first = controls[0], last = controls[controls.length - 1];
      if ((e.shiftKey && document.activeElement === first) || (!e.shiftKey && document.activeElement === last)) {
        e.preventDefault();
        (e.shiftKey ? last : first).focus();
      }
    }
  };

  return createPortal(
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={dialog} className="modal navigation-palette" role="dialog" aria-modal="true" aria-label="Go to session or page" onKeyDown={onKey}>
        <header className="navigation-search">
          <input
            className="menu-search"
            role="combobox"
            aria-label="Search sessions and pages"
            aria-expanded="true"
            aria-autocomplete="list"
            aria-controls={listId}
            aria-activedescendant={rows.length ? optionId(selected) : undefined}
            placeholder="Search sessions and pages…"
            spellCheck={false}
            value={query}
            onChange={(e) => { setQuery(e.target.value); setActive(null); }}
          />
          <button type="button" className="btn-text" aria-label="Close" onClick={onClose}><span className="navigation-esc">Esc</span><span className="navigation-cancel">Cancel</span></button>
        </header>
        <div id={listId} className="navigation-results" role="listbox" aria-label="Sessions and pages">
          {rows.map((entry, i) => (
            <div key={entry.id}>
              {rows[i - 1]?.group !== entry.group && <div className="list-head" aria-hidden="true">{entry.group}</div>}
              <button
                type="button"
                className={`picker-item navigation-option ${selected === i ? "active" : ""}`}
                id={optionId(i)}
                role="option"
                aria-selected={selected === i}
                tabIndex={-1}
                onMouseDown={(e) => e.preventDefault()}
                onMouseMove={() => setActive(entry.id)}
                onClick={() => onPick(entry)}
              >
                <span className="navigation-title">{entry.title}</span>
                <span className="navigation-detail muted small">{entry.detail}</span>
              </button>
            </div>
          ))}
        </div>
        {!rows.length && <p className="navigation-empty muted" role="status">No sessions or pages match.</p>}
        {!state.connected && <p className="muted small" role="status">Connecting to sessions…</p>}
        <footer className="navigation-footer muted small"><span>↑ ↓ to navigate</span><span>Enter to open</span></footer>
      </div>
    </div>,
    document.body,
  );
}
