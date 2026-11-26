// Agents link the files they write in many dialects: `/abs/path`, `./rel`,
// `src/a.ts:42`, `~/notes.md`, `file:///…`, ChatGPT's `sandbox:/mnt/data/…`.
// Each one is a file on the session's host, which yardd reads (`file.get`)
// so the page can download it. This file only decides what counts as one;
// yardd resolves it.

/** A scheme that is not a file: `javascript:`, `mailto:`, `vscode:`. A dot
 *  before the colon (`a.ts:4`) or a digit after it (`Makefile:12`) marks a
 *  path with a line number instead. */
const SCHEME = /^[a-z][a-z0-9+-]*:(?!\d)/i;

/** The path to ask yardd for, or null when the destination is not a local file. */
export function fileTarget(destination: string): string | null {
  const dest = destination.trim().replace(/^<(.*)>$/, "$1").trim();
  if (!dest || dest.length > 4096 || dest.startsWith("#") || dest.startsWith("?") || dest.startsWith("//")) return null;
  if (/^file:/i.test(dest)) return /^file:\/\/(localhost)?\//i.test(dest) ? dest : null;
  if (/^sandbox:\/./i.test(dest)) return dest;
  if (SCHEME.test(dest)) return null;
  // A Windows drive (`C:\x`) or a control character is not a path on these hosts.
  if (/^[a-z]:\\/i.test(dest) || /[\u0000-\u001f]/.test(dest)) return null;
  return dest;
}

/** The text a download error shows next to the link. */
export function fileError(reply: { op?: string; error?: string; message?: string }): string {
  const text = reply.error ?? reply.message ?? "The file could not be read.";
  // A host on an older yardd does not know the op.
  if (/unknown op `file\.get`/.test(text)) return "That host runs an older Switchyard. Update it to download files.";
  return text.charAt(0).toUpperCase() + text.slice(1);
}
