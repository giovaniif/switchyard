import { useId, useState, type RefObject } from "react";
import { AnchoredPopover } from "./AnchoredPopover";
import { SESSION_TITLE_LIMIT } from "./protocol";

export function SessionRename({ anchor, title, onSave, onClose }: {
  anchor: RefObject<HTMLButtonElement | null>;
  title: string;
  onSave(title: string): Promise<{ error?: string }>;
  onClose(): void;
}) {
  const [draft, setDraft] = useState(title);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const noteId = useId();
  const next = draft.trim();
  const tooLong = Array.from(next).length > SESSION_TITLE_LIMIT;
  const canSave = !saving && !!next && !tooLong && next !== title;

  return (
    <AnchoredPopover anchor={anchor} label="Rename session" onClose={() => { if (!saving) onClose(); }}>
      <form className="session-rename-form" onSubmit={async (event) => {
        event.preventDefault();
        if (!canSave) return;
        setSaving(true);
        setError("");
        const result = await onSave(next);
        if (result.error) {
          setError(result.error);
          setSaving(false);
        } else {
          onClose();
        }
      }}>
        <div className="popover-heading"><strong>Rename session</strong></div>
        <label>
          Session name
          <input
            className="field"
            value={draft}
            readOnly={saving}
            aria-invalid={tooLong || undefined}
            aria-describedby={tooLong ? noteId : undefined}
            onChange={(event) => { setDraft(event.target.value); setError(""); }}
            onFocus={(event) => event.currentTarget.select()}
          />
        </label>
        {tooLong && <p id={noteId} className="bad">Use {SESSION_TITLE_LIMIT} characters or fewer.</p>}
        {error && <p className="bad" role="alert">{error}</p>}
        <div className="popover-footer">
          <button className="btn-text" type="button" disabled={saving} onClick={onClose}>Cancel</button>
          <button className="btn-primary" type="submit" disabled={!canSave}>{saving ? "Saving…" : "Save name"}</button>
        </div>
      </form>
    </AnchoredPopover>
  );
}
