// Labels and patches for Claude Code tool calls, shared by a live session
// (main.ts) and an imported one (history.ts).
import { relative } from "node:path";
import { ownToolLabel } from "./agentTools";

export const EDIT_TOOLS = new Set(["Edit", "Write", "MultiEdit", "NotebookEdit"]);
export const short = (s: unknown, n = 160) => {
  const text = typeof s === "string" ? s : JSON.stringify(s ?? "");
  return text.length > n ? text.slice(0, n) + "…" : text;
};
const rel = (cwd: string, p: unknown) => {
  if (typeof p !== "string") return "";
  const r = relative(cwd, p);
  return r && !r.startsWith("..") ? r : p;
};

/** A short human label for a tool call; paths are shown relative to `cwd`. */
export function describe(name: string, toolInput: any, cwd: string): { kind: string; title: string; detail: string } {
  switch (name) {
    case "Bash":
      return { kind: "command", title: "Ran", detail: String(toolInput?.command ?? "") };
    case "Read":
      return { kind: "read", title: "Read", detail: rel(cwd, toolInput?.file_path) };
    case "Grep":
    case "Glob":
      return { kind: "search", title: "Searched", detail: String(toolInput?.pattern ?? "") };
    case "Edit":
    case "MultiEdit":
    case "NotebookEdit":
      return { kind: "edit", title: "Edited", detail: rel(cwd, toolInput?.file_path ?? toolInput?.notebook_path) };
    case "Write":
      return { kind: "edit", title: "Wrote", detail: rel(cwd, toolInput?.file_path) };
    case "WebFetch":
      return { kind: "other", title: "Fetched", detail: String(toolInput?.url ?? "") };
    case "WebSearch":
      return { kind: "other", title: "Searched the web", detail: String(toolInput?.query ?? "") };
    case "Task":
    case "Agent":
      return { kind: "other", title: "Agent", detail: String(toolInput?.description ?? "") };
    case "TodoWrite":
      return { kind: "other", title: "Updated tasks", detail: "" };
    default: {
      const own = ownToolLabel(name, toolInput);
      if (own) return { kind: "other", ...own };
      // An MCP tool: `mcp__<server>__<tool>` reads better as the tool, then its server.
      const mcp = /^mcp__(.+?)__(.+)$/.exec(name);
      if (mcp) return { kind: "other", title: mcp[2], detail: `${mcp[1]} · ${short(toolInput, 100)}` };
      return { kind: "other", title: name, detail: short(toolInput, 120) };
    }
  }
}

/** The SDK gives old and new text, not a patch. Build a readable one. */
export function patchFor(name: string, toolInput: any): string | null {
  const lines = (s: unknown, sign: string) =>
    String(s ?? "")
      .split("\n")
      .slice(0, 200)
      .map((l) => sign + l)
      .join("\n");
  if (name === "Edit") return replacePatch(toolInput?.old_string, toolInput?.new_string);
  if (name === "Write") return lines(toolInput?.content, "+");
  if (name === "MultiEdit" && Array.isArray(toolInput?.edits)) {
    return toolInput.edits.map((e: any) => replacePatch(e?.old_string, e?.new_string)).join("\n");
  }
  return null;
}

/** Old text -> new text, with the unchanged lines at both ends kept as context. */
export function replacePatch(oldText: unknown, newText: unknown): string {
  const a = String(oldText ?? "").split("\n");
  const b = String(newText ?? "").split("\n");
  let start = 0;
  while (start < a.length && start < b.length && a[start] === b[start]) start++;
  let endA = a.length;
  let endB = b.length;
  while (endA > start && endB > start && a[endA - 1] === b[endB - 1]) {
    endA--;
    endB--;
  }
  return [
    ...a.slice(Math.max(0, start - 2), start).map((l) => " " + l),
    ...a.slice(start, endA).map((l) => "-" + l),
    ...b.slice(start, endB).map((l) => "+" + l),
    ...a.slice(endA, endA + 2).map((l) => " " + l),
  ]
    .slice(0, 200)
    .join("\n");
}

export function resultText(content: unknown): string {
  if (typeof content === "string") return content;
  if (Array.isArray(content)) {
    return content
      .map((b: any) => (b?.type === "text" ? b.text : ""))
      .filter(Boolean)
      .join("\n");
  }
  return "";
}

/** The `tool_started` event for a `tool_use` block. An Agent call names its
 *  subagent's type (`agent`); a subagent's own call names the Agent call it
 *  runs under (`parent`). */
export function toolStarted(block: { id: string; name: string; input?: any }, cwd: string, parent?: string) {
  const d = describe(block.name, block.input, cwd);
  const event: Record<string, unknown> = { type: "tool_started", itemId: block.id, kind: d.kind, title: d.title, detail: d.detail };
  if (d.title === "Agent" && typeof block.input?.subagent_type === "string") event.agent = block.input.subagent_type;
  if (parent) event.parent = parent;
  return event;
}
