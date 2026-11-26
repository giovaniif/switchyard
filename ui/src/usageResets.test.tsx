// Every limit window an engine reports, with when it resets, for every engine
// and account: the owner asked to see "the resets the harnesses give us, like
// the full reset, weekly reset, etc" without digging for them.
import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import * as U from "./usage";
import * as M from "./UsageMeters";
import type { SubscriptionSource } from "./UsageMeters";

// Read through the modules so each test fails on its own claim, not on a
// missing export, before the change.
const { metersFor, noLimitsNote, reportsLimits } = U;
const fullResets = (...a: Parameters<typeof U.fullResets>) => U.fullResets(...a);
const { Subscriptions, UsageStrip, subscriptionRows } = M;
const OtherAccountRow = (p: Parameters<typeof M.OtherAccountRow>[0]) => M.OtherAccountRow(p);

const MIN = 60_000;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;
const at = (ms: number) => Math.floor(ms / 1000);

const claude = (now: number) => ({
  windows: {
    five_hour: { utilization: 0.91, resetsAt: at(now + HOUR + 35 * MIN + 30_000) },
    seven_day: { utilization: 0.51, resetsAt: at(now + 2 * DAY + 3 * HOUR + 30_000) },
    seven_day_fable: { utilization: 0.04, resetsAt: at(now + 2 * DAY + 3 * HOUR + 30_000) },
  },
});

const codex = (now: number) => ({
  limitId: "codex",
  primary: { usedPercent: 52, windowDurationMins: 10080, resetsAt: at(now + 4 * DAY + 30_000) },
  secondary: null,
  byLimitId: {
    codex: { limitId: "codex", primary: { usedPercent: 52, windowDurationMins: 10080, resetsAt: at(now + 4 * DAY) } },
    codex_spark: { limitId: "codex_spark", limitName: "GPT-Spark", primary: { usedPercent: 7, windowDurationMins: 300, resetsAt: at(now + 2 * HOUR) } },
  },
  resetCredits: {
    availableCount: 3,
    credits: [
      { id: "a", resetType: "codexRateLimits", status: "available", expiresAt: at(now + 13 * DAY), title: "Full reset" },
      { id: "b", resetType: "codexRateLimits", status: "available", expiresAt: at(now + 24 * DAY), title: "Full reset" },
      { id: "c", resetType: "codexRateLimits", status: "available", expiresAt: at(now + 30 * DAY), title: "Full reset" },
      { id: "d", resetType: "codexRateLimits", status: "used", expiresAt: at(now + 30 * DAY), title: "Full reset" },
    ],
  },
});

const source = (now: number): SubscriptionSource => ({
  host: "local",
  usage: { claude: claude(now), codex: codex(now) } as any,
  accounts: [],
  engines: { claude: { installed: true, loggedIn: true }, codex: { installed: true, loggedIn: true }, opencode: { installed: true } } as any,
});

describe("reset times at a glance", () => {
  test("the sidebar's Usage row says when the window it shows resets", () => {
    const html = renderToStaticMarkup(<UsageStrip sources={[source(Date.now())]} onManageAccounts={() => {}} onManage={() => {}} onRefresh={async () => ({})} />);
    expect(html).toContain("91% used");
    expect(html).toContain("5-hour resets in 1 h 35 min");
  });

  test("another account lists every window with its reset, not only the first one's percent", () => {
    const now = Date.now();
    const row = subscriptionRows([source(now)], now).find((r) => r.engine === "claude")!;
    const html = renderToStaticMarkup(<OtherAccountRow row={row} now={now} onSignIn={() => {}} />);
    expect(html).toContain("91% of 5-hour");
    expect(html).toContain("51% of Week");
    expect(html).toContain("4% of Week · Fable");
    expect(html).toContain("in 1 h 35 min");
    expect(html).toContain("in 2 d 3 h");
  });

  test("Subscriptions gives each reset as a time and as a countdown", () => {
    const html = renderToStaticMarkup(<Subscriptions sources={[source(Date.now())]} onManage={() => {}} />);
    expect(html).toMatch(/Resets [^<]*\d:\d\d[^<]* · in 1 h 35 min/);
    expect(html).toContain("in 2 d 3 h");
  });
});

describe("Codex full resets and per-model limits", () => {
  test("available full-reset credits are counted, with the soonest expiry", () => {
    const now = Date.now();
    const r = fullResets("codex", { codex: codex(now) } as any, now);
    expect(r?.count).toBe(3);
    expect(r?.title).toBe("Full reset");
    expect(r?.expiresAt).toBe(at(now + 13 * DAY) * 1000);
  });

  test("Subscriptions shows them on the Codex account", () => {
    const html = renderToStaticMarkup(<Subscriptions sources={[source(Date.now())]} onManage={() => {}} />);
    expect(html).toContain("3 full resets available");
  });

  test("a second Codex limit (another model's) is a meter of its own, the main one is not repeated", () => {
    const now = Date.now();
    const meters = metersFor("codex", { codex: codex(now) } as any, now);
    expect(meters.map((m) => m.label)).toEqual(["Week", "5-hour · GPT-Spark"]);
  });
});

describe("Claude windows beyond the known three", () => {
  test("a labelled window from the usage report is shown under its label", () => {
    const now = Date.now();
    const usage = { claude: { windows: { ...claude(now).windows, weekly_design: { utilization: 0.2, resetsAt: at(now + DAY), label: "Week · Design" } } } };
    expect(metersFor("claude", usage as any, now).map((m) => m.label)).toContain("Week · Design");
  });

  test("an unlabelled window that is not a limit stays out", () => {
    const now = Date.now();
    const usage = { claude: { windows: { five_hour: { utilization: 0.1, resetsAt: null }, overage: { utilization: 0.3, resetsAt: null } } } };
    expect(metersFor("claude", usage as any, now).map((m) => m.key)).toEqual(["five_hour"]);
  });
});

describe("Grok and the engines without limits", () => {
  test("Grok's credit allowance reads as a meter with its reset", () => {
    const now = Date.now();
    expect(reportsLimits("grok")).toBe(true);
    const meters = metersFor("grok", { grok: { windows: { week: { utilization: 0.3, resetsAt: at(now + 3 * DAY), label: "Week" } } } } as any, now);
    expect(meters).toHaveLength(1);
    expect(meters[0].percent).toBe(30);
    expect(meters[0].resetsAt).toBe(at(now + 3 * DAY) * 1000);
  });

  test("OpenCode and Gemini are named as not reporting limits", () => {
    expect(reportsLimits("opencode")).toBe(false);
    expect(noLimitsNote()).toBe("OpenCode and Gemini don't report subscription limits.");
  });
});
