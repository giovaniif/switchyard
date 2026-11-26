// `shot` and `drive`: the UI of an isolated yardd in headless Chromium, at
// fixed viewports, with console errors, page errors and failed API calls
// (yardd `error` replies and HTTP failures) collected on every run.

import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { chromium, type Browser, type BrowserContext, type Locator, type Page } from "playwright";
import { substitute, loadVars } from "./api";
import { type Args, DEFAULT_ISO, ensureDir, parseJsonArg, readJson, run, sha256, SHARED, STATE, str } from "./common";
import { type Iso, loadIso } from "./iso";
import { fail, Usage, type Result } from "./out";

export const VIEWPORTS = {
  mobile: { width: 390, height: 844, deviceScaleFactor: 2, isMobile: true, hasTouch: true },
  desktop: { width: 1440, height: 900, deviceScaleFactor: 2, isMobile: false, hasTouch: false },
} as const;
export type ViewportName = keyof typeof VIEWPORTS;

/** Console noise that is not ours. Never add the app's own errors here. */
const NOISE = [/Download the React DevTools/i, /\[vite\] connect/i];
/** Replies that carry `error` as an ordinary answer, not a failure. */
const BENIGN_API = [
  { op: "git.branches", re: /is not a git repository/ },
  { op: "changes", re: /not a git repository/ },
];
const QUIET_MS = 700;
const SETTLE_MAX_MS = 10000;

interface Watch {
  consoleErrors: string[];
  pageErrors: string[];
  /** yardd replies that are errors: `op: error`, or a reply carrying `error`. */
  failedApi: { op: string; for?: string; message: string }[];
  httpFailures: string[];
  lastFrame: number;
  inflight: number;
}

/**
 * The isolated app talks only to its own yardd. Google Fonts (the UI's Inter
 * and JetBrains Mono) are served from a local cache, fetched once, so
 * screenshots look real without a slow font host stalling navigation; every
 * other outside request is blocked.
 */
const FONT_HOSTS = /^https:\/\/fonts\.(googleapis|gstatic)\.com\//;
const NET_CACHE = () => ensureDir(join(SHARED, "net-cache"));

async function isolate(ctx: BrowserContext, iso: Iso) {
  await ctx.route(
    (url) => !url.href.startsWith(iso.url) && !["data:", "blob:"].includes(url.protocol),
    async (route) => {
      const url = route.request().url();
      if (!FONT_HOSTS.test(url)) return route.abort("blockedbyclient");
      const f = join(NET_CACHE(), sha256(url).slice(0, 32));
      if (existsSync(`${f}.body`)) return route.fulfill({ status: 200, headers: readJson(`${f}.json`), body: readFileSync(`${f}.body`) });
      try {
        const res = await route.fetch({ timeout: 5000 });
        const body = await res.body();
        if (res.ok()) {
          const keep = Object.fromEntries(Object.entries(res.headers()).filter(([k]) => ["content-type", "access-control-allow-origin"].includes(k)));
          writeFileSync(`${f}.body`, body);
          writeFileSync(`${f}.json`, JSON.stringify(keep));
        }
        return route.fulfill({ response: res, body });
      } catch {
        return route.abort("timedout");
      }
    },
  );
}

const thirdParty = (url: string, own: string) => !!url && !url.startsWith(own) && !url.startsWith("data:") && !url.startsWith("blob:");

