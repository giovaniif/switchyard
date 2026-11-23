// `api`: one op to an isolated yardd, optionally waiting for a reply and
// saving values from it as variables for later commands (`{{name}}`).
// `state`: the session list as the daemon sees it.

import { existsSync } from "node:fs";
import { type Args, DEFAULT_ISO, matches, parseJsonArg, pick, readJson, str, writeJson } from "./common";
import { loadIso, varsFile } from "./iso";
import { fail, Usage, type Result } from "./out";
import { connect } from "./ws";

/** Ops that spend a real subscription when the isolated yardd runs real engines. */
const SPENDING = new Set(["session.create", "session.send", "session.setModel", "session.handoff", "session.startNow"]);
/** The only real-engine turn allowed without --allow-spend (repo rule: cheap Codex only). */
const CHEAP = { engine: "codex", model: "gpt-5.6-luna", effort: "low" };

export function loadVars(name: string): Record<string, string> {
  return existsSync(varsFile(name)) ? readJson(varsFile(name)) : {};
}

export function saveVars(name: string, add: Record<string, string>) {
  writeJson(varsFile(name), { ...loadVars(name), ...add });
}

/** Replaces `{{name}}` with saved variables; an unknown name is a `needs-var` failure. */
export function substitute(text: string, vars: Record<string, string>): string {
  return text.replace(/\{\{(\w+)\}\}/g, (_, k) => {
    if (vars[k] === undefined) fail("needs-var", `variable {{${k}}} is not set`, "run the recipe's Setup first, or `verify/yv iso seed`");
    return vars[k];
  });
}

export async function api(a: Args): Promise<Result> {
  const name = str(a, "iso", DEFAULT_ISO)!;
  const iso = loadIso(name);
  const vars = loadVars(name);
  if (!a.pos[0]) throw new Usage("api needs an op as JSON, e.g. '{\"op\":\"session.open\",\"sessionId\":\"{{sid_echo}}\"}'");
  const msg = parseJsonArg(substitute(a.pos[0], vars), "the op");
  if (typeof msg.op !== "string") throw new Usage("the op needs an `op` field");
  if (iso.engines === "real" && SPENDING.has(msg.op) && !a.flags["allow-spend"]) {
    const sid = msg.sessionId ? (await snapshotSessions(iso.port))[msg.sessionId] : null;
    const engine = msg.engine ?? sid?.engine;
    const model = msg.model ?? sid?.model;
    const effort = msg.effort ?? sid?.effort;
    if (engine !== CHEAP.engine || model !== CHEAP.model || effort !== CHEAP.effort)
      fail("spend-refused", `real engines: only ${CHEAP.engine} ${CHEAP.model} ${CHEAP.effort} turns run without --allow-spend`, "use the fake engines (the default), or the cheap Codex model");
  }
  const wantText = str(a, "wait");
  const want = wantText ? parseJsonArg(substitute(wantText, vars), "--wait") : null;
  const timeout = Number(str(a, "timeout", "20000"));
  const ws = await connect(iso.port);
  try {
    await ws.ready();
    const from = ws.log.length;
    ws.send(msg);
    let reply: any = null;
    if (want) {
      reply = await ws.waitFor(`a message matching ${JSON.stringify(want)}`, (m) => matches(m, want) || (m.op === "error" && m.for === msg.op), timeout, from);
      if (reply.op === "error" && !matches(reply, want)) fail("api-error", `${msg.op} failed: ${reply.message}`, undefined, { reply });
    } else {
      // No reply expected: give yardd a moment to refuse it.
      await new Promise((r) => setTimeout(r, Number(str(a, "collect", "400"))));
      const err = ws.log.slice(from).find((m) => m.op === "error" && m.for === msg.op);
      if (err) fail("api-error", `${msg.op} failed: ${err.message}`, undefined, { reply: err });
    }
    const expect = str(a, "expect");
    if (expect) {
      const wantReply = parseJsonArg(substitute(expect, vars), "--expect");
      if (!matches(reply, wantReply)) fail("expect-failed", `the reply does not match ${expect}`, undefined, { reply });
    }
    const saved: Record<string, string> = {};
    for (const spec of saveSpecs(a)) {
      const [k, path] = spec;
      const v = pick(reply, path);
      if (v === undefined || v === null) fail("save-missing", `--save ${k}=${path}: nothing at that path`, undefined, { reply });
      saved[k] = typeof v === "string" ? v : JSON.stringify(v);
    }
    if (Object.keys(saved).length) saveVars(name, saved);
    const received = ws.log.slice(from);
    return { ok: true, sent: msg, reply, saved, received: a.flags.all ? received : received.length };
  } finally {
    ws.close();
  }
}

