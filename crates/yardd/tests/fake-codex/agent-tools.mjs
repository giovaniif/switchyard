// Switchyard's agent tools from the fake engines' side: what a real engine
// does with the MCP server yardd hands an agent's conversation (Codex's
// `mcp_servers.switchyard` override, Claude's `mcpServers`, ACP's
// `mcpServers`). A prompt calls one with a marker:
//
//   [[tool message_agent {"to":"Reviewer","text":"look at #12"}]]
//
// The arguments are JSON and may hold markers of their own (they are only
// text until another agent's engine reads them). Each marker becomes a real
// MCP call: the server is started as the engine was told to start it
// (`yardd agent-tools` with its key), and spoken to over stdio.
// FAKE_CODEX_DIR gets `agent-tools.jsonl`: every call with its result, and
// the tool names `tools/list` offered.

import { spawn } from "node:child_process";
import { appendFileSync } from "node:fs";

/** Every marker in the text, in order: `{ tool, args }`. */
export function toolMarkers(text) {
  const out = [];
  let from = 0;
  while (true) {
    const start = text.indexOf("[[tool ", from);
    if (start < 0) return out;
    const m = /^\[\[tool (\w+) /.exec(text.slice(start));
    if (!m) {
      from = start + 7;
      continue;
    }
    const json = start + m[0].length;
    let end = text.indexOf("}]]", json);
    let parsed = null;
    // The first `}]]` that closes valid JSON: an inner marker's does not.
    while (end >= 0) {
      try {
        parsed = JSON.parse(text.slice(json, end + 1));
        break;
      } catch {
        end = text.indexOf("}]]", end + 1);
      }
    }
    if (!parsed) {
      from = json;
      continue;
    }
    out.push({ tool: m[1], args: parsed });
    from = end + 3;
  }
}

/** A server spec as an engine got it, with its environment as a map. */
function envOf(server) {
  if (Array.isArray(server.env)) return Object.fromEntries(server.env.map((e) => [e.name, e.value]));
  return server.env ?? {};
}

/** Start the server, call each tool in order, stop it. Answers
 *  `{ tool, args, text, isError }` per call. */
export async function callTools(server, calls) {
  const dir = process.env.FAKE_CODEX_DIR;
  const log = (row) => dir && appendFileSync(`${dir}/agent-tools.jsonl`, JSON.stringify(row) + "\n");
  if (!server?.command) {
    const results = calls.map((c) => ({ ...c, text: "no switchyard server was given to this session", isError: true, missing: true }));
    for (const r of results) log(r);
    return results;
  }
  const child = spawn(server.command, server.args ?? [], { env: { ...process.env, ...envOf(server) }, stdio: ["pipe", "pipe", "inherit"] });
  let buffer = "";
  let next = 1;
  const waiting = new Map();
  child.stdout.on("data", (chunk) => {
    buffer += chunk;
    let i;
    while ((i = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, i);
      buffer = buffer.slice(i + 1);
      if (!line.trim()) continue;
      const msg = JSON.parse(line);
      waiting.get(msg.id)?.(msg);
      waiting.delete(msg.id);
    }
  });
  const rpc = (method, params) =>
    new Promise((resolve) => {
      const id = next++;
      waiting.set(id, resolve);
      child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    });
  const results = [];
  try {
    const init = await rpc("initialize", { protocolVersion: "2025-06-18", capabilities: {}, clientInfo: { name: "fake-engine", version: "0" } });
    if (init.error) throw new Error(init.error.message);
    child.stdin.write(JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" }) + "\n");
    const listed = await rpc("tools/list", {});
    const names = (listed.result?.tools ?? []).map((t) => t.name);
    log({ listed: names, messageAgent: listed.result?.tools?.find((t) => t.name === "message_agent")?.description ?? null });
    for (const call of calls) {
      const answer = await rpc("tools/call", { name: call.tool, arguments: call.args });
      const text = answer.result?.content?.map((c) => c.text).join("\n") ?? answer.error?.message ?? "";
      const result = { ...call, text, isError: !!answer.result?.isError || !!answer.error };
      log(result);
      results.push(result);
    }
  } finally {
    child.stdin.end();
    child.kill();
  }
  return results;
}

/** The answer a fake engine gives after its calls. */
export const afterCalls = (results) => results.map((r) => `Called ${r.tool}${r.isError ? " (refused)" : ""}: ${r.text.split("\n")[0]}`).join("\n");