function watch(page: Page, own: string): Watch {
  const w: Watch = { consoleErrors: [], pageErrors: [], failedApi: [], httpFailures: [], lastFrame: Date.now(), inflight: 0 };
  page.on("console", (m) => {
    // A blocked or slow outside resource is not the app's error.
    if (thirdParty(m.location()?.url ?? "", own) && /Failed to load resource/.test(m.text())) return;
    if (m.type() === "error" && !NOISE.some((r) => r.test(m.text()))) w.consoleErrors.push(m.text().slice(0, 500));
  });
  page.on("pageerror", (e) => w.pageErrors.push(String(e.message).slice(0, 500)));
  page.on("request", () => w.inflight++);
  const done = () => (w.inflight = Math.max(0, w.inflight - 1));
  page.on("requestfinished", (r) => {
    done();
    r.response().then((res) => {
      if (res && res.status() >= 400 && !thirdParty(r.url(), own)) w.httpFailures.push(`${res.status()} ${r.url()}`);
    });
  });
  page.on("requestfailed", (r) => {
    done();
    if (!thirdParty(r.url(), own)) w.httpFailures.push(`${r.failure()?.errorText ?? "failed"} ${r.url()}`);
  });
  page.on("websocket", (ws) => {
    ws.on("framesent", () => (w.lastFrame = Date.now()));
    ws.on("framereceived", (f) => {
      w.lastFrame = Date.now();
      const text = typeof f.payload === "string" ? f.payload : f.payload.toString();
      if (!text.includes('"error"')) return;
      try {
        const m = JSON.parse(text);
        if (m.op === "error") w.failedApi.push({ op: "error", for: m.for, message: String(m.message).slice(0, 300) });
        else if (typeof m.error === "string" && BENIGN_API.some((b) => b.op === m.op && b.re.test(m.error))) return;
        else if (typeof m.error === "string") w.failedApi.push({ op: m.op, message: m.error.slice(0, 300) });
        else if (typeof m.pr?.error === "string") w.failedApi.push({ op: m.op, message: m.pr.error.slice(0, 300) });
        else if (typeof m.changes?.error === "string" && BENIGN_API.some((b) => b.op === m.op && b.re.test(m.changes.error))) return;
        else if (typeof m.changes?.error === "string") w.failedApi.push({ op: m.op, message: m.changes.error.slice(0, 300) });
      } catch {}
    });
  });
  return w;
}

/** Waits until no HTTP request is in flight and the WebSocket has been quiet for 700 ms. */
async function settle(page: Page, w: Watch): Promise<boolean> {
  const end = Date.now() + SETTLE_MAX_MS;
  while (Date.now() < end) {
    if (w.inflight === 0 && Date.now() - w.lastFrame >= QUIET_MS) return true;
    await page.waitForTimeout(100);
  }
  return false;
}

export type Target = string | { role?: string; name?: string; exact?: boolean; text?: string; label?: string; placeholder?: string; testId?: string; css?: string; hasText?: string; within?: Target; nth?: number };

/** role:button:New session, text:Hello, label:Message, placeholder:..., testid:..., css:.x, or an object. */
export function parseTarget(t: Target): Exclude<Target, string> {
  if (typeof t !== "string") return t;
  const i = t.indexOf(":");
  const kind = i > 0 ? t.slice(0, i) : "text";
  const rest = i > 0 ? t.slice(i + 1) : t;
  switch (kind) {
    case "role": {
      const j = rest.indexOf(":");
      return j > 0 ? { role: rest.slice(0, j), name: rest.slice(j + 1) } : { role: rest };
    }
    case "text":
      return { text: rest };
    case "label":
      return { label: rest };
    case "placeholder":
      return { placeholder: rest };
    case "testid":
      return { testId: rest };
    case "css":
      return { css: rest };
    default:
      return { text: t };
  }
}

function locate(page: Page, t: Target): Locator {
  const o = parseTarget(t);
  const all = locateAll(page, o);
  return o.nth !== undefined ? all.nth(o.nth) : all.first();
}

/** Every visible match of a target. */
function locateAll(page: Page, t: Target): Locator {
  const o = parseTarget(t);
  const scope: Page | Locator = o.within ? locate(page, o.within) : page;
  let loc: Locator;
  if (o.role) loc = scope.getByRole(o.role as any, o.name !== undefined ? { name: o.name, exact: o.exact ?? false } : {});
  else if (o.testId) loc = scope.getByTestId(o.testId);
  else if (o.label) loc = scope.getByLabel(o.label, { exact: o.exact ?? false });
  else if (o.placeholder) loc = scope.getByPlaceholder(o.placeholder, { exact: o.exact ?? false });
  else if (o.text !== undefined) loc = scope.getByText(o.text, { exact: o.exact ?? false });
  else if (o.css) loc = scope.locator(o.css);
  else throw new Usage(`target ${JSON.stringify(t)} names no locator`);
  if (o.hasText !== undefined) loc = loc.filter({ hasText: o.hasText });
  // Act on the first visible match: hidden duplicates (menus, the other layout) are common.
  return loc.filter({ visible: true });
}

