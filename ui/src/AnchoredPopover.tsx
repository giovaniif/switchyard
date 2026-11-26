import { createContext, useContext, useLayoutEffect, useRef, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";

const Depth = createContext(0);
const FOCUSABLE = "button:not(:disabled), input:not(:disabled), select:not(:disabled), a[href], [tabindex='0']";

export function AnchoredPopover({ anchor, label, children, onClose, onEscape, variant = "" }: {
  anchor: RefObject<HTMLElement | null>;
  label: string;
  children: ReactNode;
  onClose(): void;
  onEscape?(): void;
  variant?: string;
}) {
  const depth = useContext(Depth);
  const layer = 60 + depth * 2;
  const panel = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  const escape = useRef(onEscape);
  escape.current = onEscape;
  const usage = variant === "usage-popover";
  useLayoutEffect(() => {
    const el = panel.current;
    const trigger = anchor.current;
    if (!el || !trigger) return;
    const viewport = window.visualViewport;
    const position = () => {
      const width = viewport?.width ?? innerWidth;
      const height = viewport?.height ?? innerHeight;
      const leftEdge = viewport?.offsetLeft ?? 0;
      const topEdge = viewport?.offsetTop ?? 0;
      const phone = width <= 700;
      const r = trigger.getBoundingClientRect();
      const above = r.top - topEdge - 14;
      const below = topEdge + height - r.bottom - 14;
      const usageBelow = below >= 400 || below >= above;
      const room = Math.max(above, below);
      el.style.maxHeight = `${phone ? Math.max(0, Math.min(height - 16, height * .88)) : usage ? Math.min(480, Math.max(0, usageBelow ? below : above)) : room >= 96 ? room : height - 16}px`;
      el.style.maxWidth = `${Math.max(0, width - (phone && usage ? 0 : 16))}px`;
      el.style.width = phone && usage ? `${width}px` : "";
      const box = el.getBoundingClientRect();
      if (phone) {
        const inset = usage ? 0 : 8;
        el.style.left = `${leftEdge + inset}px`;
        el.style.top = `${topEdge + height - box.height - inset}px`;
        return;
      }
      const left = usage && (r.left + r.right) / 2 > leftEdge + width / 2 ? r.right - box.width : r.left;
      const opensAbove = above >= box.height + 2 || above > below;
      const top = usage ? (usageBelow ? r.bottom + 6 : r.top - box.height - 6)
        : opensAbove ? r.top - box.height - 8 : r.bottom + 8;
      el.style.left = `${Math.max(leftEdge + 8, Math.min(left, leftEdge + width - box.width - 8))}px`;
      el.style.top = `${Math.max(topEdge + 8, Math.min(top, topEdge + height - box.height - 8))}px`;
    };
    position();
    (el.querySelector<HTMLElement>("[aria-checked=true], [aria-current=true]") ?? [...el.querySelectorAll<HTMLElement>(FOCUSABLE)].find((item) => item.getClientRects().length) ?? el).focus({ preventScroll: true });
    const resize = new ResizeObserver(position);
    resize.observe(el);
    window.addEventListener("resize", position);
    window.addEventListener("scroll", position, true);
    viewport?.addEventListener("resize", position);
    viewport?.addEventListener("scroll", position);
    const key = (event: KeyboardEvent) => {
      const dialogs = document.querySelectorAll("[data-anchored-panel]");
      if (dialogs[dialogs.length - 1] !== el) return;
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        (escape.current ?? close.current)();
        return;
      }
      const active = document.activeElement;
      const focusable = [...el.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((item) => item.getClientRects().length);
      if (event.key === "Tab") {
        const first = focusable[0], last = focusable[focusable.length - 1];
        if (event.shiftKey ? active === first || active === el : active === last) {
          event.preventDefault();
          (event.shiftKey ? last : first)?.focus();
        }
        return;
      }
      if (active instanceof HTMLInputElement || !["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
      const options = focusable.filter((item) => item.matches("[role=menuitem], [role=menuitemradio], .picker-item, .picker-fold, .setup-options button"));
      if (!options.length) return;
      const index = options.findIndex((item) => item === active);
      const next = event.key === "Home" ? 0 : event.key === "End" ? options.length - 1
        : event.key === "ArrowUp" ? (index < 1 ? options.length - 1 : index - 1) : (index + 1) % options.length;
      event.preventDefault();
      options[next].focus();
    };
    document.addEventListener("keydown", key, true);
    return () => {
      resize.disconnect();
      window.removeEventListener("resize", position);
      window.removeEventListener("scroll", position, true);
      viewport?.removeEventListener("resize", position);
      viewport?.removeEventListener("scroll", position);
      document.removeEventListener("keydown", key, true);
      if (trigger.isConnected) trigger.focus({ preventScroll: true });
    };
  }, [anchor, usage]);
  return createPortal(
    <>
      <div className="anchored-scrim" style={{ zIndex: layer }} onPointerDown={onClose} />
      <div ref={panel} data-anchored-panel="" className={`anchored-popover ${variant}`} role="dialog" aria-modal="true" aria-label={label} tabIndex={-1} style={{ zIndex: layer + 1 }}>
        <Depth.Provider value={depth + 1}>{children}</Depth.Provider>
      </div>
    </>,
    document.body,
  );
}
