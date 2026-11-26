import { expect, test } from "bun:test";
import { mkdtempSync, existsSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { scopeProfile } from "./scope";

test("no scope changes nothing", () => {
  expect(scopeProfile(null, null)).toEqual({});
  expect(scopeProfile(undefined, "switchyard")).toEqual({});
  expect(scopeProfile({ except: [] }, null)).toEqual({});
});

test("an `only` list is Claude Code's MCP allowlist, with Switchyard's own server", () => {
  expect(scopeProfile({ only: ["handoff"] }, "switchyard")).toEqual({
    managedSettings: { allowedMcpServers: [{ serverName: "handoff" }, { serverName: "switchyard" }] },
    env: { ENABLE_CLAUDEAI_MCP_SERVERS: "false" },
  });
  // Empty: no server of the account's at all.
  expect(scopeProfile({ only: [] }, null)).toEqual({ managedSettings: { allowedMcpServers: [] }, env: { ENABLE_CLAUDEAI_MCP_SERVERS: "false" } });
});

test("a claude.ai connector on the list keeps connectors fetched; the allowlist still picks", () => {
  expect(scopeProfile({ only: ["claude.ai Linear", "handoff"] }, null)).toEqual({
    managedSettings: { allowedMcpServers: [{ serverName: "claude.ai Linear" }, { serverName: "handoff" }] },
  });
});

test("an `except` list is a denylist that never names Switchyard's server", () => {
  expect(scopeProfile({ except: ["metabase", "claude.ai Metabase", "switchyard"] }, "switchyard")).toEqual({
    managedSettings: { deniedMcpServers: [{ serverName: "metabase" }, { serverName: "claude.ai Metabase" }] },
  });
});

test("names that are not text are dropped", () => {
  expect(scopeProfile({ only: ["a", 3, "", null] as unknown[] }, null).managedSettings).toEqual({ allowedMcpServers: [{ serverName: "a" }] });
});

// Against the real Claude Code CLI, with no prompt (no turn, no usage) and a
// config folder of its own holding two stdio servers. Opt in with
// YARD_TEST_REAL_CLAUDE=1 on a machine with `claude` on PATH.
const real = process.env.YARD_TEST_REAL_CLAUDE === "1" && !!Bun.which("claude");
test.skipIf(!real)("Claude Code starts only the allowed servers, and none it denies", async () => {
  const { query } = await import("@anthropic-ai/claude-agent-sdk");
  const dir = mkdtempSync(join(tmpdir(), "yard-scope-"));
  const chart = join(import.meta.dir, "../../../verify/fakes/mcp-chart");
  const server = (name: string) => ({ type: "stdio", command: chart, env: { MCP_CHART_LOG: join(dir, `${name}.log`) } });
  writeFileSync(join(dir, ".claude.json"), JSON.stringify({ mcpServers: { alpha: server("alpha"), metabase: server("metabase") } }));
  const run = async (scope: Parameters<typeof scopeProfile>[0]) => {
    const env: Record<string, string> = {};
    for (const [k, v] of Object.entries(process.env)) if (v !== undefined && k !== "CLAUDECODE" && !k.startsWith("CLAUDE_CODE_") && !k.startsWith("CLAUDE_AGENT_SDK")) env[k] = v;
    env.CLAUDE_CONFIG_DIR = dir;
    const p = scopeProfile(scope, "switchyard");
    Object.assign(env, p.env ?? {});
    async function* never(): AsyncGenerator<never> {
      await new Promise(() => {});
    }
    const q: any = query({
      prompt: never() as any,
      options: { cwd: dir, env, pathToClaudeCodeExecutable: Bun.which("claude")!, mcpServers: { switchyard: server("switchyard") as any }, ...(p.managedSettings ? { managedSettings: p.managedSettings as any } : {}) },
    });
    await q.initializationResult();
    await Bun.sleep(3000);
    const names = (await q.mcpServerStatus()).map((s: any) => s.name).sort();
    q.close?.();
    return names;
  };
  expect(await run({ only: ["alpha"] })).toEqual(["alpha", "switchyard"]);
  expect(await run({ except: ["metabase"] })).toEqual(["alpha", "switchyard"]);
  // Never even started: its process would have logged its first request.
  expect(existsSync(join(dir, "metabase.log"))).toBe(false);
}, 60_000);