/** On a narrow viewport the sidebar is a drawer: open it, as a user would, before using it. */
async function openDrawer(page: Page): Promise<boolean> {
  const menu = page.getByRole("button", { name: "Open sessions", exact: true }).filter({ visible: true });
  if ((await menu.count()) === 0) return false;
  await menu.first().click();
  await page.locator(".sidebar").waitFor({ state: "visible", timeout: 5000 });
  return true;
}

/** Opens a route: new, hosts, settings, connectors, agents, automations, session:<id | title text>, or slack:<conversation name>. */
async function goRoute(page: Page, iso: Iso, route: string, w: Watch) {
  if (!page.url().startsWith(iso.url)) {
    await page.goto(iso.url, { waitUntil: "domcontentloaded" });
    // Attached, not visible: on a phone the sidebar is a closed drawer.
    await page.locator(".sidebar").waitFor({ state: "attached", timeout: 10000 });
    await settle(page, w);
  }
  const drawer = route !== "" && (await openDrawer(page));
  if (route === "new") {
    await page.getByRole("button", { name: "New session" }).first().click();
  } else if (route === "hosts") {
    // Every host row can be hidden in Settings; Settings links to Hosts too.
    const row = page.locator(".host-row").first();
    if (await row.count()) await row.click();
    else {
      await page.getByRole("button", { name: "Settings", exact: true }).first().click();
      await page.getByRole("button", { name: "Manage hosts" }).click();
    }
  } else if (route === "subscriptions" || route === "sessions") {
    await page.getByRole("button", { name: "Go to session or page" }).click();
    const search = page.getByRole("combobox", { name: "Search sessions and pages" });
    await search.fill(route);
    await search.press("Enter");
  } else if (route === "settings") {
    await page.getByRole("button", { name: "Settings", exact: true }).first().click();
  } else if (route === "agents" || route === "automations") {
    await page.getByRole("button", { name: route === "agents" ? "Agents" : "Automations", exact: true }).first().click();
  } else if (route === "connectors") {
    await page.getByRole("button", { name: "Connectors" }).first().click();
  } else if (route.startsWith("session:")) {
    const key = route.slice("session:".length);
    let title = key;
    const byId = /^s[0-9a-f]{8,}$/.test(key) || key.includes("~");
    if (byId) {
      const res = await fetchTitle(iso, key);
      if (!res) fail("no-session", `no session ${key} in the isolated yardd`, "verify/yv state");
      title = res!;
    }
    const row = page.locator(".session-row", { hasText: title });
    if ((await row.count()) === 0) {
      // Archived rows stay hidden until the section opens.
      const toggle = page.locator(".list-toggle");
      if ((await toggle.count()) && (await toggle.getAttribute("aria-expanded")) === "false") await toggle.click();
    }
    if ((await row.count()) === 0) fail("no-session", `no sidebar row for ${title}`, "verify/yv state");
    await row.first().click();
    // Titles repeat (a hand-off keeps its title), so an id is checked against
    // the URL the row opened, and the next row with that title is tried.
    if (byId) {
      const want = `#/s/${encodeURIComponent(key)}`;
      const opened = () =>
        page.waitForFunction((h) => location.hash === h, want, { timeout: 2000 }).then(
          () => true,
          () => false,
        );
      let found = await opened();
      for (let i = 1; !found && i < (await row.count()); i++) {
        if (drawer) await openDrawer(page);
        await row.nth(i).click();
        found = await opened();
      }
      if (!found) fail("no-session", `no sidebar row opens ${key} (title ${title})`, "verify/yv state");
    }
  } else if (route.startsWith("slack:")) {
    // One of the owner's own Slack conversations, by its name in the sidebar.
    const name = route.slice("slack:".length);
    const row = page.locator(".slack-row", { hasText: name });
    if ((await row.count()) === 0) {
      const all = page.locator(".slack-dms .list-toggle", { hasText: "Show all" });
      if (await all.count()) await all.click();
    }
    if ((await row.count()) === 0) fail("no-session", `no Slack conversation ${name} in the sidebar`, "verify/yv state");
    await row.first().click();
  } else if (route !== "") {
    throw new Usage(`unknown route ${route}: new | hosts | subscriptions | sessions | settings | connectors | agents | automations | session:<id or title> | slack:<conversation name>`);
  }
  // Picking a row closes the drawer; a shot taken mid-slide would show it half open.
  if (drawer) await page.locator(".sidebar").waitFor({ state: "hidden", timeout: 5000 });
  await settle(page, w);
}

