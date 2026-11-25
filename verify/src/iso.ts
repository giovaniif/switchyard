// Isolated environments: a yardd built from this checkout, on its own port,
// with its own data dir, HOME and scripted engines. Never the live yardd on
// 47717, never ~/.yard, never a real model unless asked for (`--engines real`).

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, openSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { type Args, DEFAULT_ISO, BUILD_PATHS, ensureDir, freePort, git, headState, LIVE_PORT, readJson, ROOT, run, sleep, STATE, str, VERIFY, writeJson } from "./common";
import { fail, Usage, type Result } from "./out";
import { connect } from "./ws";

export interface Iso {
  name: string;
  port: number;
  pid: number;
  root: string;
  data: string;
  home: string;
  fake: string;
  work: string;
  engines: "fake" | "real";
  /** What the daemon was built from. */
  build: Stamp;
  startedAt: string;
  url: string;
}

export interface Stamp {
  sha: string;
  short: string;
  dirty: boolean;
  diffHash: string | null;
  at: string;
}

const ISO_DIR = join(STATE, "iso");
const STAMP = join(STATE, "build.json");
const YARDD = join(ROOT, "target/debug/yardd");
const TEST_FAKES = join(ROOT, "crates/yardd/tests/fake-codex");
const OUR_FAKES = join(VERIFY, "fakes");

export const isoFile = (name: string) => join(ISO_DIR, name, "iso.json");
export const varsFile = (name: string) => join(ISO_DIR, name, "vars.json");

function newest(dir: string, skip: RegExp = /node_modules|dist|target/): number {
  let t = 0;
  if (!existsSync(dir)) return t;
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (skip.test(e.name)) continue;
    const p = join(dir, e.name);
    t = Math.max(t, e.isDirectory() ? newest(p, skip) : statSync(p).mtimeMs);
  }
  return t;
}

const mtime = (p: string) => (existsSync(p) ? statSync(p).mtimeMs : 0);

/** Is target/debug/yardd and ui/dist built from the source as it is now? */
export function buildFreshness(): { fresh: boolean; why: string[]; stamp: Stamp | null; head: ReturnType<typeof headState> } {
  const head = headState(ROOT, BUILD_PATHS);
  const stamp: Stamp | null = existsSync(STAMP) ? readJson(STAMP) : null;
  const why: string[] = [];
  if (!existsSync(YARDD)) why.push("target/debug/yardd is missing");
  if (!existsSync(join(ROOT, "ui/dist/index.html"))) why.push("ui/dist is missing");
  if (!stamp) why.push("no build stamp (.verify/build.json)");
  else {
    if (stamp.sha !== head.sha) why.push(`built from ${stamp.short}, HEAD is ${head.short}`);
    if (stamp.diffHash !== head.diffHash) why.push("uncommitted changes differ from the last build");
  }
  const rust = Math.max(newest(join(ROOT, "crates/yardd/src")), mtime(join(ROOT, "Cargo.lock")), mtime(join(ROOT, "scripts/remote-install.sh")));
  if (rust > mtime(YARDD)) why.push("crates/yardd changed after the last yardd build");
  const ui = Math.max(newest(join(ROOT, "ui/src")), mtime(join(ROOT, "ui/index.html")), newest(join(ROOT, "ui/public")));
  if (ui > mtime(join(ROOT, "ui/dist/index.html"))) why.push("ui changed after the last ui build");
  return { fresh: why.length === 0, why, stamp, head };
}

export function jobs(): string {
  return process.env.CARGO_BUILD_JOBS || (process.platform === "darwin" ? "2" : "4");
}

