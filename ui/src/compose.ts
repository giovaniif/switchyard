import type { Attachment } from "./protocol";

// Lets other panes add to a session's reply box without lifting the
// composer's state: the composer listens for its own session id.

/** Terminal rows attached as context: `label` names where they came from. */
export interface Snippet {
  label: string;
  text: string;
}

/** Part of an answer replied to, and the user's comment on that part, if any. */
export interface Quote {
  text: string;
  note?: string;
  /** Where it was selected, to mark it in the answer while it waits (not sent). */
  range?: Range;
}

/** Text goes into the box; quotes and snippets become cards above it. */
export interface Addition {
  text?: string;
  /** Files already on the session's host, back as chips. */
  attachments?: Attachment[];
  quotes?: Quote[];
  snippets?: Snippet[];
  /** The new quote card takes the focus, for its comment. */
  focusQuote?: boolean;
}

type Listener = (added: Addition) => void;

const listeners = new Map<string, Set<Listener>>();

export function onAppend(sessionId: string, listener: Listener): () => void {
  let set = listeners.get(sessionId);
  if (!set) listeners.set(sessionId, (set = new Set()));
  set.add(listener);
  return () => {
    set.delete(listener);
    if (set.size === 0) listeners.delete(sessionId);
  };
}

function add(sessionId: string, added: Addition) {
  listeners.get(sessionId)?.forEach((listener) => listener(added));
}

/** Text for the box, as is (a terminal selection). */
export function appendToComposer(sessionId: string, text: string) {
  add(sessionId, { text });
}

/** Terminal rows, as a card above the reply box. Cut to the cap. */
export function snippetInComposer(sessionId: string, label: string, text: string) {
  const body = text.slice(0, SELECTION_CAP).replace(/\n+$/, "");
  if (body.trim()) add(sessionId, { snippets: [{ label, text: body }] });
}

/** Part of an agent's answer, as a quote card for a reply, its comment
 *  field focused. `range` is the selection, marked in the answer meanwhile. */
export function quoteInComposer(sessionId: string, selection: string, range?: Range) {
  const quote = cleanQuote(selection);
  if (quote) add(sessionId, { quotes: [{ text: quote, range }], focusQuote: true });
}

/** A message yardd gave back (Edit, Stop): its leading quotes go back to
 *  cards, so editing a reply does not turn them into `>` text, and its
 *  files go back to chips. */
export function returnToComposer(sessionId: string, message: string, attachments: Attachment[] = []) {
  const { quotes, text } = splitQuotes(message);
  add(sessionId, { ...(quotes.length ? { quotes, text } : { text }), ...(attachments.length ? { attachments } : {}) });
}

export const SELECTION_CAP = 20_000;

/** A selection as a quote: cut to the cap, outer blank lines and trailing
 *  space trimmed, runs of blank lines folded to one. Empty if nothing. */
export function cleanQuote(selection: string): string {
  const body = selection
    .slice(0, SELECTION_CAP)
    .replace(/\r\n?/g, "\n")
    .replace(/^\s*\n|\s+$/g, "")
    .replace(/\n{3,}/g, "\n\n");
  return body.trim() ? body : "";
}

function blockquote(quote: string): string {
  return quote
    .split("\n")
    .map((line) => (line ? `> ${line}` : ">"))
    .join("\n");
}

/** Heads a reply whose quotes carry comments: each passage and its comment
 *  are one numbered item, so the agent can tell which comment is about
 *  which part (several quotes and one message ran together before). */
export const COMMENTS_HEADER = "Comments on parts of your answers:";
const INDENT = "   ";

/** One numbered comment: `1. > passage`, the passage's other lines and the
 *  comment indented under it (a Markdown list item holding a blockquote). */
function commentItem(q: Quote, n: number): string {
  const quote = blockquote(q.text).split("\n");
  // A comment that starts with `>` would read as more of the quote: escaped.
  const note = (q.note ?? "").trim().replace(/^>/, "\\>");
  const lines = [`${n}. ${quote[0]}`, ...quote.slice(1).map((l) => INDENT + l), ...(note ? note.split("\n").map((l) => (l ? INDENT + l : "")) : [`${INDENT}(no comment on this part)`])];
  return lines.join("\n");
}

/** What a reply sends. Quotes without comments: each as a blockquote, then
 *  the text, as before. Once any quote has a comment: the header, then one
 *  numbered item per quote with its comment under it, then the text. Plain
 *  Markdown either way, so every engine reads it. */
export function withQuotes(quotes: (string | Quote)[], text: string, snippets: Snippet[] = []): string {
  const list = quotes.map((q) => (typeof q === "string" ? { text: q } : q));
  const commented = list.some((q) => q.note?.trim());
  const head = commented ? [`${COMMENTS_HEADER}\n\n${list.map((q, i) => commentItem(q, i + 1)).join("\n\n")}`] : list.map((q) => blockquote(q.text));
  return [...head, ...snippets.map(snippetBlock), ...(text ? [text] : [])].join("\n\n");
}

/** The inverse of the commented form: its items and the rest, or null when
 *  the message does not start with the header. */
