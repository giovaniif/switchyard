// yard-claude: one Claude Code session on the Agent SDK, bridged to yardd.
// (Or, with `sessions` / `messages <id>`, a one-shot for importing; see oneshot.ts.)
//
// stdin  (JSON lines): start | user | set_model | interrupt
//                      | permission { requestId, decision, answers? }
//                      | mcp_reconnect { server } | mcp_call { ref, server, tool, arguments }
//                      `user` is { text, attachments?, permission? }: files
//                      yardd saved on this host (see message.ts); a new
//                      turn's permission mode (see permission.ts)
// stdout (JSON lines): { type: "event", event } | { type: "usage", usage }
//                      | { type: "session", claudeSessionId }
//                      | { type: "reply", ref, result } | { type: "reply", ref, error }
//
// An MCP tool with a widget (MCP Apps) ends with `tool_completed` carrying
// `app` (see apps.ts); yardd stores the widget and keeps an id in the log.
//
// `start` may carry `mcp`: the connectors an agent's or automation's
// session may use (see scope.ts).
//
// `start` may carry `resume`: a Claude Code session id to continue after a
// yardd restart. yardd learns that id from the `session` line.
//
// Events are already in yardd's normalized shape, so yardd only relays them.
// Nothing else may write to stdout.
import { readOnly, restrictedProfile } from "./restricted";
import { agentToolsProfile, isOwnTool, SERVER } from "./agentTools";
import { scopeProfile, type McpScope } from "./scope";
import { query } from "@anthropic-ai/claude-agent-sdk";
import { createInterface } from "node:readline";
import { userMessage } from "./message";
import { createSubagents } from "./subagents";
import { describe as describeTool, EDIT_TOOLS, patchFor, resultText, short, toolStarted } from "./tools";
import { oneShot } from "./oneshot";
import { ASK_TOOL, answeredInput, questionRequest } from "./questions";
import { createApps } from "./apps";
import { connectors } from "./connectors";
import { mergeUsage, type ClaudeUsage } from "./usage";
import { modeFor, PERMISSION_MODE } from "./permission";

type Decision = "allow" | "allow_always" | "deny";

const out = (msg: unknown) => process.stdout.write(JSON.stringify(msg) + "\n");

// `sessions` and `messages <id>`: answer once and exit (see oneshot.ts).
const mode = process.argv[2];
if (mode === "sessions" || mode === "messages" || mode === "usage") {
  // A long history is megabytes: exit only once the pipe has taken all of
  // it, or yardd reads a cut-off line.
  const done = (msg: unknown, code: number) =>
    process.stdout.write(JSON.stringify(msg) + "\n", () => process.exit(code));
  try {
    done(await oneShot(mode, process.argv.slice(3)), 0);
  } catch (error: any) {
    done({ error: String(error?.message ?? error) }, 1);
  }
  await new Promise(() => {});
}
// `connectors <cwd>`: list and sign in MCP servers, no session (see connectors.ts).
if (mode === "connectors") {
  await connectors(process.argv[3] || process.cwd());
  await new Promise(() => {});
}
const emit = (event: Record<string, unknown>) => out({ type: "event", event });

// Ids must stay unique across sidecar restarts in one session (resume, a
// login), or a new reply would land on an old item of the same id.
const idPrefix = crypto.randomUUID().slice(0, 8);
let counter = 0;
const nextId = (prefix: string) => `${prefix}-${idPrefix}-${++counter}`;

// User turns are pushed here while the query is running (streaming input).
const queue: unknown[] = [];
let wake: (() => void) | null = null;
let closed = false;
async function* input() {
  while (true) {
    while (queue.length) yield queue.shift();
    if (closed) return;
    await new Promise<void>((resolve) => (wake = resolve));
  }
}

/** What the user answered: a decision, and for a question the answers. */
type Answer = { decision: Decision; answers?: unknown };
const waiting = new Map<string, (a: Answer) => void>();
let q: any = null;
let apps: ReturnType<typeof createApps> | null = null;
// Plan usage so far: each rate_limit_event reports one window.
let usage: ClaudeUsage | null = null;
let cwd = process.cwd();

// The mode `start` set, and whether no turn has come yet (see modeFor).
let startMode = "ask";
let firstTurn = true;
// `user` lines are handled one after another: one may wait for a mode switch.
let userLines: Promise<void> = Promise.resolve();

