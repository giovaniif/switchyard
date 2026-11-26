// Block parsing for the thread's Markdown. A blank line is not the only
// block boundary: agents put a list right under "Changes:", a heading right
// over its text, and summarise test runs as GFM tables. As source those read
// as "- " lines and pipe soup. Everything stays text; App.tsx renders it.

export type FencedBlock =
  | { kind: "text"; text: string }
  | { kind: "code"; language: string; body: string; complete: boolean };

/** Line-based fences keep inline backticks and nested Markdown examples intact.
 *  An open fence stays code while the agent streams its remaining lines. */
export function fencedBlocks(text: string): FencedBlock[] {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const out: FencedBlock[] = [];
  let prose: string[] = [];
  const flush = () => {
    if (prose.some((line) => line.length)) out.push({ kind: "text", text: prose.join("\n") });
    prose = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const opening = lines[i].match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
    if (!opening || (opening[1][0] === "`" && opening[2].includes("`"))) {
      prose.push(lines[i]);
      continue;
    }
    // Preserve the newline before a fence as part of the preceding prose.
    if (prose.length) prose.push("");
    flush();
    const body: string[] = [];
    const closing = new RegExp(`^ {0,3}${opening[1][0]}{${opening[1].length},}\\s*$`);
    let complete = false;
    while (++i < lines.length) {
      if (closing.test(lines[i])) {
        complete = true;
        break;
      }
      body.push(lines[i]);
    }
    out.push({ kind: "code", language: opening[2].trim().split(/\s+/)[0].toLowerCase(), body: body.join("\n"), complete });
  }
  flush();
  return out;
}

export type Align = "left" | "center" | "right" | null;

export interface MdTable {
  header: string[];
  align: Align[];
  /** Each as long as the header: short rows are padded, long ones cut. */
  rows: string[][];
}

export type MdSegment =
  | { kind: "text"; lines: string[] }
  /** `level` is the number of `#`s; the thread shows every level alike. */
  | { kind: "heading"; text: string; level: number }
  /** `> ` lines; a document shows them as a quote. */
  | { kind: "quote"; lines: string[] }
  | { kind: "ul"; items: string[] }
  /** `start` when the first item is not 1: steps split by code blocks keep their numbers. */
  | { kind: "ol"; items: string[]; start?: number }
  | { kind: "table"; table: MdTable };

/** A row's cells: outer pipes dropped, split on pipes that are neither
 *  escaped (`\|`) nor inside inline code. */
export function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const cells: string[] = [];
  let cell = "";
  let inCode = false;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "\\" && s[i + 1] === "|") {
      cell += "|";
      i++;
    } else if (c === "`") {
      inCode = !inCode;
      cell += c;
    } else if (c === "|" && !inCode) {
      cells.push(cell.trim());
      cell = "";
    } else cell += c;
  }
  cells.push(cell.trim());
  return cells;
}

const DELIMITER_CELL = /^:?-+:?$/;

function alignments(line: string): Align[] | null {
  if (!line.includes("-")) return null;
  const cells = splitRow(line);
  if (!cells.every((c) => DELIMITER_CELL.test(c))) return null;
  return cells.map((c) => (c.startsWith(":") && c.endsWith(":") ? "center" : c.endsWith(":") ? "right" : c.startsWith(":") ? "left" : null));
}

// Closing hashes need a space before them, as in GFM: "## Use C#" keeps its "#".
const HEADING = /^(#{1,6}) +(.+?)(?:\s+#+)?\s*$/;
const QUOTE = /^ {0,3}> ?(.*)$/;
const BULLET = /^\s*[-*+] +(.*)$/;
const NUMBERED = /^\s*(\d{1,9})[.)] +(.*)$/;

/** A paragraph's lines split into blocks. Headings, lists and tables start
 *  on any line, as in GFM; a numbered list that interrupts text must start
 *  at 1, so a sentence that begins "2024. " stays text. An indented line
 *  continues the list item above it. A table is a header row with a pipe,
 *  a delimiter row with as many cells, then every following line that has
 *  a pipe. */
export function blockSegments(lines: string[]): MdSegment[] {
  const out: MdSegment[] = [];
  const last = () => out[out.length - 1];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    const heading = line.match(HEADING);
    if (heading) {
      out.push({ kind: "heading", text: heading[2], level: heading[1].length });
      i++;
      continue;
    }
    const quote = line.match(QUOTE);
    if (quote) {
      const prevQuote = last();
      if (prevQuote?.kind === "quote") prevQuote.lines.push(quote[1]);
      else out.push({ kind: "quote", lines: [quote[1]] });
      i++;
      continue;
    }
    const bullet = line.match(BULLET);
    const numbered = line.match(NUMBERED);
    const prev = last();
    const inList = prev && (prev.kind === "ul" || prev.kind === "ol");
    if (bullet || (numbered && (out.length === 0 || inList || numbered[1] === "1"))) {
      const kind = bullet ? "ul" : "ol";
      const text = bullet ? bullet[1] : numbered![2];
      const start = numbered && !bullet ? Number(numbered[1]) : 1;
      if (prev && prev.kind === kind) prev.items.push(text);
      else if (kind === "ol" && start !== 1) out.push({ kind, items: [text], start });
      else out.push({ kind, items: [text] });
      i++;
      continue;
    }
    if (inList && /^\s{2,}\S/.test(line)) {
      const items = (prev as { items: string[] }).items;
      items[items.length - 1] += ` ${line.trim()}`;
      i++;
      continue;
    }
    const header = line.includes("|") ? splitRow(line) : null;
    const align = header && i + 1 < lines.length ? alignments(lines[i + 1]) : null;
    if (header && align && align.length === header.length) {
      const rows: string[][] = [];
      i += 2;
      while (i < lines.length && lines[i].includes("|")) {
        const cells = splitRow(lines[i]);
        rows.push(header.map((_, k) => cells[k] ?? ""));
        i++;
      }
      out.push({ kind: "table", table: { header, align, rows } });
      continue;
    }
    if (prev?.kind === "text") prev.lines.push(line);
    else out.push({ kind: "text", lines: [line] });
    i++;
  }
  return out;
}
