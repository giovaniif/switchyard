// The evidence bundle: proofs tied to a claim and to the base and head commits.
// A PR's claim is backed by artifacts, not prose. `verify` is the proof bar.

import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join, resolve } from "node:path";
import { type Args, git, gitOut, headState, mergeBase, nowIso, readJson, REPO, ROOT, run, sha256File, str, VERIFY, writeJson } from "./common";
import { fail, type Problem, Usage, type Result } from "./out";

export const REFS = ["base", "head"] as const;
export const KINDS = ["repro", "fix", "test", "design", "regression", "note"] as const;
type Ref = (typeof REFS)[number];
type Kind = (typeof KINDS)[number];

export interface Artifact {
  path: string;
  sha256: string;
}

export interface Proof {
  id: number;
  ref: Ref;
  kind: Kind;
  /** The commit the proof was taken at (the checkout's HEAD, or the build that served a screenshot). */
  at: string;
  dirty: boolean;
  viewports: string[];
  text?: string;
  cmd?: string;
  exit?: number;
  failedApi?: string[];
  failingRecipes?: string[];
  passingRecipes?: string[];
  artifacts: Artifact[];
  addedAt: string;
}

export interface Bundle {
  v: 1;
  claim: string;
  repo: string;
  base: string;
  head: string;
  /** Files changed between base and head, with git's status letter (A/M/D/R...). */
  changed: { status: string; path: string }[];
  ui: boolean;
  createdAt: string;
  proofs: Proof[];
}

const file = (dir: string) => join(dir, "bundle.json");

/** UI changes need both viewports and a design check. Deletions and renames count. */
export const isUiPath = (p: string) => p.startsWith("ui/src/") || p.startsWith("ui/index.html") || p.startsWith("ui/public/");

export function changedFiles(base: string, head: string, cwd = ROOT): { status: string; path: string }[] {
  return git(["diff", "--name-status", "-M", `${base}..${head}`], cwd)
    .out.trim()
    .split("\n")
    .filter(Boolean)
    .flatMap((l) => {
      const [status, ...paths] = l.split("\t");
      return paths.map((path) => ({ status: status[0], path }));
    });
}

export function load(dir: string): Bundle {
  if (!existsSync(file(dir))) fail("no-bundle", `no evidence bundle in ${dir}`, `verify/yv evidence init ${dir} --claim "..."`);
  return readJson(file(dir));
}

function save(dir: string, b: Bundle) {
  writeJson(file(dir), b);
}

function init(dir: string, a: Args): Result {
  const claim = str(a, "claim");
  if (!claim || claim.length < 20) throw new Usage('--claim "<the problem, restated: what the user sees, why it is wrong>" (20+ characters)');
  if (existsSync(file(dir)) && !a.flags.force) fail("bundle-exists", `${dir} already has a bundle`, "use another dir, or --force to start over");
  const head = gitOut(["rev-parse", str(a, "head", "HEAD")!]);
  const base = str(a, "base") ? gitOut(["rev-parse", str(a, "base")!]) : mergeBase(head);
  if (base === head) fail("no-change", "head is the merge-base with main: there is nothing to prove yet", "commit the fix first, or pass --head <fix sha>");
  const changed = changedFiles(base, head);
  mkdirSync(dir, { recursive: true });
  const b: Bundle = { v: 1, claim, repo: ROOT, base, head, changed, ui: changed.some((c) => isUiPath(c.path)), createdAt: nowIso(), proofs: [] };
  save(dir, b);
  return { ok: true, dir: resolve(dir), bundle: b, hint: "record a base repro next: check out the base, `verify/yv iso up`, drive it, then `evidence add --ref base --kind repro --file <shot.json>`" };
}