async function fetchTitle(iso: Iso, id: string): Promise<string | null> {
  const { connect } = await import("./ws");
  const ws = await connect(iso.port);
  try {
    await ws.ready();
    // Remote sessions come in host.snapshot messages shortly after connecting.
    for (let i = 0; i < 30 && !ws.sessions()[id]; i++) await new Promise((r) => setTimeout(r, 100));
    return ws.sessions()[id]?.title ?? null;
  } finally {
    ws.close();
  }
}

function viewports(a: Args): ViewportName[] {
  const v = str(a, "viewport", "both")!;
  if (v === "both") return ["mobile", "desktop"];
  if (v in VIEWPORTS) return [v as ViewportName];
  throw new Usage("--viewport is mobile, desktop or both");
}

function outDir(a: Args, kind: string): string {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const dir = str(a, "out") ?? join(STATE, kind, stamp);
  mkdirSync(dir, { recursive: true });
  return dir;
}

/** PNG width and height from the IHDR chunk. */
export function pngSize(path: string): { width: number; height: number } {
  const b = readFileSync(path);
  return { width: b.readUInt32BE(16), height: b.readUInt32BE(20) };
}

async function bannerTexts(page: Page): Promise<string[]> {
  return page.locator(".banner").allInnerTexts().catch(() => []);
}

interface Capture {
  viewport: ViewportName;
  file: string;
  size: { width: number; height: number };
}

async function snap(page: Page, dir: string, label: string, vp: ViewportName, problems: Result["problems"]): Promise<Capture> {
  const file = join(dir, `${label}-${vp}.png`);
  // Finish CSS transitions first: a shot right after "Open sessions" caught
  // the phone drawer mid-slide (0.2 s), half off screen, with every step passing.
  await page.screenshot({ path: file, animations: "disabled" });
  const size = pngSize(file);
  const want = VIEWPORTS[vp];
  if (size.width !== want.width * want.deviceScaleFactor || size.height !== want.height * want.deviceScaleFactor)
    problems!.push({ code: "bad-shot-size", msg: `${file} is ${size.width}x${size.height}, expected ${want.width * 2}x${want.height * 2}` });
  return { viewport: vp, file, size };
}

async function withBrowser<T>(fn: (b: Browser) => Promise<T>, showScrollbars = false): Promise<T> {
  const browser = await chromium.launch({ ignoreDefaultArgs: showScrollbars ? ["--hide-scrollbars"] : undefined });
  try {
    return await fn(browser);
  } finally {
    await browser.close();
  }
}

function summary(w: Watch) {
  return { consoleErrors: w.consoleErrors, pageErrors: w.pageErrors, failedApi: w.failedApi, httpFailures: w.httpFailures };
}

const dirty = (w: Watch) => w.consoleErrors.length + w.pageErrors.length + w.failedApi.length + w.httpFailures.length > 0;

