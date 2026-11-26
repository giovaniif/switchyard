import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import "@xterm/xterm/css/xterm.css";
import type { TermInfo } from "./protocol";
import type { Actions } from "./store";
import { SELECTION_CAP, snippetInComposer } from "./compose";
import { joinRows, pickRows, type RowPick } from "./termRows";
import { AnchoredPopover } from "./AnchoredPopover";

// ANSI colors that stay readable on a white ground.
const THEME = {
  background: "#ffffff",
  foreground: "#1a1a1a",
  cursor: "#1a1a1a",
  cursorAccent: "#ffffff",
  selectionBackground: "#dcdcdc",
  black: "#1a1a1a",
  red: "#b42318",
  green: "#17803d",
  yellow: "#8a5a00",
  blue: "#1d4ed8",
  magenta: "#7e22ce",
  cyan: "#0e7490",
  white: "#666666",
  brightBlack: "#8a8a8a",
  brightRed: "#d92d20",
  brightGreen: "#1f9d55",
  brightYellow: "#a86a00",
  brightBlue: "#2563eb",
  brightMagenta: "#9333ea",
  brightCyan: "#0891b2",
  brightWhite: "#3a3a3a",
};

const EXITED_LINE = "\r\n\x1b[2m[shell exited, close this tab to remove it]\x1b[0m\r\n";

const decode = (b64: string) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));

const Glyph = ({ d, size = 12 }: { d: string; size?: number }) => (
  <svg aria-hidden="true" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d={d} />
  </svg>
);
const PLUS = "M12 5v14M5 12h14";
const CHEVRON = "m6 9 6 6 6-6";

/** Reads a terminal's rows as text, for attaching to the reply. */
type RowReader = (pick: RowPick) => string;
const CROSS = "M18 6 6 18M6 6l12 12";

/**
 * The session's terminals, one tab each. yardd owns the PTYs and their
 * scrollback; every tab keeps its own xterm while this pane lives, so
 * switching is instant and a reconnect only replays what was missed.
 */