/** Copies an artifact into the bundle so later edits to the original cannot change the proof. */
function keep(dir: string, src: string, id: number): Artifact {
  if (!existsSync(src)) fail("no-file", `${src} does not exist`);
  // Two files of one proof can share a name (a drive.json from each of two
  // runs); copying both to one path keeps only the last and breaks its hash.
  let dest = join(dir, `p${id}-${basename(src)}`);
  for (let n = 2; existsSync(dest); n++) dest = join(dir, `p${id}-${n}-${basename(src)}`);
  copyFileSync(src, dest);
  return { path: basename(dest), sha256: sha256File(dest) };
}

/** A yv shot/drive/map JSON: what it proves, and which build produced it. */
function readYvJson(path: string) {
  const j = JSON.parse(readFileSync(path, "utf8"));
  const pngs: string[] = [];
  const viewports = new Set<string>();
  const failedApi: string[] = [];
  for (const s of j.shots ?? []) {
    pngs.push(s.file);
    viewports.add(s.viewport);
    for (const f of s.failedApi ?? []) failedApi.push(`${f.for ?? f.op}: ${f.message}`);
  }
  for (const r of j.runs ?? []) {
    viewports.add(r.viewport);
    for (const c of r.captures ?? []) pngs.push(c.file);
    if (r.failed?.screenshot) pngs.push(r.failed.screenshot);
    if (r.video) pngs.push(r.video);
    for (const f of r.failedApi ?? []) failedApi.push(`${f.for ?? f.op}: ${f.message}`);
  }
  const failing: string[] = [];
  const passing: string[] = [];
  for (const rec of j.records ?? []) {
    for (const r of rec.results ?? []) (r.status === "fail" ? failing : r.status === "pass" ? passing : []).push(`${rec.area}/${r.label}`);
  }
  return { j, pngs: pngs.filter((p) => existsSync(p)), viewports: [...viewports], failedApi, failing, passing, build: j.build as { sha: string; dirty: boolean } | undefined };
}

