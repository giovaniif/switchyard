import { useEffect, useRef, useState } from "react";
import { Icon } from "./App";

export const COPY = "M9 9h11v11H9zM5 15H4V4h11v1";
const CHECK = "M5 12l5 5 9-11";

/** Put text on the clipboard. The page can be served over plain http on a
 *  private address, where `navigator.clipboard` does not exist: then the old
 *  selection copy still works. */
export async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return true;
    }
  } catch {
    // Refused (no focus, no permission): try the selection copy below.
  }
  const area = document.createElement("textarea");
  area.value = text;
  area.setAttribute("readonly", "");
  area.style.position = "fixed";
  area.style.opacity = "0";
  document.body.appendChild(area);
  area.select();
  let ok = false;
  try {
    ok = document.execCommand("copy");
  } catch {
    ok = false;
  }
  area.remove();
  return ok;
}

/** A small Copy button that confirms with a check for a moment. */
export function CopyButton({ text, label = "Copy", className = "" }: { text: string; label?: string; className?: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  return (
    <button
      type="button"
      className={`btn-icon copy-btn ${copied ? "copied" : ""} ${className}`}
      aria-label={copied ? "Copied" : label}
      title={copied ? "Copied" : label}
      onClick={async (e) => {
        // Inside rows that open and close on click.
        e.stopPropagation();
        if (await copyText(text)) {
          setCopied(true);
          clearTimeout(timer.current);
          timer.current = setTimeout(() => setCopied(false), 1500);
        }
      }}
    >
      <Icon d={copied ? CHECK : COPY} size={13} />
    </button>
  );
}