/** Builds yardd and the UI from the checkout as it is, and stamps what was built. */
export function build(force = false): Result {
  const before = buildFreshness();
  if (!force && before.fresh) return { ok: true, built: false, stamp: before.stamp };
  const steps: Record<string, unknown>[] = [];
  if (!existsSync(join(ROOT, "ui/node_modules"))) {
    const r = run(["bun", "install"], { cwd: join(ROOT, "ui") });
    if (r.code !== 0) fail("build-ui-deps", "bun install in ui failed", r.err.slice(-2000));
  }
  const t0 = Date.now();
  const ui = run(["bunx", "vite", "build"], { cwd: join(ROOT, "ui") });
  steps.push({ step: "ui", ok: ui.code === 0, ms: Date.now() - t0 });
  if (ui.code !== 0) fail("build-ui", "vite build failed", undefined, { log: (ui.out + ui.err).slice(-4000) });
  const t1 = Date.now();
  const cargo = run(["cargo", "build", "-p", "yardd"], { env: { CARGO_BUILD_JOBS: jobs() } });
  steps.push({ step: "yardd", ok: cargo.code === 0, ms: Date.now() - t1 });
  if (cargo.code !== 0) fail("build-yardd", "cargo build -p yardd failed", undefined, { log: cargo.err.slice(-4000) });
  if (!existsSync(join(ROOT, "sidecar/yard-claude/node_modules"))) run(["bun", "install"], { cwd: join(ROOT, "sidecar/yard-claude") });
  const head = headState(ROOT, BUILD_PATHS);
  const stamp: Stamp = { ...head, at: new Date().toISOString() };
  ensureDir(STATE);
  writeJson(STAMP, stamp);
  return { ok: true, built: true, stamp, steps };
}

export function loadIso(name = DEFAULT_ISO): Iso {
  const f = isoFile(name);
  if (!existsSync(f)) fail("iso-not-running", `no isolated environment named ${name}`, `verify/yv iso up${name === "default" ? "" : ` --name ${name}`}`);
  const iso: Iso = readJson(f);
  if (!alive(iso.pid)) fail("iso-dead", `isolated environment ${name} (pid ${iso.pid}) is not running`, `verify/yv iso down --name ${name} && verify/yv iso up --name ${name}`);
  return iso;
}

const alive = (pid: number) => {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
};

async function health(port: number): Promise<string | null> {
  try {
    const r = await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(1000) });
    return r.ok ? (await r.text()).trim() : null;
  } catch {
    return null;
  }
}