function add(dir: string, a: Args): Result {
  const b = load(dir);
  const ref = str(a, "ref") as Ref;
  const kind = str(a, "kind") as Kind;
  if (!REFS.includes(ref)) throw new Usage("--ref base|head");
  if (!KINDS.includes(kind)) throw new Usage(`--kind ${KINDS.join("|")}`);
  const id = b.proofs.length + 1;
  const now = headState();
  const proof: Proof = { id, ref, kind, at: now.sha, dirty: now.dirty, viewports: [], artifacts: [], addedAt: nowIso() };
  const warnings: Problem[] = [];
  const text = str(a, "text");
  if (text) proof.text = text;

  // pos: ["add", dir, ...files]
  const files = a.pos.slice(2);
  const flagFiles = a.flags.file;
  if (typeof flagFiles === "string") files.push(...flagFiles.split(","));
  for (const f of files) {
    if (f.endsWith(".json")) {
      const y = readYvJson(f);
      if (y.build) {
        // The proof is about the build that served the page, not whatever is checked out now.
        proof.at = y.build.sha;
        proof.dirty = y.build.dirty;
      }
      proof.viewports.push(...y.viewports);
      proof.failedApi = [...(proof.failedApi ?? []), ...y.failedApi];
      if (y.j.records) {
        proof.failingRecipes = y.failing;
        proof.passingRecipes = y.passing;
      }
      proof.artifacts.push(keep(dir, f, id));
      for (const p of y.pngs) proof.artifacts.push(keep(dir, p, id));
      if (kind === "fix" && y.j.ok === false) warnings.push({ code: "fix-not-clean", msg: `${f} reports ok:false: ${(y.j.problems ?? []).map((p: any) => p.msg).join("; ")}` });
    } else {
      proof.artifacts.push(keep(dir, f, id));
    }
  }
  const viewport = str(a, "viewport");
  if (viewport) proof.viewports.push(...(viewport === "both" ? ["mobile", "desktop"] : [viewport]));
  proof.viewports = [...new Set(proof.viewports)];

  const cmd = str(a, "cmd");
  if (cmd) {
    // The CLI runs the command itself, so the exit code is observed, not reported.
    const t0 = Date.now();
    const r = run(["bash", "-c", cmd], { timeoutMs: Number(str(a, "timeout", "1800000")) });
    proof.cmd = cmd;
    proof.exit = r.code;
    const log = join(dir, `p${id}-cmd.log`);
    writeFileSync(log, `$ ${cmd}\n# exit ${r.code} after ${Date.now() - t0} ms at ${now.sha}${now.dirty ? " (dirty)" : ""}\n\n${r.out}\n${r.err}`);
    proof.artifacts.push({ path: basename(log), sha256: sha256File(log) });
  } else if (str(a, "exit") !== undefined) {
    fail("exit-needs-cmd", "--exit is not taken on trust: pass --cmd and the CLI runs it");
  }
  if (kind === "design" && !cmd && files.length === 0) {
    const r = run([join(VERIFY, "yv"), "design-check", "--base", b.base]);
    proof.cmd = `verify/yv design-check --base ${b.base.slice(0, 12)}`;
    proof.exit = r.code;
    const log = join(dir, `p${id}-design.json`);
    writeFileSync(log, r.out);
    proof.artifacts.push({ path: basename(log), sha256: sha256File(log) });
  }
  if (kind === "regression" && !cmd && files.length === 0) {
    const area = str(a, "area");
    if (!area) throw new Usage("--kind regression needs --file <map run json>, or --area <area|all> to run it now");
    const r = run([join(VERIFY, "yv"), "map", "run", area], { timeoutMs: 3600000 });
    const log = join(dir, `p${id}-map-${area}.json`);
    writeFileSync(log, r.out);
    const y = readYvJson(log);
    proof.cmd = `verify/yv map run ${area}`;
    proof.exit = r.code;
    proof.failingRecipes = y.failing;
    proof.passingRecipes = y.passing;
    proof.artifacts.push({ path: basename(log), sha256: sha256File(log) });
  }

  if (kind !== "note" && !proof.cmd && proof.artifacts.length === 0) throw new Usage(`a ${kind} proof needs --file, --cmd${kind === "design" ? "" : ""} or (for design/regression) nothing to run it now`);
  if (kind === "note" && !proof.text) throw new Usage('a note needs --text (e.g. "no-repro: <why>")');

  // Where was it taken?
  if (kind !== "note") {
    if (ref === "head") {
      if (proof.at !== b.head) fail("wrong-ref", `this proof was taken at ${proof.at.slice(0, 12)}, the bundle's head is ${b.head.slice(0, 12)}`, "check out the head commit, rebuild (`verify/yv iso restart` builds), and take it again");
      if (proof.dirty) fail("dirty", "head proofs need a clean checkout: uncommitted changes would not be in the PR", "commit or stash, rebuild, take it again");
    } else {
      const anc = git(["merge-base", "--is-ancestor", proof.at, b.head]).code === 0;
      if (!anc || proof.at === b.head) fail("wrong-ref", `a base proof must be taken at the base or a commit before the fix (got ${proof.at.slice(0, 12)})`, `git checkout ${b.base.slice(0, 12)} (or the test-only commit), rebuild, take it again`);
      if (proof.dirty) fail("dirty", "base proofs need a clean checkout");
    }
  }
  b.proofs.push(proof);
  save(dir, b);
  return { ok: true, proof, warnings };
}

export interface Check {
  rule: string;
  ok: boolean;
  msg: string;
}

const hasNote = (b: Bundle, prefix: string) => b.proofs.some((p) => p.kind === "note" && p.text?.toLowerCase().startsWith(prefix));