async function run(start: { restricted?: boolean; cwd: string; model: string; effort?: string; permission?: string; resume?: string | null; tools?: Record<string, unknown> | null; mcp?: McpScope }) {
  cwd = start.cwd;
  startMode = start.permission ?? "ask";
  const subagents = createSubagents(cwd);

  // Keep the parent session's markers out of the child Claude Code process.
  const env: Record<string, string> = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (value === undefined) continue;
    if (key === "CLAUDECODE" || key.startsWith("CLAUDE_CODE_") || key.startsWith("CLAUDE_AGENT_SDK")) continue;
    env[key] = value;
  }

  const efforts = ["low", "medium", "high", "xhigh", "max"];
  const options: Record<string, unknown> = {
    cwd,
    model: start.model,
    env,
    includePartialMessages: true,
    permissionMode: PERMISSION_MODE[start.permission ?? "ask"] ?? "default",
    // Lets the session switch into full access later; it grants nothing by
    // itself while the mode is another one.
    allowDangerouslySkipPermissions: true,
    canUseTool: async (toolName: string, toolInput: any, ctx: { signal?: AbortSignal; suggestions?: unknown[]; toolUseID?: string }) => {
      if (readOnly(start)) return { behavior: "deny", message: "This session answers from the evidence yardd supplies, without tools." };
      // Switchyard's own tools (allowedTools covers them; this is the fallback).
      if (start.tools && isOwnTool(toolName)) return { behavior: "allow", updatedInput: toolInput };
      const requestId = nextId("claude");
      const wait = () =>
        new Promise<Answer>((resolve) => {
          waiting.set(requestId, resolve);
          ctx?.signal?.addEventListener("abort", () => resolve({ decision: "deny" }));
        }).finally(() => waiting.delete(requestId));
      // Claude Code asks this even in full access; it needs answers, not a yes.
      if (toolName === ASK_TOOL) {
        emit(questionRequest(requestId, toolInput, ctx?.toolUseID));
        const { decision, answers } = await wait();
        if (decision === "deny") return { behavior: "deny", message: "The user chose not to answer these questions." };
        return { behavior: "allow", updatedInput: answeredInput(toolInput, answers) };
      }
      const d = describeTool(toolName, toolInput, cwd);
      emit({
        type: "approval_request",
        requestId,
        kind: d.kind === "command" ? "command" : EDIT_TOOLS.has(toolName) ? "edit" : "other",
        title: d.kind === "command" ? d.detail : `${d.title} ${d.detail}`.trim(),
        detail: typeof toolInput?.description === "string" ? toolInput.description : "",
      });
      const { decision } = await wait();
      if (decision === "deny") return { behavior: "deny", message: "Denied by the user." };
      if (decision === "allow_always" && ctx?.suggestions?.length) {
        return { behavior: "allow", updatedInput: toolInput, updatedPermissions: ctx.suggestions };
      }
      return { behavior: "allow", updatedInput: toolInput };
    },
  };
  Object.assign(options, agentToolsProfile(start));
  // An agent's or automation's session: only its connectors (see scope.ts).
  const scoped: ReturnType<typeof scopeProfile> = start.restricted ? {} : scopeProfile(start.mcp, start.tools ? SERVER : null);
  if (scoped.managedSettings) options.managedSettings = scoped.managedSettings;
  if (scoped.env) Object.assign(env, scoped.env);
  Object.assign(options, restrictedProfile(start));
  if (start.effort && efforts.includes(start.effort)) options.effort = start.effort;
  if (start.resume) options.resume = start.resume;
  // Recent models omit thinking text unless asked; summarized is what Claude Code shows.
  options.extraArgs = { "thinking-display": "summarized" };
  const claudeBin = Bun.which("claude");
  if (claudeBin) options.pathToClaudeCodeExecutable = claudeBin;

  q = query({ prompt: input() as any, options: options as any });
  apps = createApps(q);

  // The id of the text block currently streaming, so deltas and the final
  // text land on the same item.
  let openText: string | null = null;
  let openThinking: string | null = null;

  for await (const m of q as AsyncIterable<any>) {
    // A subagent's steps nest under its Agent call; the rest of its chatter
    // stays out of the thread.
    if (m.parent_tool_use_id) {
      for (const event of subagents.child(m)) emit(event);
      continue;
    }

    if (m.type === "stream_event") {
      const e = m.event;
      if (e?.type === "content_block_start" && e.content_block?.type === "text") openText = nextId("text");
      if (e?.type === "content_block_start" && e.content_block?.type === "thinking") openThinking = nextId("think");
      if (e?.type === "content_block_delta" && e.delta?.type === "thinking_delta" && e.delta.thinking) {
        openThinking ??= nextId("think");
        emit({ type: "reasoning_delta", itemId: openThinking, text: e.delta.thinking });
      }
      if (e?.type === "content_block_delta" && e.delta?.type === "text_delta") {
        openText ??= nextId("text");
        emit({ type: "assistant_delta", itemId: openText, text: e.delta.text });
      }
      continue;
    }

    if (m.type === "assistant") {
      for (const block of m.message?.content ?? []) {
        if (block.type === "thinking") {
          if (block.thinking || openThinking) emit({ type: "reasoning", itemId: openThinking ?? nextId("think"), text: block.thinking ?? "" });
          openThinking = null;
        } else if (block.type === "text") {
          emit({ type: "assistant_message", itemId: openText ?? nextId("text"), text: block.text });
          openText = null;
        } else if (block.type === "tool_use") {
          const d = describeTool(block.name, block.input, cwd);
          emit(toolStarted(block, cwd));
          apps?.started(block);
          const patch = patchFor(block.name, block.input);
          if (patch) {
            emit({
              type: "file_change",
              itemId: `${block.id}-diff`,
              files: [{ path: d.detail, diff: patch, kind: block.name === "Write" ? "add" : "update" }],
            });
          }
        }
      }
      continue;
    }

    if (m.type === "user") {
      const content = m.message?.content;
      if (!Array.isArray(content)) continue;
      for (const block of content) {
        if (block?.type !== "tool_result") continue;
        // An Agent call ends with its task's notification, not with this.
        if (subagents.holds(block.tool_use_id)) continue;
        const app = await apps?.completed(block, m.tool_use_result).catch(() => null);
        emit({
          type: "tool_completed",
          itemId: block.tool_use_id,
          ok: !block.is_error,
          output: short(resultText(block.content), 4000),
          ...(app ? { app } : {}),
        });
      }
      continue;
    }

    if (m.type === "system" && (m.subtype === "task_started" || m.subtype === "task_notification")) {
      for (const event of subagents.task(m)) emit(event);
      continue;
    }

    if (m.type === "system" && m.subtype === "init" && typeof m.session_id === "string") {
      out({ type: "session", claudeSessionId: m.session_id });
      continue;
    }

    if (m.type === "rate_limit_event") {
      usage = mergeUsage(usage, m.rate_limit_info);
      out({ type: "usage", usage });
      continue;
    }

    if (m.type === "result") {
      openText = null;
      openThinking = null;
      const ok = m.subtype === "success";
      if (!ok) emit({ type: "error", message: typeof m.result === "string" && m.result ? m.result : String(m.subtype) });
      emit({ type: "turn_completed", status: ok ? "completed" : "failed" });
    }
  }
}