export async function isoUp(a: Args): Promise<Result> {
  const name = str(a, "name", DEFAULT_ISO)!;
  if (!/^[\w-]+$/.test(name)) throw new Usage("--name takes letters, digits, - and _");
  const engines = (str(a, "engines", "fake") as Iso["engines"]) ?? "fake";
  if (engines !== "fake" && engines !== "real") throw new Usage("--engines is fake or real");
  if (existsSync(isoFile(name))) {
    const old: Iso = readJson(isoFile(name));
    if (alive(old.pid)) fail("iso-exists", `isolated environment ${name} is already running on port ${old.port}`, `verify/yv iso down --name ${name}`, { iso: old });
    rmSync(join(ISO_DIR, name), { recursive: true, force: true });
  }
  let buildResult: Result | null = null;
  const fresh = buildFreshness();
  if (!fresh.fresh) {
    if (a.flags["no-build"]) fail("build-stale", `the build is stale: ${fresh.why.join("; ")}`, "verify/yv build");
    buildResult = build();
  }
  const stamp: Stamp = readJson(STAMP);

  const root = join(tmpdir(), `yv-${name}-${process.pid}-${Date.now().toString(36)}`);
  const dirs = { data: join(root, "data"), home: join(root, "home"), fake: join(root, "fake"), work: join(root, "work") };
  for (const d of Object.values(dirs)) mkdirSync(d, { recursive: true });
  const port = await freePort();
  if (port === LIVE_PORT) fail("live-port", "refusing the live yardd's port");

  // Agents usually run inside a Switchyard session, which inherits the live
  // yardd's YARD_* settings (YARD_PORT=47717, YARD_WORKSPACE, YARD_BIND...).
  // None of them may leak into an isolated daemon.
  const inherited = Object.fromEntries(Object.entries(process.env).filter(([k]) => !k.startsWith("YARD_") && !k.startsWith("FAKE_"))) as Record<string, string>;
  const env: Record<string, string> = {
    ...inherited,
    YARD_PORT: String(port),
    YARD_DATA_DIR: dirs.data,
    YARD_ROOT: ROOT,
    YARD_MIN_FREE_GB: "0",
    YARD_HOST_NAME: str(a, "host-name", "iso-host")!,
    SHELL: "/bin/bash",
    BASH_SILENCE_DEPRECATION_WARNING: "1",
  };
  if (engines === "fake") {
    // Scripted engines first on PATH, a throwaway HOME, gh and ssh that never leave the machine.
    env.PATH = `${OUR_FAKES}:${TEST_FAKES}:${process.env.PATH}`;
    env.HOME = dirs.home;
    env.FAKE_CODEX_DIR = dirs.fake;
    env.FAKE_CODEX_WRITE = "1";
    // The fake Claude sidecar's importable sessions live here.
    env.FAKE_CLAUDE_CWD = dirs.work;
    env.YARD_SIDECAR = join(TEST_FAKES, "yard-claude");
    env.YARD_SSH = join(OUR_FAKES, "ssh");
    env.YARD_WORKSPACE = dirs.work;
    env.GH_CONFIG_DIR = join(dirs.home, ".config/gh");
    // Run from inside a Switchyard session, these name that session's real account folders.
    for (const k of ["CLAUDE_CONFIG_DIR", "CODEX_HOME", "GROK_HOME"]) delete env[k];
    // Session naming adds engine turns; recipes that check it pass --env YARD_SESSION_NAMES=on.
    env.YARD_SESSION_NAMES = "off";
    if (!a.flags["logged-out"]) {
      writeFileSync(join(dirs.fake, "codex-logged-in"), "");
      writeFileSync(join(dirs.fake, "claude-logged-in"), "");
      writeFileSync(join(dirs.fake, "grok-logged-in"), "");
      writeFileSync(join(dirs.fake, "gemini-logged-in"), "");
    }
  } else {
    // Real engines need the real HOME for their logins; the data dir stays throwaway.
    env.YARD_SSH = join(OUR_FAKES, "ssh");
    env.YARD_WORKSPACE = dirs.work;
  }
  if (a.flags.pressure) env.YARD_FORCE_PRESSURE = "1";
  for (const kv of String(str(a, "env", "")).split(",").filter(Boolean)) {
    const [k, ...v] = kv.split("=");
    if (!k.startsWith("YARD_")) throw new Usage("--env only takes YARD_* settings");
    env[k] = v.join("=");
  }

  const { pid, body } = await startDaemon(root, dirs.data, port, env);
  const iso: Iso = { name, port, pid, root, ...dirs, engines, build: stamp, startedAt: new Date().toISOString(), url: `http://127.0.0.1:${port}` };
  ensureDir(join(ISO_DIR, name));
  writeJson(isoFile(name), iso);
  writeJson(join(ISO_DIR, name, "env.json"), env);
  writeJson(varsFile(name), { port: String(port), url: iso.url, work: dirs.work, home: dirs.home, fake: dirs.fake, data: dirs.data });
  return { ok: true, iso, health: body, build: buildResult, hint: `seed it with: verify/yv iso seed${name === "default" ? "" : ` --name ${name}`}` };
}

async function startDaemon(root: string, data: string, port: number, env: Record<string, string>): Promise<{ pid: number; body: string }> {
  const log = join(data, "yardd.log");
  const fd = openSync(log, "a");
  // Its own process group, so `iso down` stops yardd and every engine it started.
  const child = spawn(YARDD, [], { cwd: root, env, detached: true, stdio: ["ignore", fd, fd] });
  child.unref();
  const pid = child.pid!;
  let body: string | null = null;
  for (let i = 0; i < 100 && !body; i++) {
    await sleep(100);
    if (!alive(pid)) break;
    body = await health(port);
  }
  if (!body) {
    killGroup(pid);
    return fail("iso-start", `yardd did not answer /health on port ${port}`, undefined, { log: readFileSync(log, "utf8").slice(-3000) });
  }
  return { pid, body };
}

async function stopDaemon(pid: number) {
  if (!alive(pid)) return;
  killGroup(pid);
  for (let i = 0; i < 30 && alive(pid); i++) await sleep(100);
  if (alive(pid)) killGroup(pid, "SIGKILL");
}

/** Stops and starts the same daemon on the same port and data: what a user's restart looks like. */
export async function isoRestart(a: Args): Promise<Result> {
  const name = str(a, "name", DEFAULT_ISO)!;
  const iso = loadIso(name);
  const env: Record<string, string> = readJson(join(ISO_DIR, name, "env.json"));
  await stopDaemon(iso.pid);
  if (!a.flags["no-build"]) {
    const fresh = buildFreshness();
    if (!fresh.fresh) build();
  }
  const { pid, body } = await startDaemon(iso.root, iso.data, iso.port, env);
  const next = { ...iso, pid, build: readJson(STAMP), startedAt: new Date().toISOString() };
  writeJson(isoFile(name), next);
  return { ok: true, iso: next, health: body };
}