/** The proof bar. Pure over the bundle, the artifact files and the checkout's HEAD. */
export function verifyBundle(dir: string, b: Bundle, checkoutHead: string | null): Check[] {
  const checks: Check[] = [];
  const P = (ref: Ref, kind: Kind) => b.proofs.filter((p) => p.ref === ref && p.kind === kind);

  const repro = P("base", "repro");
  checks.push({ rule: "base-repro", ok: repro.length > 0 || hasNote(b, "no-repro:"), msg: repro.length ? `${repro.length} base repro(s)` : hasNote(b, "no-repro:") ? "no-repro note" : "no base repro and no `no-repro: <why>` note" });

  const fixes = P("head", "fix");
  const need = b.ui ? ["mobile", "desktop"] : [];
  const covered = new Set(fixes.flatMap((p) => p.viewports));
  const missing = need.filter((v) => !covered.has(v));
  checks.push({
    rule: "head-fix",
    ok: fixes.length > 0 && missing.length === 0,
    msg: fixes.length === 0 ? "no head fix proof" : missing.length ? `UI change: no head fix at ${missing.join(", ")}` : `${fixes.length} head fix proof(s)${b.ui ? " at both viewports" : ""}`,
  });

  const baseApi = new Set(b.proofs.filter((p) => p.ref === "base").flatMap((p) => p.failedApi ?? []));
  // Some fixes are a refusal: an op that now says no instead of doing the
  // wrong thing, or an error reworded. Each such op is named, with a reason,
  // in an `expected-api: <op>: <why>` note, so the verifier sees and judges it.
  const expectedOps = new Set(
    b.proofs
      .filter((p) => p.kind === "note" && p.text?.toLowerCase().startsWith("expected-api:"))
      .map((p) => p.text!.slice("expected-api:".length).split(":"))
      .filter((parts) => parts.length > 1 && parts.slice(1).join(":").trim())
      .map((parts) => parts[0].trim()),
  );
  const headApi = [...new Set(b.proofs.filter((p) => p.ref === "head").flatMap((p) => p.failedApi ?? []))].filter((f) => !baseApi.has(f));
  const excused = headApi.filter((f) => expectedOps.has(f.split(":")[0]));
  const newApi = headApi.filter((f) => !excused.includes(f));
  const excusedMsg = excused.length ? ` (expected by a note: ${excused.join(" | ")})` : "";
  checks.push({ rule: "api-failures", ok: newApi.length === 0, msg: newApi.length ? `failed API calls only on head: ${newApi.join(" | ")}` : `no failed API call that base does not also have${excusedMsg}` });

  const tests = P("head", "test");
  const passed = tests.filter((p) => p.exit === 0);
  const failedHead = tests.filter((p) => p.exit !== 0);
  checks.push({ rule: "head-tests", ok: passed.length > 0 && failedHead.length === 0, msg: passed.length === 0 ? "no passing head test run" : failedHead.length ? `${failedHead.length} head test run(s) failed` : `${passed.length} passing head test run(s)` });

  const red = P("base", "test").filter((p) => p.exit !== undefined && p.exit !== 0);
  checks.push({ rule: "failing-test-first", ok: red.length > 0 || hasNote(b, "no-failing-test:"), msg: red.length ? "the regression test failed before the fix" : hasNote(b, "no-failing-test:") ? "no-failing-test note" : "no base test run that failed, and no `no-failing-test: <why>` note" });

  if (b.ui) {
    const d = P("head", "design");
    checks.push({ rule: "design-check", ok: d.length > 0 && d.every((p) => p.exit === 0 || p.exit === undefined), msg: d.length === 0 ? "UI change without a design check" : d.some((p) => p.exit) ? "design check failed" : "design check recorded" });
  }

  const baseFail = new Set(b.proofs.filter((p) => p.ref === "base").flatMap((p) => p.failingRecipes ?? []));
  const headOnly = [...new Set(P("head", "regression").flatMap((p) => p.failingRecipes ?? []))].filter((r) => !baseFail.has(r));
  checks.push({ rule: "regressions", ok: headOnly.length === 0, msg: headOnly.length ? `recipes failing only on head: ${headOnly.join(", ")}` : P("head", "regression").length ? "no head-only regression" : "no regression run recorded (allowed)" });

  const bad: string[] = [];
  for (const p of b.proofs)
    for (const art of p.artifacts) {
      const f = join(dir, art.path);
      if (!existsSync(f) || sha256File(f) !== art.sha256) bad.push(art.path);
    }
  checks.push({ rule: "artifacts", ok: bad.length === 0, msg: bad.length ? `changed or missing since recorded: ${bad.join(", ")}` : "every artifact matches its hash" });

  if (checkoutHead !== null) checks.push({ rule: "checkout", ok: checkoutHead === b.head, msg: checkoutHead === b.head ? "the checkout is at the recorded head" : `the checkout is at ${checkoutHead.slice(0, 12)}, the bundle's head is ${b.head.slice(0, 12)}` });
  return checks;
}