/** Screenshots of one route at each viewport, with what went wrong on the way. */
export async function shot(a: Args): Promise<Result> {
  const name = str(a, "iso", DEFAULT_ISO)!;
  const iso = loadIso(name);
  const route = substitute(a.pos[0] ?? "new", loadVars(name));
  const label = (str(a, "name") ?? route.replace(/[^\w-]+/g, "_")).slice(0, 60);
  const dir = outDir(a, "shots");
  const problems: NonNullable<Result["problems"]> = [];
  const runs: Record<string, unknown>[] = [];
  await withBrowser(async (browser) => {
    for (const vp of viewports(a)) {
      const ctx = await browser.newContext({ viewport: { width: VIEWPORTS[vp].width, height: VIEWPORTS[vp].height }, deviceScaleFactor: 2, isMobile: VIEWPORTS[vp].isMobile, hasTouch: VIEWPORTS[vp].hasTouch });
      await isolate(ctx, iso);
      const page = await ctx.newPage();
      const w = watch(page, iso.url);
      const t0 = Date.now();
      try {
        await goRoute(page, iso, route, w);
        const settled = await settle(page, w);
        const banners = await bannerTexts(page);
        if (banners.some((b) => b.includes("not reachable"))) problems.push({ code: "wrong-state", msg: `${vp}: the page shows "yardd is not reachable"` });
        const cap = await snap(page, dir, label, vp, problems);
        const rec = { ...cap, settled, banners, ms: Date.now() - t0, ...summary(w) };
        runs.push(rec);
        if (dirty(w)) problems.push({ code: "errors", msg: `${vp}: ${w.consoleErrors.length} console, ${w.pageErrors.length} page, ${w.failedApi.length} API, ${w.httpFailures.length} HTTP errors` });
      } finally {
        await ctx.close();
      }
    }
  });
  const result: Result = { ok: problems.length === 0 || (!!a.flags["allow-errors"] && problems.every((p) => p.code === "errors")), route, build: iso.build, dir, shots: runs, problems };
  writeFileSync(join(dir, `${label}.json`), JSON.stringify(result, null, 2));
  return result;
}

export type Step = Record<string, any>;