export function splitComments(message: string): { quotes: Quote[]; text: string } | null {
  const lines = message.split("\n");
  if (lines[0] !== COMMENTS_HEADER) return null;
  const quotes: Quote[] = [];
  let i = 1;
  const skipBlank = () => {
    while (i < lines.length && lines[i] === "") i++;
  };
  skipBlank();
  for (;;) {
    const m = /^\d+\. >(?: (.*))?$/.exec(lines[i] ?? "");
    if (!m) break;
    const quote = [m[1] ?? ""];
    const note: string[] = [];
    i++;
    while (i < lines.length) {
      const line = lines[i];
      if (line === "") {
        // A blank line stays inside the item when more of it follows.
        let j = i;
        while (j < lines.length && lines[j] === "") j++;
        if (j >= lines.length || !lines[j].startsWith(INDENT)) break;
        (note.length ? note : quote).push(...lines.slice(i, j).map(() => ""));
        i = j;
        continue;
      }
      if (!line.startsWith(INDENT)) break;
      const body = line.slice(INDENT.length);
      if (!note.length && QUOTE_LINE.test(body)) quote.push(unquote(body));
      else note.push(body);
      i++;
    }
    const comment = note.join("\n").trim().replace(/^\\>/, ">");
    quotes.push({ text: quote.join("\n"), ...(comment && comment !== "(no comment on this part)" ? { note: comment } : {}) });
    skipBlank();
  }
  if (!quotes.length) return null;
  return { quotes, text: lines.slice(i).join("\n") };
}

export const lineCount = (text: string) => text.split("\n").length;

/** A snippet as a labelled fenced block. The fence is longer than any run of
 *  backticks in the output, so terminal text cannot close it early. */
export function snippetBlock(s: Snippet): string {
  const longest = Math.max(0, ...(s.text.match(/`+/g) ?? []).map((run) => run.length));
  const fence = "`".repeat(Math.max(3, longest + 1));
  const lines = lineCount(s.text);
  return `${s.label}, ${lines === 1 ? "1 line" : `${lines} lines`}:\n${fence}text\n${s.text}\n${fence}`;
}

const QUOTE_LINE = /^>/;
const unquote = (line: string) => line.replace(/^> ?/, "");

/** The inverse of `withQuotes`: the quotes a message starts with (with
 *  their comments, in the commented form), and the rest. A plain quote must
 *  end at a blank line; one that runs straight into text is left as text. */
export function splitQuotes(message: string): { quotes: Quote[]; text: string } {
  const commented = splitComments(message);
  if (commented) return commented;
  const lines = message.split("\n");
  const quotes: Quote[] = [];
  let i = 0;
  while (i < lines.length && QUOTE_LINE.test(lines[i])) {
    let j = i;
    while (j < lines.length && QUOTE_LINE.test(lines[j])) j++;
    if (j < lines.length && lines[j] !== "") break;
    const quote = lines.slice(i, j).map(unquote).join("\n");
    if (quote.trim()) quotes.push({ text: quote });
    i = j + 1;
  }
  if (quotes.length === 0) return { quotes: [], text: message };
  return { quotes, text: lines.slice(i).join("\n") };
}

export interface QuoteSegment {
  quote: boolean;
  text: string;
}

/** A sent message in runs of quoted lines and plain text, for the bubble.
 *  Lines in a fenced block are never quotes. The blank line around a quote
 *  is spacing, not text. */
export function quoteSegments(message: string): QuoteSegment[] {
  const out: QuoteSegment[] = [];
  let fenced = false;
  let run: string[] = [];
  let quoting = false;
  const flush = () => {
    const text = quoting ? run.join("\n") : run.join("\n").replace(/^\n+|\n+$/g, "");
    if (quoting || text) out.push({ quote: quoting, text });
    run = [];
  };
  for (const line of message.split("\n")) {
    if (line.startsWith("```")) fenced = !fenced;
    const quote = !fenced && QUOTE_LINE.test(line);
    if (quote !== quoting && run.length) flush();
    quoting = quote;
    run.push(quote ? unquote(line) : line);
  }
  if (run.length) flush();
  if (!out.some((s) => s.quote)) return [{ quote: false, text: message }];
  return out;
}

export interface FenceSegment {
  code: boolean;
  text: string;
}

/** A sent message in runs of text and closed fenced blocks, for the bubble
 *  (terminal output attached to a reply). A fence closes only on a line of
 *  at least as many backticks; an unclosed one stays text. */
export function fenceSegments(message: string): FenceSegment[] {
  const lines = message.split("\n");
  const out: FenceSegment[] = [];
  let text: string[] = [];
  const flushText = () => {
    const t = text.join("\n").replace(/^\n+|\n+$/g, "");
    if (t) out.push({ code: false, text: t });
    text = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const open = /^(`{3,})[^`]*$/.exec(lines[i]);
    const end = open ? lines.findIndex((l, j) => j > i && new RegExp(`^\`{${open[1].length},}\\s*$`).test(l)) : -1;
    if (!open || end < 0) {
      text.push(lines[i]);
      continue;
    }
    flushText();
    out.push({ code: true, text: lines.slice(i + 1, end).join("\n") });
    i = end;
  }
  flushText();
  if (!out.some((s) => s.code)) return [{ code: false, text: message }];
  return out;
}

// A session's reply box as it was when the user looked away. The session
// view remounts on every switch, so without this a half-written reply, its
// quotes and its files would be dropped. The New session box keeps its
// half-typed task the same way, under a fixed key. Memory only: a reload clears it.
const drafts = new Map<string, unknown>();

export function keepDraft<D>(sessionId: string, draft: D | null) {
  if (draft) drafts.set(sessionId, draft);
  else drafts.delete(sessionId);
}

export function draftOf<D>(sessionId: string | undefined): D | undefined {
  return sessionId === undefined ? undefined : (drafts.get(sessionId) as D | undefined);
}
