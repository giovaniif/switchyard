// What the workspace's file view shows for a file an agent linked: the parts
// that need no DOM, so they can be tested. The view is `FileView.tsx`.

export type PreviewKind = "markdown" | "table" | "image" | "text" | "none";

/** Past this, text is not rendered: the Markdown renderer and a DOM table
 *  of that size would freeze the page. The file still downloads. */
export const PREVIEW_MAX_BYTES = 2 * 1024 * 1024;
/** Rows a table shows; the summary still counts them all. */
export const TABLE_ROW_LIMIT = 1000;
/** Lines a text file shows. */
export const TEXT_LINE_LIMIT = 5000;

const IMAGES: Record<string, string> = { png: "image/png", jpg: "image/jpeg", jpeg: "image/jpeg", gif: "image/gif", webp: "image/webp" };

export const extension = (name: string) => {
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
};

/** The image type to show a file as. SVG is left out: it is a document that
 *  can carry script, and it is not worth the care to show one. */
export const imageMime = (name: string): string | null => IMAGES[extension(name)] ?? null;

/** UTF-8 text, or null for anything binary (a NUL early on, or bytes that are not UTF-8). */
export function decodeText(bytes: Uint8Array): string | null {
  if (bytes.subarray(0, 8000).includes(0)) return null;
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(bytes).replace(/^﻿/, "");
  } catch {
    return null;
  }
}

export function previewKind(name: string, bytes: Uint8Array): PreviewKind {
  const ext = extension(name);
  if (imageMime(name)) return "image";
  if (bytes.length > PREVIEW_MAX_BYTES || decodeText(bytes) === null) return "none";
  if (ext === "md" || ext === "markdown" || ext === "mdx") return "markdown";
  if (ext === "csv" || ext === "tsv") return "table";
  return "text";
}

/** `,` or `;` (European spreadsheets) for a .csv, by which the first line has more of; a tab for .tsv. */
export function delimiterFor(name: string, text: string): string {
  if (extension(name) === "tsv") return "\t";
  const first = text.slice(0, text.search(/\r?\n|$/));
  const count = (c: string) => first.split(c).length - 1;
  return count(";") > count(",") ? ";" : ",";
}

/** RFC 4180: quoted fields may hold the delimiter, `""` and line breaks. */
export function parseDelimited(text: string, delimiter: string): string[][] {
  const rows: string[][] = [];
  let row: string[] = [];
  let field = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"' && text[i + 1] === '"') {
        field += '"';
        i++;
      } else if (c === '"') quoted = false;
      else field += c;
    } else if (c === '"' && field === "") quoted = true;
    else if (c === delimiter) {
      row.push(field);
      field = "";
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(field);
      rows.push(row);
      row = [];
      field = "";
    } else field += c;
  }
  if (field !== "" || row.length) {
    row.push(field);
    rows.push(row);
  }
  return rows;
}

const NUMBER = /^[-+]?(?:\d{1,3}(?:,\d{3})+|\d+)(?:\.\d+)?%?$|^[-+]?\.\d+%?$/;

export interface TableView {
  header: string[];
  rows: string[][];
  /** Data rows in the file, past the ones shown. */
  total: number;
  columns: number;
  /** Columns whose every filled cell is a number, aligned right like a spreadsheet. */
  numeric: boolean[];
}

export function tableView(all: string[][], limit = TABLE_ROW_LIMIT): TableView {
  const header = all[0] ?? [];
  const body = all.slice(1);
  const columns = Math.max(header.length, ...body.slice(0, limit).map((r) => r.length), 0);
  const rows = body.slice(0, limit);
  const numeric = Array.from({ length: columns }, (_, k) => {
    const filled = rows.map((r) => (r[k] ?? "").trim()).filter(Boolean);
    return filled.length > 0 && filled.every((v) => NUMBER.test(v));
  });
  return { header, rows, total: body.length, columns, numeric };
}

/** A link inside a Markdown file is relative to that file, not to the session's folder. */
export function relativeTo(file: string, link: string): string {
  if (/^(?:\/|~\/|~$|[a-z][a-z0-9+-]*:(?!\d))/i.test(link)) return link;
  const parts = file.split("/").slice(0, -1);
  for (const part of link.split("/")) {
    if (part === "..") {
      if (parts.length > 1) parts.pop();
    } else if (part !== "." && part !== "") parts.push(part);
  }
  return parts.join("/") || "/";
}

/** The line a link points at: `a.ts:42`, `a.ts:42:7`, `a.ts#L42-L50`. */
export function linkedLine(link: string): number | null {
  const m = link.match(/#L(\d+)(?:C\d+)?(?:-L?\d+)?$/) ?? link.match(/:(\d+)(?::\d+|-\d+)?$/);
  const n = m ? Number(m[1]) : 0;
  return n > 0 ? n : null;
}

/** `~/…` for paths under the host's home, which is most of them. */
export function shortPath(path: string, home: string): string {
  const base = home.replace(/\/+$/, "");
  return base && (path === base || path.startsWith(`${base}/`)) ? `~${path.slice(base.length)}` : path;
}

const count = (n: number, one: string, many: string) => `${n.toLocaleString("en-US")} ${n === 1 ? one : many}`;

/** The line under the file's name: "CSV · 92 rows · 4 columns". */
export function describe(name: string, kind: PreviewKind, detail: { rows?: number; columns?: number; lines?: number }): string {
  const ext = extension(name).toUpperCase();
  const parts = [kind === "markdown" ? "Markdown" : ext || "File"];
  if (kind === "table" && detail.rows !== undefined) parts.push(count(detail.rows, "row", "rows"), count(detail.columns ?? 0, "column", "columns"));
  if (kind === "text" && detail.lines !== undefined) parts.push(count(detail.lines, "line", "lines"));
  return parts.join(" · ");
}

/** Save bytes as a file: a `download` link to a blob that is never opened,
 *  so an agent's HTML cannot run on yardd's origin. */
export function saveFile(name: string, data: Uint8Array) {
  const url = URL.createObjectURL(new Blob([data as BlobPart], { type: "application/octet-stream" }));
  const a = document.createElement("a");
  a.href = url;
  a.download = name || "file";
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