async function runStep(page: Page, iso: Iso, step: Step, w: Watch, ctx: { dir: string; vp: ViewportName; captures: Capture[]; notes: any[]; problems: NonNullable<Result["problems"]>; idx: number }) {
  const timeout = step.timeout ?? 10000;
  if (step.goto !== undefined) return goRoute(page, iso, step.goto, w);
  if (step.resize) {
    const { width, height } = step.resize;
    if (!Number.isInteger(width) || width <= 0 || !Number.isInteger(height) || height <= 0) {
      throw new Usage("resize needs positive integer width and height");
    }
    await page.setViewportSize({ width, height });
    return settle(page, w);
  }
  if (step.click) {
    await locate(page, step.click).click({ timeout, button: step.button ?? "left" });
    return settle(page, w);
  }
  // A real pointer resting on the target (hover cards), and a finger on it
  // (mobile viewport only: its context has touch).
  if (step.hover) {
    await locate(page, step.hover).hover({ timeout });
    return;
  }
  if (step.tap) {
    await locate(page, step.tap).tap({ timeout });
    return settle(page, w);
  }
  if (step.select) {
    await locate(page, step.select).selectOption(String(step.value), { timeout });
    return settle(page, w);
  }
  if (step.fill) {
    await locate(page, step.fill).fill(String(step.value ?? ""), { timeout });
    return;
  }
  if (step.type !== undefined) {
    // Key by key, for widgets that ignore fill (the xterm terminal).
    if (step.target) await locate(page, step.target).click({ timeout });
    await page.keyboard.type(String(step.type), { delay: 10 });
    return;
  }
  if (step.drag) {
    // HTML5 drag and drop: press on one target, move onto another, release.
    // `hold` waits over the target before releasing (spring-loaded folders);
    // `during` screenshots the drag while it is held there.
    if (!step.to) throw new Usage(`drag needs "to", the target to drop on: ${JSON.stringify(step)}`);
    const src = locate(page, step.drag);
    const dst = locate(page, step.to);
    if (!step.hold && !step.during) await src.dragTo(dst, { timeout });
    else {
      // The first move after the press only starts the drag; the target sees
      // dragover from the next one on, so arrive in steps and nudge.
      const onto = async (dx: number) => {
        await dst.waitFor({ state: "visible", timeout });
        const b = await dst.boundingBox();
        if (!b) throw new Error(`drag target ${JSON.stringify(step.to)} has no box`);
        await page.mouse.move(b.x + b.width / 2 + dx, b.y + b.height / 2, { steps: 4 });
      };
      await src.hover({ timeout });
      await page.mouse.down();
      // Start the drag where it was picked up: some targets only appear once a drag is on.
      const from = await src.boundingBox();
      if (from) await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2 + 6, { steps: 3 });
      await onto(0);
      await onto(1);
      await page.waitForTimeout(Number(step.hold ?? 100));
      await onto(0);
      if (step.during) ctx.captures.push(await snap(page, ctx.dir, String(step.during), ctx.vp, ctx.problems));
      await page.mouse.up();
    }
    return settle(page, w);
  }
  if (step.upload) {
    const files = (Array.isArray(step.files) ? step.files : [step.files]).map(String);
    await page.locator(typeof step.upload === "string" && step.upload.startsWith("css:") ? step.upload.slice(4) : 'input[type="file"]').first().setInputFiles(files, { timeout });
    return settle(page, w);
  }
  if (step.press) {
    if (step.target) await locate(page, step.target).press(step.press, { timeout });
    else await page.keyboard.press(step.press);
    return settle(page, w);
  }
  if (step.wait !== undefined) {
    if (typeof step.wait === "number") return page.waitForTimeout(step.wait);
    return locate(page, step.wait).waitFor({ state: step.state ?? "visible", timeout });
  }
  if (step.expect) {
    const loc = locate(page, step.expect);
    if (step.visible === false || step.count === 0) {
      // filter({visible}) means "no visible match" is the hidden state.
      const deadline = Date.now() + timeout;
      while ((await loc.count()) > 0) {
        if (Date.now() > deadline) throw new Error(`expected ${JSON.stringify(step.expect)} to be absent or hidden`);
        await page.waitForTimeout(100);
      }
      return;
    }
    await loc.waitFor({ state: "visible", timeout });
    if (step.text !== undefined) {
      const deadline = Date.now() + timeout;
      let got = "";
      while (Date.now() < deadline) {
        got = await loc.innerText();
        if (got.includes(step.text)) break;
        await page.waitForTimeout(100);
      }
      if (!got.includes(step.text)) throw new Error(`expected ${JSON.stringify(step.expect)} to contain "${step.text}", got "${got.slice(0, 300)}"`);
    }
    if (step.value !== undefined) {
      const got = await loc.inputValue();
      if (got !== step.value) throw new Error(`expected value "${step.value}", got "${got}"`);
    }
    if (step.enabled !== undefined) {
      const got = await loc.isEnabled();
      if (got !== step.enabled) throw new Error(`expected ${JSON.stringify(step.expect)} to be ${step.enabled ? "enabled" : "disabled"}`);
    }
    if (step.count !== undefined) {
      // The first match being visible says nothing of the rest: a remounted
      // image has no size until it decodes. Poll like "text" does.
      const all = locateAll(page, step.expect);
      const deadline = Date.now() + timeout;
      let n = await all.count();
      while (n !== step.count && Date.now() < deadline) {
        await page.waitForTimeout(100);
        n = await all.count();
      }
      if (n !== step.count) throw new Error(`expected ${step.count} visible ${JSON.stringify(step.expect)}, found ${n}`);
    }
    if (step.inViewport) {
      const box = await loc.boundingBox();
      const vp = page.viewportSize()!;
      if (!box || box.x < 0 || box.y < 0 || box.x + box.width > vp.width + 1 || box.y + box.height > vp.height + 1)
        throw new Error(`expected ${JSON.stringify(step.expect)} inside the ${vp.width}x${vp.height} viewport, box ${JSON.stringify(box)}`);
    }
    return;
  }
  if (step.shot) {
    await settle(page, w);
    ctx.captures.push(await snap(page, ctx.dir, String(step.shot), ctx.vp, ctx.problems));
    return;
  }
  if (step.aria !== undefined) {
    const target = step.aria === true || step.aria === "" ? page.locator("body") : locate(page, step.aria);
    ctx.notes.push({ step: ctx.idx, aria: await target.ariaSnapshot() });
    return;
  }
  if (step.eval) {
    const value = await page.evaluate(step.eval);
    ctx.notes.push({ step: ctx.idx, eval: step.eval, value });
    if (step.equals !== undefined && JSON.stringify(value) !== JSON.stringify(step.equals))
      throw new Error(`eval returned ${JSON.stringify(value)}, expected ${JSON.stringify(step.equals)}`);
    return;
  }
  if (step.reload) {
    await page.reload({ waitUntil: "domcontentloaded" });
    await page.locator(".sidebar").waitFor({ state: "attached", timeout });
    return settle(page, w);
  }
  throw new Usage(`unknown step ${JSON.stringify(step)}: goto click hover tap fill select type press drag wait expect upload shot aria eval reload`);
}

