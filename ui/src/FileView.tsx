import { createContext, useEffect, useMemo, useRef, useState } from "react";
import { FILE, Icon, IMPORT, Markdown, REFRESH } from "./App";
import { humanSize } from "./attach";
import {
  decodeText,
  delimiterFor,
  describe,
  imageMime,
  linkedLine,
  parseDelimited,
  PREVIEW_MAX_BYTES,
  previewKind,
  relativeTo,
  saveFile,
  shortPath,
  tableView,
  TEXT_LINE_LIMIT,
  type PreviewKind,
} from "./filePreview";
import type { Actions, FileRead } from "./store";

/** Opens a file a link in the conversation names. Resolves with an error to
 *  show beside the link when it could not (no workspace: it downloads). Null
 *  outside a session, where links stay code. */
export const FileLinks = createContext<((path: string) => Promise<string | null>) | null>(null);

/** One file in the workspace: a preview for Markdown, CSV/TSV, images and
 *  other text, and a Download button for anything. */
export function FileView({ sessionId, path, nonce, home, actions, onOpen }: {
  sessionId: string;
  /** As the link wrote it; yardd resolves it on the session's host. */
  path: string;
  /** Bumped when the same link is clicked again, which reads it again. */
  nonce: number;
  home: string;
  actions: Actions;
  onOpen(path: string): Promise<string | null>;
}) {
  const [read, setRead] = useState<FileRead | null>(null);
  const [loading, setLoading] = useState(true);
  const [again, setAgain] = useState(0);
  useEffect(() => {
    let live = true;
    setLoading(true);
    actions.readFile(sessionId, path).then((result) => {
      if (!live) return;
      setRead(result);
      setLoading(false);
    });
    return () => {
      live = false;
    };
  }, [sessionId, path, nonce, again]);
  // A different file starts blank instead of showing the last one's content.
  useEffect(() => setRead(null), [sessionId, path]);

  const file = read && !("error" in read) ? read : null;
  const kind: PreviewKind = file ? previewKind(file.name, file.data) : "none";
  const name = file?.name ?? fileLabel(path);
  const reload = () => setAgain((n) => n + 1);

  return (
    <section className="pane-body file-view" aria-label={`File ${name}`} aria-busy={loading || undefined}>
      <header className="file-head">
        <span className="file-badge" aria-hidden="true">
          <Icon d={FILE} size={16} />
        </span>
        <h2 className="file-name" title={name}>
          {name}
        </h2>
        <button className="btn-icon file-reload" aria-label="Reload file" title="Reload file" disabled={loading} onClick={reload}>
          <Icon d={REFRESH} />
        </button>
        <button className="btn file-download" disabled={!file} onClick={() => file && saveFile(file.name, file.data)}>
          <Icon d={IMPORT} size={14} />
          Download
        </button>
        {file && <FileMeta file={file} kind={kind} />}
        {file && (
          <p className="file-path" title={file.path}>
            {shortPath(file.path, home)}
          </p>
        )}
      </header>
      {read && "error" in read ? (
        <div className="file-empty" role="alert">
          <strong>Couldn't open this file</strong>
          <p>{read.error}</p>
          <button className="btn" onClick={reload}>
            Try again
          </button>
        </div>
      ) : !file ? (
        <p className="file-loading muted shimmer">Opening {name}…</p>
      ) : (
        <FileBody file={file} kind={kind} line={linkedLine(path)} onOpen={(link) => onOpen(relativeTo(file.path, link))} />
      )}
    </section>
  );
}