export function TerminalPane(props: { sessionId: string; connected: boolean; visible: boolean; actions: Actions }) {
  const { sessionId, connected, visible, actions } = props;
  const [tabs, setTabs] = useState<TermInfo[] | null>(null);
  const [active, setActive] = useState<string | null>(null);
  const [selection, setSelection] = useState("");
  const [openError, setOpenError] = useState("");
  // With no terminal on screen nothing has measured the pane, and a shell
  // started at 80x24 prints its prompt for that width; the resize that
  // follows makes readline redraw and leaves a cut-off copy above the real
  // prompt. So a placeholder view fits the pane first and sends this op
  // with its size.
  const [pending, setPending] = useState<"term.open" | "term.new" | null>(null);
  // Terminal 1 starts by itself once; after that an empty list stays empty.
  const started = useRef(false);
  const dims = useRef({ cols: 80, rows: 24 });
  const shown = useRef<TermInfo[]>([]);
  shown.current = tabs ?? [];
  // After `+`, the tabs that existed then: the first `term.opened` for another
  // terminal is the new one (an attach reply is for a tab we already had).
  const awaiting = useRef<Set<string> | null>(null);
  const readers = useRef(new Map<string, RowReader>());
  const [menu, setMenu] = useState(false);
  const menuAnchor = useRef<HTMLButtonElement>(null);
  const expectNew = () => (awaiting.current = new Set(shown.current.map((t) => t.termId)));

  useEffect(() => {
    if (!connected) return;
    const stop = actions.onTerm((msg) => {
      if (msg.sessionId !== sessionId) return;
      if (msg.op === "term.error") {
        awaiting.current = null;
        setOpenError(msg.message || "Unable to open a terminal.");
      } else if (msg.op === "term.list") {
        const list: TermInfo[] = msg.terms ?? [];
        setTabs(list);
        if (list.length === 0 && !started.current) {
          started.current = true;
          setPending("term.open");
        }
        if (list.length > 0) started.current = true;
      } else if (msg.op === "term.listChanged") {
        actions.post({ op: "term.list", sessionId });
      } else if (msg.op === "term.opened" && awaiting.current && !awaiting.current.has(msg.termId)) {
        awaiting.current = null;
        setActive(msg.termId);
        // The list may not have caught up yet; show the tab right away.
        setTabs((list) =>
          list?.some((t) => t.termId === msg.termId) ? list : [...(list ?? []), { termId: msg.termId, title: msg.title, exited: msg.exited }],
        );
      }
    });
    actions.post({ op: "term.list", sessionId });
    return stop;
  }, [sessionId, connected]);

  // Keep a tab selected: the first one, unless the selected one is still there.
  useEffect(() => {
    if (!tabs) return;
    if (!active || !tabs.some((t) => t.termId === active)) setActive(tabs[0]?.termId ?? null);
  }, [tabs, active]);

  const newTerminal = () => {
    setOpenError("");
    if (!shown.current.length) {
      setPending("term.new");
      return;
    }
    expectNew();
    actions.post({ op: "term.new", sessionId, ...dims.current });
  };
  const openFitted = (cols: number, rows: number) => {
    if (!pending) return;
    expectNew();
    actions.post({ op: pending, sessionId, cols, rows });
    setPending(null);
  };
  const closeTerminal = (termId: string) => {
    const list = tabs ?? [];
    const i = list.findIndex((t) => t.termId === termId);
    const rest = list.filter((t) => t.termId !== termId);
    setTabs(rest);
    setActive(rest[Math.min(i, rest.length - 1)]?.termId ?? null);
    actions.post({ op: "term.close", termId });
  };
  const cut = selection.length > SELECTION_CAP;
  const activeTab = (tabs ?? []).find((t) => t.termId === active);
  const attach = (text: string) => {
    setMenu(false);
    // Tabs are titled by number; "Terminal 2" says where the rows came from.
    snippetInComposer(sessionId, activeTab?.title ? `Terminal ${activeTab.title}` : "Terminal", text);
  };
  const attachRows = (pick: RowPick) => {
    const read = active ? readers.current.get(active) : undefined;
    if (read) attach(read(pick));
  };

  return (
    <div className="term-pane" style={{ display: visible ? "flex" : "none" }}>
      <div className="term-strip" role="tablist" aria-label="Terminals">
        {(tabs ?? []).map((t) => {
          const on = t.termId === active;
          return (
            <span key={t.termId} className={`term-tab ${on ? "active" : ""} ${t.exited ? "exited" : ""}`}>
              <button role="tab" aria-selected={on} title={t.exited ? "Shell exited" : undefined} onClick={() => setActive(t.termId)}>
                {t.title}
              </button>
              {on && (
                <button className="term-x" aria-label="Close terminal" title="Close terminal" onClick={() => closeTerminal(t.termId)}>
                  <Glyph d={CROSS} size={10} />
                </button>
              )}
            </span>
          );
        })}
        <button className="btn-icon" aria-label="New terminal" title="New terminal" disabled={!connected} onClick={newTerminal}>
          <Glyph d={PLUS} />
        </button>
        <span className="spacer" />
        {active && (
          <span className="term-attach">
            <button ref={menuAnchor} className="btn-text" aria-haspopup="dialog" aria-expanded={menu} title="Attach terminal output to your reply" onClick={() => setMenu(!menu)}>
              Attach
              <Glyph d={CHEVRON} size={11} />
            </button>
            {menu && (
              <AnchoredPopover anchor={menuAnchor} label="Attach terminal output" variant="select-panel" onClose={() => setMenu(false)}>
                <div role="menu">
                  {selection.trim() && (
                    <button
                      className="picker-item menu-item"
                      role="menuitem"
                      title={cut ? `The selection is ${selection.length.toLocaleString()} characters; only the first ${SELECTION_CAP.toLocaleString()} are attached.` : undefined}
                      onClick={() => attach(selection)}
                    >
                      Selection
                    </button>
                  )}
                  <button className="picker-item menu-item" role="menuitem" onClick={() => attachRows({ kind: "screen" })}>
                    Rows on screen
                  </button>
                  <button className="picker-item menu-item" role="menuitem" onClick={() => attachRows({ kind: "last", n: 100 })}>
                    Last 100 lines
                  </button>
                  <button className="picker-item menu-item" role="menuitem" onClick={() => attachRows({ kind: "last", n: 500 })}>
                    Last 500 lines
                  </button>
                </div>
              </AnchoredPopover>
            )}
          </span>
        )}
      </div>
      {/* Closing the last tab leaves no terminal; a blank pane with only the
          small `+` read as broken. Not while one is on its way (the first
          list of a new session is empty too). */}
      {openError && (
        <div className="pane-body terminal-error" role="status">
          <strong>Terminal unavailable</strong>
          <p className="muted">{openError}</p>
          <div><button className="btn" disabled={!connected} onClick={newTerminal}>Try again</button></div>
        </div>
      )}
      {tabs?.length === 0 && !awaiting.current && !pending && !openError && (
        <div className="pane-body">
          <div className="muted">No terminal is open.</div>
          <div>
            <button className="btn" disabled={!connected} onClick={newTerminal}>
              Open a terminal
            </button>
          </div>
        </div>
      )}
      {pending && (
        <TermView
          key="pending"
          termId=""
          connected={connected}
          visible={visible}
          actions={actions}
          onSize={() => {}}
          onReader={() => {}}
          onOpen={openFitted}
        />
      )}
      {(tabs ?? []).map((t) => (
        <TermView
          key={t.termId}
          termId={t.termId}
          connected={connected}
          visible={visible && t.termId === active}
          actions={actions}
          onSelection={t.termId === active ? setSelection : undefined}
          onSize={(cols, rows) => (dims.current = { cols, rows })}
          onReader={(read) => (read ? readers.current.set(t.termId, read) : readers.current.delete(t.termId))}
        />
      ))}
    </div>
  );
}

