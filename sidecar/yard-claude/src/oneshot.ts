// One-shot modes. For importing Claude Code sessions, yardd runs
//   yard-claude sessions [--limit N]   -> { sessions: SessionRow[] }, newest first
//   yard-claude messages <sessionId>   -> { session: SessionRow & { model }, events }
// and for an account's plan limits and models, with no turn,
//   yard-claude usage                  -> { usage: ClaudeUsage | null }
//   yard-claude models                 -> { models: PickerModel[] }
// and reads one JSON line. Sessions come from the SDK, never from parsing
// ~/.claude/projects, and use whatever account the CLI is logged in with.
import { getSessionInfo, getSessionMessages, listSessions, query } from "@anthropic-ai/claude-agent-sdk";
import { historyEvents, lastModel, sessionRow } from "./history";
import { fromUsageReport } from "./usage";
import { pickerModels } from "./models";

async function* never(): AsyncGenerator<never> {
  await new Promise(() => {});
}

/** A Claude Code with no prompt, so nothing is sent to a model: what it
 *  answers at start (models) and on request (usage) needs no turn. */
async function withClaude<T>(read: (q: any) => Promise<T>): Promise<T> {
  const env: Record<string, string> = {};
  for (const [key, value] of Object.entries(process.env)) {
    if (value === undefined) continue;
    if (key === "CLAUDECODE" || key.startsWith("CLAUDE_CODE_") || key.startsWith("CLAUDE_AGENT_SDK")) continue;
    env[key] = value;
  }
  const options: Record<string, unknown> = { cwd: process.env.HOME || process.cwd(), env };
  const claudeBin = Bun.which("claude");
  if (claudeBin) options.pathToClaudeCodeExecutable = claudeBin;
  const q: any = query({ prompt: never() as any, options: options as any });
  try {
    await q.initializationResult();
    return await read(q);
  } finally {
    q.close?.();
  }
}

/** The account's limits from Claude Code's usage report. */
const usage = () =>
  withClaude(async (q) => {
    // Experimental in the SDK (0.3.323); a build without it reports no usage.
    if (typeof q.usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET !== "function") return { usage: null };
    const report = await q.usage_EXPERIMENTAL_MAY_CHANGE_DO_NOT_RELY_ON_THIS_API_YET({ skipBehaviors: true });
    return { usage: fromUsageReport(report) };
  });

/** The models the account's plan has, from Claude Code's initialize answer. */
const models = () => withClaude(async (q) => ({ models: pickerModels(await q.supportedModels()) }));

export async function oneShot(mode: string, args: string[]): Promise<unknown> {
  if (mode === "usage") return usage();
  if (mode === "models") return models();
  if (mode === "sessions") {
    const i = args.indexOf("--limit");
    const limit = i >= 0 ? Number(args[i + 1]) : 200;
    // Like Claude Code's own /resume: no SDK or headless sessions (Switchyard's own included).
    const list = await listSessions({ includeProgrammatic: false, limit });
    return { sessions: list.map(sessionRow).sort((a, b) => b.lastModified - a.lastModified) };
  }
  const sessionId = args[0];
  if (!sessionId) throw new Error("messages needs a session id");
  const info = await getSessionInfo(sessionId);
  if (!info) throw new Error(`no Claude Code session ${sessionId}`);
  const row = sessionRow(info);
  const messages = await getSessionMessages(sessionId);
  return { session: { ...row, model: lastModel(messages) }, events: historyEvents(messages, row.cwd ?? process.cwd()) };
}