function killGroup(pid: number, sig: NodeJS.Signals = "SIGTERM") {
  try {
    process.kill(-pid, sig);
  } catch {
    try {
      process.kill(pid, sig);
    } catch {}
  }
}

export async function isoDown(a: Args): Promise<Result> {
  const names = a.flags.all ? (existsSync(ISO_DIR) ? readdirSync(ISO_DIR) : []) : [str(a, "name", DEFAULT_ISO)!];
  const stopped: string[] = [];
  for (const name of names) {
    const f = isoFile(name);
    if (!existsSync(f)) continue;
    const iso: Iso = readJson(f);
    // Only the process we started: its command line must be this checkout's yardd.
    const cmdline = existsSync(`/proc/${iso.pid}/cmdline`) ? readFileSync(`/proc/${iso.pid}/cmdline`, "utf8") : "";
    const ours = process.platform !== "linux" || cmdline.includes(YARDD) || !alive(iso.pid);
    if (ours) await stopDaemon(iso.pid);
    if (!a.flags.keep && iso.root.startsWith(tmpdir())) rmSync(iso.root, { recursive: true, force: true });
    rmSync(join(ISO_DIR, name), { recursive: true, force: true });
    stopped.push(name);
  }
  return { ok: true, stopped };
}

export function isoLs(): Result {
  const list = existsSync(ISO_DIR)
    ? readdirSync(ISO_DIR)
        .filter((n) => existsSync(isoFile(n)))
        .map((n) => {
          const iso: Iso = readJson(isoFile(n));
          return { name: n, port: iso.port, pid: iso.pid, alive: alive(iso.pid), engines: iso.engines, build: iso.build.short + (iso.build.dirty ? "+dirty" : ""), url: iso.url };
        })
    : [];
  return { ok: true, isos: list };
}

/** Writes a small git repository with one commit. */
function repo(dir: string, files: Record<string, string>) {
  for (const [path, body] of Object.entries(files)) {
    mkdirSync(join(dir, path, ".."), { recursive: true });
    writeFileSync(join(dir, path), body);
  }
  git(["init", "-q", "-b", "main"], dir);
  git(["add", "-A"], dir);
  git(["-c", "user.name=Test", "-c", "user.email=test@example.com", "commit", "-qm", "TEST-initial commit"], dir);
}

const DEMO_FILES = {
  "README.md": "# TEST-demo-app\n\nSynthetic repository for Switchyard verification.\n",
  "package.json": '{ "name": "test-demo-app", "private": true }\n',
  "src/greet.ts": 'export const greet = (name: string) => `Hello, ${name}`;\n',
  "src/lib.rs": "fn kept() {}\n",
};

/**
 * Synthetic data: two repositories and one session per scripted behaviour.
 * Every name starts with TEST- so it is obviously not real.
 */
export async function isoSeed(a: Args): Promise<Result> {
  const name = str(a, "name", DEFAULT_ISO)!;
  const iso = loadIso(name);
  const profile = str(a, "profile", "basic")!;
  if (!["empty", "basic"].includes(profile)) throw new Usage("--profile is empty or basic");
  const demo = join(iso.work, "TEST-demo-app");
  const plain = join(iso.work, "TEST-plain-folder");
  if (!existsSync(demo)) repo(demo, DEMO_FILES);
  mkdirSync(plain, { recursive: true });
  writeFileSync(join(plain, "notes.txt"), "TEST notes\n");
  const vars: Record<string, string> = { repo: demo, plain };
  const before = readJson(varsFile(name), {} as Record<string, string>);
  if (before.sid_echo && !a.flags.force) return { ok: true, profile, already: true, vars: before, hint: "already seeded; --force seeds again" };
  if (profile === "basic") {
    const ws = await connect(iso.port);
    try {
      await ws.ready();
      const make = async (key: string, text: string, extra: Record<string, unknown> = {}, settle = "turn_completed") => {
        const sid = await ws.create({ engine: "codex", model: "fake-fast", effort: "low", cwd: demo, worktree: true, permission: "ask", text, ...extra });
        vars[key] = sid;
        if (settle === "turn_completed") await ws.waitEvent(sid, "turn_completed");
        else if (settle) await ws.waitEvent(sid, settle);
        return sid;
      };
      await make("sid_fail", "[fail] TEST-failing turn");
      await make("sid_think", "[think] TEST-plan the refactor");
      await make("sid_echo", "TEST-say hello");
      await make("sid_edit", "[edit] TEST-fix the greeting");
      await make("sid_claude", "TEST-hello from Claude", { engine: "claude", model: "claude-haiku-5-5", effort: "medium" });
      await make("sid_cmd", "[cmd] TEST-run the migration", {}, "approval_request");
      // A pull request for the edit session's branch, answered by the fake gh.
      const branch = ws.sessions()[vars.sid_edit]?.branch;
      if (branch) {
        writeJson(join(iso.fake, "gh-pr.json"), fakePr(branch));
        vars.edit_branch = branch;
      }
    } finally {
      ws.close();
    }
  }
  const all = { ...readJson(varsFile(name), {}), ...vars };
  writeJson(varsFile(name), all);
  return { ok: true, profile, vars: all };
}