/** One terminal. Built once per tab; re-attaches after a reconnect. */
function TermView(props: {
  termId: string;
  connected: boolean;
  visible: boolean;
  actions: Actions;
  onSelection?: (text: string) => void;
  onSize(cols: number, rows: number): void;
  onReader(read: RowReader | null): void;
  /** A placeholder with no PTY yet: its first fit opens one at that size. */
  onOpen?(cols: number, rows: number): void;
}) {
  const { termId, connected, visible, actions } = props;
  const host = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const live = useRef(props);
  live.current = props;
  // Bytes written so far, in the PTY's own numbering; -1 before the first
  // scrollback snapshot. Chunks can overlap the snapshot or arrive before it.
  const written = useRef(-1);
  const exited = useRef(false);
  // Set by the first fit; attaching waits for it.
  const sized = useRef(false);
  const wasConnected = useRef(connected);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const term = new Terminal({
      fontFamily: '"JetBrains Mono", ui-monospace, monospace',
      fontSize: 12,
      lineHeight: 1.35,
      cursorBlink: true,
      scrollback: 5000,
      theme: THEME,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    termRef.current = term;
    sized.current = false;
    // Nothing attaches before the first real fit. Attached at xterm's default
    // 80 columns, the PTY was resized to 80 and the shell redrew its prompt
    // for that width into the scrollback; a snapshot written before the fit
    // was reflowed afterwards. Either way a reload brought wrapped lines back
    // garbled. Right after a reload the renderer has no cell size yet, so the
    // fit is retried once per frame for a while (one chain at a time).
    let tries = 0;
    let waiting = false;
    let gone = false;
    const attach = () => {
      if (live.current.onOpen) live.current.onOpen(term.cols, term.rows);
      else if (live.current.connected) live.current.actions.post({ op: "term.attach", termId, cols: term.cols, rows: term.rows });
    };
    const refit = () => {
      if (gone) return;
      // Fitting a hidden element would size the PTY to nothing.
      if (el.offsetWidth === 0 || el.offsetHeight === 0) return;
      let ok = false;
      try {
        if (fit.proposeDimensions()) {
          fit.fit();
          ok = true;
        }
      } catch {
        /* not laid out yet */
      }
      if (sized.current) return;
      // Better a reflowed terminal than none at all.
      if (ok || ++tries > 120) {
        sized.current = true;
        attach();
      } else if (!waiting) {
        waiting = true;
        requestAnimationFrame(() => {
          waiting = false;
          refit();
        });
      }
    };
    refit();

    let early: { offset: number; data: string }[] = [];
    const write = (msg: { offset: number; data: string }, replay = false) => {
      const bytes = decode(msg.data);
      const end = msg.offset + bytes.length;
      if (end <= written.current) return;
      const unread = msg.offset < written.current ? bytes.subarray(written.current - msg.offset) : bytes;
      if (replay) {
        // xterm can answer queries while parsing history. Keep those replies
        // out of the shell without suppressing adjacent live output.
        term.write("", () => { term.options.disableStdin = true; });
        term.write(unread, () => { term.options.disableStdin = false; });
      } else {
        term.write(unread);
      }
      written.current = end;
    };
    const markExited = () => {
      if (exited.current) return;
      exited.current = true;
      term.write(EXITED_LINE);
    };

    const stop = live.current.actions.onTerm((msg) => {
      if (msg.termId !== termId) return;
      if (msg.op === "term.opened") {
        if (written.current < 0) written.current = msg.offset;
        write(msg, true);
        early.forEach((chunk) => write(chunk));
        early = [];
        if (msg.exited) markExited();
        else live.current.actions.post({ op: "term.resize", termId, cols: term.cols, rows: term.rows });
      } else if (msg.op === "term.data") {
        if (written.current < 0) early.push(msg);
        else write(msg);
      } else if (msg.op === "term.exit") {
        markExited();
      }
    });

    const onData = term.onData((data) => {
      if (!exited.current && !live.current.onOpen) live.current.actions.post({ op: "term.input", termId, data });
    });
    const onResize = term.onResize(({ cols, rows }) => {
      live.current.onSize(cols, rows);
      if (!exited.current && !live.current.onOpen) live.current.actions.post({ op: "term.resize", termId, cols, rows });
    });
    const onSelect = term.onSelectionChange(() => live.current.onSelection?.(term.getSelection()));
    live.current.onReader((pick) => {
      const b = term.buffer.active;
      const [start, end] = pickRows({ length: b.length, viewportY: b.viewportY, rows: term.rows }, pick);
      const rows = [];
      for (let i = start; i < end; i++) {
        const line = b.getLine(i);
        if (!line) continue;
        // A row that wraps onto the next keeps its trailing spaces: they are
        // part of the line, not padding.
        rows.push({ text: line.translateToString(!b.getLine(i + 1)?.isWrapped), wrapped: line.isWrapped });
      }
      return joinRows(rows);
    });
    const observer = new ResizeObserver(refit);
    observer.observe(el);

    return () => {
      gone = true;
      stop();
      onData.dispose();
      onResize.dispose();
      onSelect.dispose();
      live.current.onReader(null);
      observer.disconnect();
      term.dispose();
      termRef.current = null;
    };
  }, [termId]);

  // Reattach after a reconnect, for what was missed (offsets drop the
  // overlap). The first attach comes from the first fit, above.
  useEffect(() => {
    const back = connected && !wasConnected.current;
    wasConnected.current = connected;
    const term = termRef.current;
    if (back && term && sized.current && !props.onOpen) actions.post({ op: "term.attach", termId, cols: term.cols, rows: term.rows });
  }, [termId, connected]);

  useEffect(() => {
    const term = termRef.current;
    if (!visible || !term) return;
    term.focus();
    props.onSelection?.(term.getSelection());
  }, [visible]);

  // The padding lives on the outer box: the fit addon measures the host's
  // border box, so padding on the host itself makes the terminal too wide.
  return (
    <div className="term" style={{ display: visible ? "flex" : "none" }}>
      <div className="term-host" ref={host} />
    </div>
  );
}
