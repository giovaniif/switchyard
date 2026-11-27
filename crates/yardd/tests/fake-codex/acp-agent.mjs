// A scripted ACP agent (Agent Client Protocol, JSON-RPC lines on stdio), the
// part `opencode acp` and `grok agent stdio` share in these fakes. No turn
// ever reaches a model.
//
// A turn's behaviour is picked by a marker in the prompt:
//   [cmd]    a command that asks permission, then runs ("hi") or is refused
//   [edit]   an edit of notes.txt with a diff, asking permission first; the
//            file is written when allowed and FAKE_CODEX_WRITE is set
//   [think]  two thought chunks, then a reply
//   [slow]   one chunk, then waits for session/cancel (stopReason cancelled)
//   [stuck]  one chunk, then never ends, whatever it is asked
//   [late]   like [stuck], but 6 s after session/cancel (past Stop's grace)
//            it streams a thought and "Late words", then still never ends
//   [fail]   the prompt fails with an error
//   [exit]   the agent process exits mid-turn
//   [[tool NAME {json}]]  calls Switchyard's agent tool NAME through the
//            `switchyard` server in the session's `mcpServers` (see
//            agent-tools.mjs): a `switchyard_NAME` tool call that asks
//            permission first, then "Called NAME: <result>" per call
//   other    "Echo: <prompt>" in three chunks
//
// FAKE_CODEX_DIR gets `acp.log` (a line per start, request and answer) and
// `acp-prompts.jsonl` (every prompt with the session's model, effort and
// mode at that moment). Sessions are saved in the engine's state folder, so a
// restarted agent resumes one it made and refuses one it never saw.

import { appendFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { afterCalls, callTools, toolMarkers } from "./agent-tools.mjs";

export function runAgent({ engine, state, account, loggedIn }) {
  const dir = process.env.FAKE_CODEX_DIR;
  const log = (line) => dir && appendFileSync(`${dir}/acp.log`, `${engine} ${line}\n`);
  log(`start pid=${process.pid} account=${account}`);
  mkdirSync(state, { recursive: true });
  const sessionsFile = `${state}/acp-sessions.json`;
  const sessions = new Map(existsSync(sessionsFile) ? JSON.parse(readFileSync(sessionsFile, "utf8")) : []);
  const save = () => writeFileSync(sessionsFile, JSON.stringify([...sessions]));

  const write = (msg) => process.stdout.write(`${JSON.stringify({ jsonrpc: "2.0", ...msg })}\n`);
  const update = (sessionId, u) => write({ method: "session/update", params: { sessionId, update: u } });
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  let n = 0;
  let nextAsk = 5000;
  const waiting = new Map();
  const ask = (method, params) =>
    new Promise((resolve) => {
      const id = nextAsk++;
      waiting.set(id, resolve);
      write({ id, method, params });
    });
  const cancelled = new Map();
  // Each session's `switchyard` server, as session/new, resume or load gave it.
  const tools = new Map();
  const keepTools = (sid, servers) => tools.set(sid, (servers ?? []).find((s) => s.name === "switchyard") ?? null);

  const MODELS = [
    { modelId: "fake-small", name: "Fake Small", efforts: ["low", "high"] },
    { modelId: "fake-large", name: "Fake Large", efforts: [] },
  ];
  const modelIds = engine === "opencode" ? ["opencode/fake-free", "fakeprov/fake-pro"] : MODELS.map((m) => m.modelId);
  if (engine === "opencode" && process.env.YARD_TEST_OPENCODE_V2 === "1") modelIds.push("opencode-go/fake-go");
  const effortId = engine === "opencode" ? "effort" : "reasoning_effort";
  const configOptions = (s) => {
    const list = [
      { id: "model", name: "Model", category: "model", type: "select", currentValue: s.model, options: modelIds.map((v) => ({ value: v, name: v })) },
      { id: effortId, name: "Effort", category: "thought_level", type: "select", currentValue: s.effort, options: ["default", "low", "high"].map((v) => ({ value: v, name: v })) },
    ];
    if (engine === "opencode") {
      list.push({ id: "mode", name: "Session Mode", category: "mode", type: "select", currentValue: s.mode, options: [{ value: "build", name: "build" }, { value: "plan", name: "plan" }] });
    }
    return list;
  };
  const modes = (s) =>
    engine === "gemini"
      ? { currentModeId: s.mode, availableModes: ["default", "auto_edit", "yolo"].map((id) => ({ id, name: id })) }
      : engine === "grok"
      ? { currentModeId: s.mode, availableModes: ["default", "acceptEdits", "plan", "bypassPermissions"].map((id) => ({ id, name: id })) }
      : undefined;
  const opened = (id) => {
    const s = sessions.get(id);
    return { ...(engine === "gemini" ? { models: { currentModelId: s.model, availableModels: MODELS.map(({ modelId, name }) => ({ modelId, name })) } } : { configOptions: configOptions(s) }), ...(modes(s) ? { modes: modes(s) } : {}) };
  };

  async function prompt(id, params) {
    const sid = params.sessionId;
    const s = sessions.get(sid);
    if (!s) return write({ id, error: { code: -32602, message: `unknown session ${sid}` } });
    const text = (params.prompt ?? []).filter((b) => b.type === "text").map((b) => b.text).join("\n");
    if (dir) {
      appendFileSync(
        `${dir}/acp-prompts.jsonl`,
        JSON.stringify({ engine, account, sessionId: sid, model: s.model, effort: s.effort, mode: s.mode, prompt: params.prompt }) + "\n",
      );
    }
    cancelled.set(sid, false);
    const say = async (reply) => {
      const third = Math.ceil(reply.length / 3);
      for (const part of [reply.slice(0, third), reply.slice(third, 2 * third), reply.slice(2 * third)]) {
        if (part) update(sid, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: part } });
        await sleep(5);
      }
    };
    const permission = (toolCall) =>
      ask("session/request_permission", {
        sessionId: sid,
        toolCall,
        options: [
          { optionId: "once", kind: "allow_once", name: "Allow once" },
          { optionId: "always", kind: "allow_always", name: "Always allow" },
          { optionId: "reject", kind: "reject_once", name: "Reject" },
        ],
      });
    const allowed = (answer) => answer?.outcome?.outcome === "selected" && answer.outcome.optionId !== "reject";

    const calls = toolMarkers(text);
    if (calls.length) {
      const results = [];
      for (const c of calls) {
        const call = { toolCallId: `call-${++n}`, title: `switchyard_${c.tool}`, kind: "other", status: "pending", rawInput: c.args };
        update(sid, { sessionUpdate: "tool_call", ...call });
        if (!allowed(await permission(call))) {
          update(sid, { sessionUpdate: "tool_call_update", toolCallId: call.toolCallId, status: "failed" });
          results.push({ ...c, text: "refused", isError: true });
          continue;
        }
        const [r] = await callTools(tools.get(sid), [c]);
        update(sid, { sessionUpdate: "tool_call_update", toolCallId: call.toolCallId, status: "completed", content: [{ type: "content", content: { type: "text", text: r.text } }] });
        results.push(r);
      }
      await say(afterCalls(results));
      return write({ id, result: { stopReason: "end_turn" } });
    }
    if (text.includes("[fail]")) return write({ id, error: { code: -32603, message: "fake failure" } });
    if (text.includes("[exit]")) {
      update(sid, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Going…" } });
      await sleep(20);
      process.exit(3);
    }
    if (text.includes("[think]")) {
      update(sid, { sessionUpdate: "agent_thought_chunk", content: { type: "text", text: "Weighing it. " } });
      update(sid, { sessionUpdate: "agent_thought_chunk", content: { type: "text", text: "Decided." } });
      await say("Thought it through.");
      return write({ id, result: { stopReason: "end_turn" } });
    }
    if (text.includes("[cmd]")) {
      const call = { toolCallId: `call-${++n}`, title: "bash", kind: "execute", status: "pending", rawInput: { command: "echo hi" } };
      update(sid, { sessionUpdate: "tool_call", ...call });
      const answer = await permission(call);
      if (allowed(answer)) {
        update(sid, { sessionUpdate: "tool_call_update", toolCallId: call.toolCallId, status: "completed", content: [{ type: "content", content: { type: "text", text: "hi" } }] });
        await say("Ran it.");
      } else {
        update(sid, { sessionUpdate: "tool_call_update", toolCallId: call.toolCallId, status: "failed", content: [{ type: "content", content: { type: "text", text: "refused" } }] });
        await say("Skipped it.");
      }
      return write({ id, result: { stopReason: cancelled.get(sid) ? "cancelled" : "end_turn" } });
    }
    if (text.includes("[edit]")) {
      const path = `${s.cwd}/notes.txt`;
      const before = existsSync(path) ? readFileSync(path, "utf8") : null;
      const diff = { type: "diff", path, oldText: before, newText: "edited by the fake\n" };
      const call = { toolCallId: `call-${++n}`, title: "Edit notes.txt", kind: "edit", status: "pending", locations: [{ path }], content: [diff] };
      update(sid, { sessionUpdate: "tool_call", ...call });
      const answer = await permission(call);
      if (allowed(answer)) {
        if (process.env.FAKE_CODEX_WRITE) writeFileSync(path, diff.newText);
        update(sid, { sessionUpdate: "tool_call_update", toolCallId: call.toolCallId, status: "completed", content: [diff] });
        await say("Edited notes.txt.");
      } else {
        update(sid, { sessionUpdate: "tool_call_update", toolCallId: call.toolCallId, status: "failed" });
        await say("Left it alone.");
      }
      return write({ id, result: { stopReason: "end_turn" } });
    }
    if (text.includes("[slow]") || text.includes("[stuck]") || text.includes("[late]")) {
      update(sid, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Working" } });
      if (text.includes("[late]")) {
        while (!cancelled.get(sid)) await sleep(20);
        await sleep(6000);
        update(sid, { sessionUpdate: "agent_thought_chunk", content: { type: "text", text: "Late thought" } });
        update(sid, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "Late words" } });
        return;
      }
      if (text.includes("[stuck]")) return;
      while (!cancelled.get(sid)) await sleep(20);
      return write({ id, result: { stopReason: "cancelled" } });
    }
    await say(`Echo: ${text.split("\n")[0]}`);
    write({ id, result: { stopReason: "end_turn" } });
  }

  const handle = async (msg) => {
    if (msg.method === undefined && waiting.has(msg.id)) {
      log(`answer ${JSON.stringify(msg.result ?? msg.error)}`);
      const resolve = waiting.get(msg.id);
      waiting.delete(msg.id);
      return resolve(msg.result);
    }
    const p = msg.params ?? {};
    log(`req ${msg.method}${msg.method === "session/set_config_option" ? ` ${p.configId}=${JSON.stringify(p.value)}` : ""}${msg.method === "session/set_mode" ? ` ${p.modeId}` : ""}`);
    switch (msg.method) {
      case "initialize":
        return write({
          id: msg.id,
          result: {
            protocolVersion: 1,
            agentCapabilities: { loadSession: true, promptCapabilities: { image: true }, ...(engine === "gemini" ? {} : { sessionCapabilities: { resume: {} } }) },
            authMethods: [{ id: engine, name: engine }],
            ...(engine === "grok"
              ? {
                  _meta: {
                    modelState: {
                      currentModelId: "fake-small",
                      availableModels: MODELS.map((m) => ({
                        modelId: m.modelId,
                        name: m.name,
                        _meta: {
                          supportsReasoningEffort: m.efforts.length > 0,
                          reasoningEffort: m.efforts.length ? "high" : undefined,
                          reasoningEfforts: m.efforts.map((e) => ({ id: e, value: e, default: e === "high" })),
                        },
                      })),
                    },
                  },
                }
              : {}),
          },
        });
      // Grok's credit allowance, as its agent answers the TUI's /usage. The
      // field names are the ones in grok 1.0.77's BillingConfig; the nesting
      // and value types are a guess (no signed-in account to read one from).
      case "_x.ai/billing": {
        if (engine !== "grok") return write({ id: msg.id, error: { code: -32601, message: "Method not found" } });
        if (!loggedIn()) return write({ id: msg.id, error: { code: -32000, message: "Authentication required", data: "Authentication required to fetch billing data" } });
        const start = new Date(Date.now() - 2 * 86400_000);
        const end = new Date(Date.now() + 5 * 86400_000 + 3 * 3600_000 + 30_000);
        return write({
          id: msg.id,
          result: {
            creditUsagePercent: 23,
            currentPeriod: { start: start.toISOString(), end: end.toISOString() },
            billingPeriodStart: start.toISOString(),
            billingCycle: "WEEKLY",
            monthlyLimit: null,
            onDemandCap: 0,
            onDemandUsed: 0,
            prepaidBalance: 0,
            isUnifiedBillingUser: true,
            history: [],
          },
        });
      }
      case "session/new": {
        if (!loggedIn()) return write({ id: msg.id, error: { code: -32000, message: "Authentication required", data: "no auth method id provided" } });
        const sid = `${engine}-ses-${process.pid}-${++n}`;
        sessions.set(sid, { cwd: p.cwd, model: modelIds[0], effort: "default", mode: engine === "opencode" ? "build" : "default" });
        save();
        keepTools(sid, p.mcpServers);
        log(`mcpServers ${(p.mcpServers ?? []).map((s) => s.name).join(",")}`);
        log(`servers session/new ${JSON.stringify(p.mcpServers ?? [])}`);
        return write({ id: msg.id, result: { sessionId: sid, ...opened(sid) } });
      }
      case "session/resume":
      case "session/load": {
        if (!sessions.has(p.sessionId)) return write({ id: msg.id, error: { code: -32002, message: `Session not found: ${p.sessionId}` } });
        keepTools(p.sessionId, p.mcpServers);
        log(`servers ${msg.method} ${JSON.stringify(p.mcpServers ?? [])}`);
        if (msg.method === "session/load") {
          update(p.sessionId, { sessionUpdate: "user_message_chunk", content: { type: "text", text: "old prompt" } });
          update(p.sessionId, { sessionUpdate: "agent_message_chunk", content: { type: "text", text: "old answer" } });
        }
        return write({ id: msg.id, result: opened(p.sessionId) });
      }
      case "session/set_model": {
        const s = sessions.get(p.sessionId);
        if (!s || !modelIds.includes(p.modelId)) return write({ id: msg.id, error: { code: -32602, message: "unknown model" } });
        s.model = p.modelId;
        save();
        return write({ id: msg.id, result: {} });
      }
      case "session/set_config_option": {
        if (engine === "gemini") return write({ id: msg.id, error: { code: -32601, message: "unsupported" } });
        const s = sessions.get(p.sessionId);
        if (!s) return write({ id: msg.id, error: { code: -32602, message: "unknown session" } });
        const value = typeof p.value === "string" ? p.value : p.value?.value;
        if (p.configId === "model") {
          if (!modelIds.includes(value)) return write({ id: msg.id, error: { code: -32602, message: `unknown model ${value}` } });
          s.model = value;
        } else if (p.configId === effortId) s.effort = value;
        else if (p.configId === "mode") s.mode = value;
        else return write({ id: msg.id, error: { code: -32602, message: `unknown option ${p.configId}` } });
        save();
        return write({ id: msg.id, result: { configOptions: configOptions(s) } });
      }
      case "session/set_mode": {
        const s = sessions.get(p.sessionId);
        if (s) s.mode = p.modeId;
        save();
        return write({ id: msg.id, result: {} });
      }
      case "session/prompt":
        return prompt(msg.id, p);
      case "session/cancel":
        cancelled.set(p.sessionId, true);
        return;
      default:
        if (msg.id !== undefined) write({ id: msg.id, error: { code: -32601, message: `unknown method ${msg.method}` } });
    }
  };

  let buffer = "";
  process.stdin.on("data", (chunk) => {
    buffer += chunk;
    let i;
    while ((i = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, i);
      buffer = buffer.slice(i + 1);
      if (line.trim()) handle(JSON.parse(line));
    }
  });
  process.stdin.on("end", () => process.exit(0));
}