function fakePr(branch: string) {
  return {
    view: {
      number: 4242,
      title: "TEST-fix the greeting",
      url: "https://github.com/test-owner/test-demo-app/pull/4242",
      state: "OPEN",
      isDraft: false,
      baseRefName: "main",
      headRefName: branch,
      reviewDecision: "REVIEW_REQUIRED",
      mergeable: "MERGEABLE",
      mergeStateStatus: "BLOCKED",
      author: { login: "test-author" },
      additions: 12,
      deletions: 3,
      updatedAt: new Date(Date.now() - 3 * 3600_000).toISOString(),
      statusCheckRollup: [
        { __typename: "CheckRun", name: "test", workflowName: "CI", status: "COMPLETED", conclusion: "SUCCESS", detailsUrl: "https://example.com/1" },
        { __typename: "CheckRun", name: "lint", workflowName: "CI", status: "COMPLETED", conclusion: "FAILURE", detailsUrl: "https://example.com/2" },
        { __typename: "CheckRun", name: "build", workflowName: "CI", status: "IN_PROGRESS", conclusion: null, detailsUrl: "https://example.com/3" },
      ],
    },
    // More PRs for hover cards on links in a thread: merged and draft.
    peeks: {
      "https://github.com/test-owner/test-demo-app/pull/4100": {
        number: 4100,
        title: "TEST-add the greeting module",
        url: "https://github.com/test-owner/test-demo-app/pull/4100",
        state: "MERGED",
        isDraft: false,
        author: { login: "test-author" },
        baseRefName: "main",
        headRefName: "test-greeting",
        reviewDecision: "APPROVED",
        mergeStateStatus: "UNKNOWN",
        additions: 140,
        deletions: 0,
        updatedAt: new Date(Date.now() - 5 * 86400_000).toISOString(),
        statusCheckRollup: [{ __typename: "CheckRun", name: "test", workflowName: "CI", status: "COMPLETED", conclusion: "SUCCESS" }],
      },
      "https://github.com/test-owner/test-demo-app/pull/4243": {
        number: 4243,
        title: "TEST-draft: a much longer pull request title that has to wrap onto a second line inside the hover card",
        url: "https://github.com/test-owner/test-demo-app/pull/4243",
        state: "OPEN",
        isDraft: true,
        author: { login: "test-author" },
        baseRefName: "main",
        headRefName: "test-draft",
        reviewDecision: null,
        mergeStateStatus: "DRAFT",
        additions: 1,
        deletions: 1,
        updatedAt: new Date(Date.now() - 20 * 60_000).toISOString(),
        statusCheckRollup: [],
      },
    },
    threads: [
      {
        isResolved: false,
        isOutdated: false,
        path: "src/greet.ts",
        line: 1,
        originalLine: 1,
        comments: { nodes: [{ author: { login: "test-reviewer" }, body: "TEST-review: greet should trim the name.", url: "https://example.com/c1" }] },
      },
      {
        isResolved: true,
        isOutdated: false,
        path: "README.md",
        line: 2,
        originalLine: 2,
        comments: { nodes: [{ author: { login: "test-reviewer" }, body: "TEST-review: resolved nit.", url: "https://example.com/c2" }] },
      },
    ],
  };
}