function verify(dir: string, a: Args): Result {
  const b = load(dir);
  const head = a.flags["no-checkout"] ? null : gitOut(["rev-parse", "HEAD"]);
  const checks = verifyBundle(dir, b, head);
  const failed = checks.filter((c) => !c.ok);
  return { ok: failed.length === 0, claim: b.claim, base: b.base, head: b.head, ui: b.ui, checks, problems: failed.map((c) => ({ code: c.rule, msg: c.msg })) };
}

function show(dir: string): Result {
  const b = load(dir);
  return { ok: true, dir: resolve(dir), bundle: b, files: readdirSync(dir) };
}

/** A Markdown section for the PR body, built from the bundle. */
export function markdown(dir: string, urls: Record<string, string> = {}): string {
  const b = load(dir);
  const checks = verifyBundle(dir, b, null);
  const lines = [
    "## Verification",
    "",
    `**Claim:** ${b.claim}`,
    "",
    `Base \`${b.base.slice(0, 7)}\` → head \`${b.head.slice(0, 7)}\`. Evidence bundle: ${urls["bundle.json"] ? `[bundle.json](${urls["bundle.json"]})` : "`" + dir + "`"}`,
    "",
    "| Rule | Result |",
    "|---|---|",
    ...checks.map((c) => `| ${c.rule} | ${c.ok ? "✅" : "❌"} ${c.msg} |`),
    "",
    "| # | Ref | Kind | At | Viewports | Command / note |",
    "|---|---|---|---|---|---|",
    ...b.proofs.map((p) => `| ${p.id} | ${p.ref} | ${p.kind} | \`${p.at.slice(0, 7)}\` | ${p.viewports.join(", ") || "-"} | ${(p.cmd ? `\`${p.cmd}\` → exit ${p.exit}` : "") + (p.text ? ` ${p.text}` : "")} |`),
  ];
  return lines.join("\n");
}

/** A new head after review fixes: base proofs stay (same merge-base), head proofs must be taken again. */
function retarget(dir: string, a: Args): Result {
  const b = load(dir);
  const head = gitOut(["rev-parse", str(a, "head", "HEAD")!]);
  const base = mergeBase(head);
  if (base !== b.base && !a.flags.force) fail("base-moved", `the merge-base moved (${b.base.slice(0, 7)} → ${base.slice(0, 7)}): base proofs no longer apply`, "start a new bundle, or --force to keep them anyway");
  const dropped = b.proofs.filter((p) => p.ref === "head").length;
  b.proofs = b.proofs.filter((p) => p.ref === "base");
  b.head = head;
  b.base = base;
  b.changed = changedFiles(base, head);
  b.ui = b.changed.some((c) => isUiPath(c.path));
  save(dir, b);
  return { ok: true, head, dropped, kept: b.proofs.length, hint: "add the head proofs again: fix, test, design" };
}

/** Pushes the bundle and its artifacts to the asset branch; the PR links bundle.json. */
function publish(dir: string, a: Args): Result {
  const b = load(dir);
  const files = readdirSync(dir).map((f) => join(dir, f));
  const label = str(a, "label") ?? `evidence-${b.head.slice(0, 7)}`;
  const r = run([join(VERIFY, "yv"), "assets", "push", ...files, "--label", label, ...(a.flags["dry-run"] ? ["--dry-run"] : [])]);
  const j = JSON.parse(r.out);
  if (!j.ok) fail("publish-failed", j.problems?.[0]?.msg ?? "assets push failed");
  const urls: Record<string, string> = {};
  for (const [f, u] of Object.entries(j.urls as Record<string, string>)) urls[basename(f)] = u;
  writeJson(join(dir, "published.json"), { label, commit: j.commit, urls });
  return { ok: true, bundleUrl: urls["bundle.json"], commit: j.commit, markdown: markdown(dir, urls), hint: "put `markdown` in the PR body's Verification section; the verifier fetches bundleUrl" };
}

/** The published file for one artifact under `prefix`: `<hash>-<name>`. */
export function pickArtifact<T extends { path: string }>(tree: T[], prefix: string, art: { path: string; sha256: string }): T | undefined {
  const named = tree.filter((t) => t.path.startsWith(prefix) && t.path.endsWith(`-${art.path}`));
  // A bundle published twice leaves two files with the same name under the
  // prefix; the recorded hash picks the one this proof made.
  return named.find((t) => t.path === `${prefix}${art.sha256.slice(0, 12)}-${art.path}`) ?? named[0];
}

/** Downloads a published bundle (bundle.json URL) with its artifacts into dir. */
async function fetchBundle(url: string, dir: string): Promise<Result> {
  if (!/^https:\/\/raw\.githubusercontent\.com\//.test(url)) throw new Usage("evidence fetch <raw.githubusercontent.com URL of bundle.json> <dir>");
  mkdirSync(dir, { recursive: true });
  const get = async (u: string) => {
    const r = await fetch(u, { signal: AbortSignal.timeout(30000) });
    if (!r.ok) fail("fetch-failed", `${u}: HTTP ${r.status}`);
    return Buffer.from(await r.arrayBuffer());
  };
  const bundle: Bundle = JSON.parse((await get(url)).toString());
  writeFileSync(file(dir), JSON.stringify(bundle, null, 2) + "\n");
  // Artifacts were pushed next to bundle.json, each name prefixed by its hash.
  const listing = url.replace(/[^/]+$/, "");
  const commit = url.split("/")[5];
  const tree = JSON.parse(run(["gh", "api", `repos/${REPO}/git/trees/${commit}?recursive=1`]).out || "{}");
  const prefix = listing.split(`/${commit}/`)[1];
  let n = 0;
  for (const p of bundle.proofs)
    for (const art of p.artifacts) {
      const entry = pickArtifact(tree.tree ?? [], prefix, art);
      if (!entry) continue;
      writeFileSync(join(dir, art.path), await get(`https://raw.githubusercontent.com/${REPO}/${commit}/${entry.path}`));
      n++;
    }
  return { ok: true, dir: resolve(dir), proofs: bundle.proofs.length, artifacts: n, head: bundle.head };
}

export async function evidence(a: Args): Promise<Result> {
  const [sub, dir] = a.pos;
  if (!dir && sub !== undefined) throw new Usage(`evidence ${sub} <dir>`);
  switch (sub) {
    case "init":
      return init(dir, a);
    case "add":
      return add(dir, a);
    case "verify":
      return verify(dir, a);
    case "show":
      return show(dir);
    case "markdown":
      return { ok: true, markdown: markdown(dir, existsSync(join(dir, "published.json")) ? readJson(join(dir, "published.json")).urls : {}) };
    case "publish":
      return publish(dir, a);
    case "retarget":
      return retarget(dir, a);
    case "fetch":
      return fetchBundle(dir, a.pos[2] ?? "");
    default:
      throw new Usage("evidence init|add|verify|show|markdown|publish|retarget <dir> | evidence fetch <bundle-url> <dir>");
  }
}

export const _test = { keep };