function saveSpecs(a: Args): [string, string][] {
  const raw = a.flags.save;
  if (!raw || raw === true) return [];
  return substitute(String(raw), loadVars(str(a, "iso", DEFAULT_ISO)!))
    .split(",")
    .map((s) => {
      const i = s.indexOf("=");
      if (i <= 0) throw new Usage("--save takes name=path[,name=path]");
      return [s.slice(0, i), s.slice(i + 1)] as [string, string];
    });
}

async function snapshotSessions(port: number) {
  const ws = await connect(port);
  try {
    await ws.ready();
    return ws.sessions();
  } finally {
    ws.close();
  }
}

/** The daemon's view: sessions (id, title, status, engine, branch, pinned, archived, group, automation, agent, updatedAt), hosts, models. */
export async function state(a: Args): Promise<Result> {
  const name = str(a, "iso", DEFAULT_ISO)!;
  const iso = loadIso(name);
  const ws = await connect(iso.port);
  try {
    const snap = await ws.ready();
    await new Promise((r) => setTimeout(r, 300));
    const sessions = Object.values(ws.sessions()).map((s: any) => ({
      id: s.id,
      title: s.title,
      status: s.status,
      engine: s.engine,
      model: s.model,
      permission: s.permission,
      branch: s.branch,
      pinned: !!s.pinned,
      archived: !!s.archived,
      group: s.group ?? null,
      cwd: s.cwd,
      automation: s.automation ?? null,
      agent: s.agent ?? null,
      // Every session.update bumps it, so a recipe can prove nothing was sent.
      updatedAt: s.updatedAt,
    }));
    const out: Result = { ok: true, sessions, host: snap.host ? { name: snap.host.name, pressure: snap.host.pressure ?? null } : null, vars: loadVars(name) };
    for (const spec of saveSpecs(a)) {
      const v = pick({ sessions }, spec[1]);
      if (v === undefined) fail("save-missing", `--save ${spec[0]}=${spec[1]}: nothing at that path`, undefined, { sessions });
      saveVars(name, { [spec[0]]: String(v) });
    }
    const expect = str(a, "expect");
    if (expect) {
      const want = parseJsonArg(substitute(expect, loadVars(name)), "--expect");
      if (!matches({ sessions, host: out.host }, want)) fail("expect-failed", `the daemon state does not match ${expect}`, undefined, { sessions });
    }
    if (a.flags.full) out.snapshot = snap;
    return out;
  } finally {
    ws.close();
  }
}

/** Sets or reads variables by hand: `var set name value`, `var get name`, `var ls`. */
export function vars(a: Args): Result {
  const name = str(a, "iso", DEFAULT_ISO)!;
  const [sub, k, v] = a.pos;
  if (sub === "set" && k && v !== undefined) {
    saveVars(name, { [k]: v });
    return { ok: true, set: { [k]: v } };
  }
  if (sub === "get" && k) {
    const all = loadVars(name);
    if (all[k] === undefined) fail("needs-var", `variable ${k} is not set`);
    return { ok: true, name: k, value: all[k] };
  }
  if (sub === "ls" || !sub) return { ok: true, vars: loadVars(name) };
  throw new Usage("var set <name> <value> | var get <name> | var ls");
}