/** Scripted interaction. On failure: a screenshot, an accessibility snapshot and the reason. */
export async function drive(a: Args): Promise<Result> {
  const name = str(a, "iso", DEFAULT_ISO)!;
  const iso = loadIso(name);
  const vars = loadVars(name);
  const route = substitute(a.pos[0] ?? "new", vars);
  const stepsText = str(a, "steps");
  if (!stepsText) throw new Usage("drive needs --steps '<json array>' (or --steps @file.json)");
  const raw = stepsText.startsWith("@") ? readFileSync(stepsText.slice(1), "utf8") : stepsText;
  const steps: Step[] = parseJsonArg(substitute(raw, vars), "--steps");
  if (!Array.isArray(steps)) throw new Usage("--steps must be a JSON array");
  const dir = outDir(a, "drives");
  const problems: NonNullable<Result["problems"]> = [];
  const runs: Record<string, unknown>[] = [];
  const allowErrors = !!a.flags["allow-errors"];
  await withBrowser(async (browser) => {
    for (const vp of viewports(a)) {
      const v = VIEWPORTS[vp];
      const ctx: BrowserContext = await browser.newContext({
        viewport: { width: v.width, height: v.height },
        deviceScaleFactor: 2,
        isMobile: v.isMobile,
        hasTouch: v.hasTouch,
        ...(a.flags.video ? { recordVideo: { dir, size: { width: v.width, height: v.height } } } : {}),
      });
      await isolate(ctx, iso);
      const page = await ctx.newPage();
      const w = watch(page, iso.url);
      const st = { dir, vp, captures: [] as Capture[], notes: [] as any[], problems, idx: -1 };
      const t0 = Date.now();
      let failed: Record<string, unknown> | null = null;
      try {
        await goRoute(page, iso, route, w);
        for (let i = 0; i < steps.length; i++) {
          st.idx = i;
          try {
            await runStep(page, iso, steps[i], w, st);
          } catch (e: any) {
            if (e instanceof Usage) throw e;
            const shotFile = join(dir, `fail-${vp}-step${i}.png`);
            await page.screenshot({ path: shotFile }).catch(() => {});
            const aria = await page.locator("body").ariaSnapshot().catch(() => "");
            failed = { step: i, action: steps[i], reason: String(e?.message ?? e).split("\n")[0].slice(0, 500), screenshot: shotFile, aria: aria.slice(0, 6000) };
            problems.push({ code: "step-failed", msg: `${vp}: step ${i} ${JSON.stringify(steps[i])}: ${failed.reason}`, hint: `see ${shotFile} and the aria snapshot` });
            break;
          }
        }
        if (dirty(w) && !allowErrors) problems.push({ code: "errors", msg: `${vp}: ${w.consoleErrors.length} console, ${w.pageErrors.length} page, ${w.failedApi.length} API, ${w.httpFailures.length} HTTP errors` });
      } catch (e) {
        if (e instanceof Usage) throw e;
        failed = failed ?? { step: st.idx, reason: String((e as any)?.message ?? e).slice(0, 500) };
        problems.push({ code: "drive-failed", msg: `${vp}: ${failed.reason}` });
      } finally {
        const video = page.video();
        await ctx.close();
        let videoFile: string | null = null;
        if (video) {
          const webm = join(dir, `drive-${vp}.webm`);
          renameSync(await video.path(), webm);
          videoFile = toMp4(webm);
        }
        runs.push({ viewport: vp, ms: Date.now() - t0, steps: steps.length, failed, captures: st.captures, notes: st.notes, video: videoFile, ...summary(w) });
      }
    }
  }, !!a.flags["show-scrollbars"]);
  const result: Result = { ok: problems.length === 0, route, build: iso.build, dir, runs, problems };
  writeFileSync(join(dir, "drive.json"), JSON.stringify(result, null, 2));
  return result;
}

/** mp4 when ffmpeg is installed (GitHub plays it inline); otherwise the webm. */
function toMp4(webm: string): string {
  const mp4 = webm.replace(/\.webm$/, ".mp4");
  const r = run(["ffmpeg", "-y", "-loglevel", "error", "-i", webm, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-movflags", "+faststart", mp4]);
  return r.code === 0 && existsSync(mp4) ? mp4 : webm;
}