createInterface({ input: process.stdin }).on("line", async (line) => {
  let msg: any;
  try {
    msg = JSON.parse(line);
  } catch {
    return;
  }
  try {
    switch (msg.type) {
      case "start":
        run(msg).then(
          () => process.exit(0),
          (error) => {
            emit({ type: "error", message: String(error?.message ?? error) });
            emit({ type: "turn_completed", status: "failed" });
            process.exit(1);
          },
        );
        break;
      case "user":
        userLines = userLines.then(async () => {
          // A new turn names its mode (a message joining a running turn does not).
          const switchTo = modeFor(msg.permission, startMode, firstTurn);
          if (typeof msg.permission === "string" && msg.permission) firstTurn = false;
          if (switchTo) {
            try {
              await q?.setPermissionMode(switchTo as any);
            } catch (error: any) {
              // A read-only turn never runs in a mode that can act.
              if (switchTo === "plan") {
                emit({ type: "error", message: `Could not switch to Plan for this read-only turn: ${String(error?.message ?? error)}` });
                emit({ type: "turn_completed", status: "failed" });
                return;
              }
            }
          }
          queue.push(userMessage(String(msg.text ?? ""), Array.isArray(msg.attachments) ? msg.attachments : []));
          wake?.();
        });
        break;
      case "set_model":
        await q?.setModel(msg.model);
        break;
      case "interrupt":
        await q?.interrupt();
        break;
      case "set_permission":
        await q?.setPermissionMode((PERMISSION_MODE[msg.permission] ?? "default") as any);
        break;
      case "permission":
        waiting.get(msg.requestId)?.({ decision: msg.decision as Decision, answers: msg.answers });
        break;
      // A server was signed in elsewhere (the Connectors page): dial it again
      // so its tools work in this session without a restart.
      case "mcp_reconnect":
        await q?.reconnectMcpServer(String(msg.server)).catch(() => {});
        break;
      case "mcp_call": {
        const ref = msg.ref;
        if (!apps) {
          out({ type: "reply", ref, error: "The session has not started yet." });
          break;
        }
        apps.call(String(msg.server), String(msg.tool), msg.arguments).then(
          (result) => out({ type: "reply", ref, result }),
          (error) => out({ type: "reply", ref, error: String(error?.message ?? error).slice(0, 1000) }),
        );
        break;
      }
    }
  } catch (error: any) {
    emit({ type: "error", message: String(error?.message ?? error) });
  }
}).on("close", () => {
  closed = true;
  wake?.();
});