/** The tab's name for a link before yardd answers: its last part, without a position. */
export function fileLabel(path: string): string {
  const last = path.replace(/^(?:file:\/\/(?:localhost)?|sandbox:)/i, "").replace(/\/+$/, "").split("/").pop() ?? "";
  return last.replace(/#L\d.*$/, "").replace(/(?::\d+(?:-\d+)?){1,2}$/, "") || "File";
}

function FileMeta({ file, kind }: { file: { name: string; data: Uint8Array }; kind: PreviewKind }) {
  const detail = useMemo(() => {
    const text = kind === "table" || kind === "text" ? decodeText(file.data) ?? "" : "";
    if (kind === "table") {
      const rows = parseDelimited(text, delimiterFor(file.name, text));
      const view = tableView(rows);
      return { rows: view.total, columns: view.columns };
    }
    if (kind === "text") return { lines: text.split("\n").length - (text.endsWith("\n") ? 1 : 0) };
    return {};
  }, [file, kind]);
  return (
    <p className="file-meta">
      {describe(file.name, kind, detail)} · {humanSize(file.data.length)}
    </p>
  );
}

function FileBody({ file, kind, line, onOpen }: { file: { name: string; data: Uint8Array }; kind: PreviewKind; line: number | null; onOpen(path: string): Promise<string | null> }) {
  const text = useMemo(() => (kind === "markdown" || kind === "table" || kind === "text" ? decodeText(file.data) ?? "" : ""), [file, kind]);
  if (kind === "markdown") {
    return (
      <div className="file-doc">
        {/* Links inside the document resolve from its own folder. */}
        <FileLinks.Provider value={onOpen}>
          <Markdown text={text} />
        </FileLinks.Provider>
      </div>
    );
  }
  if (kind === "table") return <FileTable name={file.name} text={text} />;
  if (kind === "image") return <FileImage name={file.name} data={file.data} />;
  if (kind === "text") return <FileText text={text} line={line} />;
  const big = file.data.length > PREVIEW_MAX_BYTES;
  return (
    <div className="file-empty">
      <strong>{big ? "Too large to preview" : "No preview for this file"}</strong>
      <p>{big ? `Previews stop at ${humanSize(PREVIEW_MAX_BYTES)}. Download it to open it on your computer.` : "Download it to open it on your computer."}</p>
    </div>
  );
}

function FileTable({ name, text }: { name: string; text: string }) {
  const view = useMemo(() => tableView(parseDelimited(text, delimiterFor(name, text))), [name, text]);
  if (!view.columns) return <p className="file-empty-line muted">This file is empty.</p>;
  const cols = Array.from({ length: view.columns }, (_, k) => k);
  const cls = (k: number) => (view.numeric[k] ? "num" : undefined);
  return (
    <>
      <div className="file-table" role="region" aria-label={`${name} table`} tabIndex={0}>
        <table>
          <thead>
            <tr>
              <th className="row-num" aria-label="Row" />
              {cols.map((k) => (
                <th key={k} className={cls(k)} title={view.header[k]}>
                  {view.header[k] ?? ""}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {view.rows.map((row, r) => (
              <tr key={r}>
                <td className="row-num">{r + 1}</td>
                {cols.map((k) => (
                  <td key={k} className={cls(k)} title={row[k] && row[k].length > 40 ? row[k] : undefined}>
                    {row[k] ?? ""}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {view.total > view.rows.length && (
        <p className="file-note">
          Showing the first {view.rows.length.toLocaleString("en-US")} of {view.total.toLocaleString("en-US")} rows. Download the file for the rest.
        </p>
      )}
    </>
  );
}

function FileImage({ name, data }: { name: string; data: Uint8Array }) {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    const next = URL.createObjectURL(new Blob([data as BlobPart], { type: imageMime(name) ?? "application/octet-stream" }));
    setUrl(next);
    return () => URL.revokeObjectURL(next);
  }, [name, data]);
  return <div className="file-image">{url && <img src={url} alt={name} />}</div>;
}

/** Numbered lines; the line a link named (`a.ts:42`) is marked and scrolled to. */
function FileText({ text, line }: { text: string; line: number | null }) {
  const lines = useMemo(() => {
    const all = text.split("\n");
    if (all.length > 1 && all[all.length - 1] === "") all.pop();
    return all;
  }, [text]);
  const shown = lines.slice(0, TEXT_LINE_LIMIT);
  const hit = useRef<HTMLSpanElement>(null);
  useEffect(() => hit.current?.scrollIntoView({ block: "center" }), [text, line]);
  return (
    <>
      <pre className="file-text">
        <code>
          {shown.map((content, i) => (
            <span key={i} ref={i + 1 === line ? hit : undefined} className={i + 1 === line ? "file-line file-line-hit" : "file-line"}>
              {content}
              {"\n"}
            </span>
          ))}
        </code>
      </pre>
      {lines.length > shown.length && (
        <p className="file-note">
          Showing the first {shown.length.toLocaleString("en-US")} of {lines.length.toLocaleString("en-US")} lines. Download the file for the rest.
        </p>
      )}
    </>
  );
}
