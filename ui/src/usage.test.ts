import { describe, expect, test } from "bun:test";
import { formatReset, levelOf, metersFor, noLimitsNote, percentText, stripGroups } from "./usage";

const NOW = 1_800_000_000_000;
const S = 1000;
const MIN = 60 * S;
const HOUR = 60 * MIN;
const DAY = 24 * HOUR;

describe("percentText", () => {
  test("rounds and stays inside 0-100", () => {
    expect(percentText(12.4)).toBe("12%");
    expect(percentText(46.6)).toBe("47%");
    expect(percentText(-3)).toBe("0%");
    expect(percentText(140)).toBe("100%");
  });
});

describe("levelOf", () => {
  test("amber above 70, red above 90, exactly on the line stays below it", () => {
    expect(levelOf(0)).toBe("ok");
    expect(levelOf(70)).toBe("ok");
    expect(levelOf(70.5)).toBe("warn");
    expect(levelOf(90)).toBe("warn");
    expect(levelOf(90.5)).toBe("high");
    expect(levelOf(100)).toBe("high");
  });
});

describe("formatReset", () => {
  test("hours and minutes", () => {
    expect(formatReset(NOW + 2 * HOUR + 10 * MIN + 30 * S, NOW)).toBe("resets in 2 h 10 min");
    expect(formatReset(NOW + 3 * HOUR, NOW)).toBe("resets in 3 h");
  });
  test("under an hour is minutes, under a minute says so", () => {
    expect(formatReset(NOW + 12 * MIN + 5 * S, NOW)).toBe("resets in 12 min");
    expect(formatReset(NOW + 20 * S, NOW)).toBe("resets in under a minute");
  });
  test("a day or more is days and hours", () => {
    expect(formatReset(NOW + 3 * DAY + 4 * HOUR + 30 * S, NOW)).toBe("resets in 3 d 4 h");
    expect(formatReset(NOW + 2 * DAY, NOW)).toBe("resets in 2 d");
  });
  test("a reset time in the past, or none at all", () => {
    expect(formatReset(NOW - MIN, NOW)).toBe("resets now");
    expect(formatReset(null, NOW)).toBe("");
  });
});

describe("metersFor", () => {
  test("Codex: primary is the 5-hour window, secondary the week, with reset times", () => {
    const at = NOW / 1000 + 7830;
    const meters = metersFor("codex", {
      codex: {
        primary: { usedPercent: 12, windowDurationMins: 300, resetsAt: at },
        secondary: { usedPercent: 34, windowDurationMins: 10080, resetsAt: at + 3 * 86400 },
      },
    });
    expect(meters.map((m) => [m.label, m.percent])).toEqual([
      ["5-hour", 12],
      ["Week", 34],
    ]);
    expect(meters[0].resetsAt).toBe(at * 1000);
  });

  test("a window whose reset time has passed reads as 0%, with no reset time", () => {
    const past = NOW / 1000 - 60;
    const later = NOW / 1000 + 3 * 86400;
    const codex = metersFor(
      "codex",
      {
        codex: {
          primary: { usedPercent: 95, windowDurationMins: 300, resetsAt: past },
          secondary: { usedPercent: 34, windowDurationMins: 10080, resetsAt: later },
        },
      },
      NOW,
    );
    expect(codex.map((m) => [m.label, m.percent, m.level, m.resetsAt])).toEqual([
      ["5-hour", 0, "ok", null],
      ["Week", 34, "ok", later * 1000],
    ]);
    const claude = metersFor("claude", { claude: { windows: { five_hour: { utilization: 0.91, resetsAt: past } } } }, NOW);
    expect(claude.map((m) => [m.label, m.percent, m.level, m.resetsAt])).toEqual([["5-hour", 0, "ok", null]]);
  });

  test("Codex: a lone weekly window is labelled by its length, not by its slot", () => {
    const meters = metersFor("codex", { codex: { primary: { usedPercent: 60, windowDurationMins: 10080, resetsAt: null }, secondary: null } });
    expect(meters.map((m) => m.label)).toEqual(["Week"]);
    expect(meters[0].resetsAt).toBeNull();
  });

  test("Codex: nothing known gives no meters", () => {
    expect(metersFor("codex", {})).toEqual([]);
    expect(metersFor("codex", { codex: { primary: null } })).toEqual([]);
  });

  test("Claude: utilization is a fraction; windows come in a fixed order with per-model weeks last, by name", () => {
    const meters = metersFor("claude", {
      claude: {
        status: "allowed",
        windows: {
          seven_day_sonnet: { utilization: 0.08, resetsAt: 1_800_400_000 },
          seven_day: { utilization: 0.17, resetsAt: 1_800_300_000 },
          five_hour: { utilization: 0.47, resetsAt: 1_800_010_000 },
          seven_day_opus: { utilization: 0.5, resetsAt: null },
        },
      },
    });
    expect(meters.map((m) => [m.label, m.percent])).toEqual([
      ["5-hour", 47],
      ["Week", 17],
      ["Week · Opus", 50],
      ["Week · Sonnet", 8],
    ]);
    expect(meters[0].resetsAt).toBe(1_800_010_000_000);
  });

  test("Claude: a window with no utilization is not a meter; overage windows are not shown", () => {
    const meters = metersFor("claude", {
      claude: { windows: { five_hour: { utilization: null, resetsAt: 1 }, overage: { utilization: 0.9, resetsAt: 1 }, seven_day: { utilization: 0.2, resetsAt: 1 } } },
    });
    expect(meters.map((m) => m.label)).toEqual(["Week"]);
  });

  test("Claude: the older shape (fiveHour, sevenDay) still reads", () => {
    const meters = metersFor("claude", { claude: { fiveHour: 0.47, sevenDay: 0.17 } });
    expect(meters.map((m) => [m.label, m.percent])).toEqual([
      ["5-hour", 47],
      ["Week", 17],
    ]);
  });

  test("meters carry their threshold level", () => {
    const meters = metersFor("claude", { claude: { windows: { five_hour: { utilization: 0.95, resetsAt: null }, seven_day: { utilization: 0.75, resetsAt: null } } } });
    expect(meters.map((m) => m.level)).toEqual(["high", "warn"]);
  });
});

describe("stripGroups", () => {
  const at = (ms: number) => Math.floor(ms / 1000);
  const claude = (five: number, fiveReset: number | null, week = 0.2) => ({
    windows: {
      five_hour: { utilization: five, resetsAt: fiveReset == null ? null : at(fiveReset) },
      seven_day: { utilization: week, resetsAt: at(NOW + 3 * DAY) },
    },
  });
  const codex = (used: number, reset: number | null) => ({
    primary: { usedPercent: used, windowDurationMins: 300, resetsAt: reset == null ? null : at(reset) },
    secondary: { usedPercent: 30, windowDurationMins: 10080, resetsAt: at(NOW + 3 * DAY) },
  });
  const BOTH = ["claude", "codex"] as ("claude" | "codex")[];
  const pick = (groups: ReturnType<typeof stripGroups>) => groups.map((g) => `${g.engine}/${g.account ?? "-"}@${g.host}`);

  test("a host with no Claude data does not hide another's: the remote's Claude limits show", () => {
    const groups = stripGroups(
      [
        { host: "local", usage: { codex: codex(12, NOW + HOUR) }, accounts: [] },
        { host: "server", usage: { claude: claude(0.47, NOW + HOUR) }, accounts: [] },
      ],
      BOTH,
    );
    expect(pick(groups)).toEqual(["claude/-@server", "codex/-@local"]);
    expect(metersFor("claude", groups[0].usage).map((m) => m.percent)).toEqual([47, 20]);
  });

  test("the same engine on two hosts shows once, from the one that reported last (later reset time)", () => {
    const groups = stripGroups(
      [
        { host: "local", usage: { claude: claude(0.9, NOW - HOUR), codex: codex(40, NOW + 2 * HOUR) }, accounts: [] },
        { host: "server", usage: { claude: claude(0.1, NOW + 4 * HOUR), codex: codex(10, NOW + HOUR) }, accounts: [] },
      ],
      BOTH,
    );
    expect(pick(groups)).toEqual(["claude/-@server", "codex/-@local"]);
  });

  test("same reset times: more used is the later report, since use only grows inside a window", () => {
    const groups = stripGroups(
      [
        { host: "local", usage: { claude: claude(0.3, NOW + HOUR) }, accounts: [] },
        { host: "server", usage: { claude: claude(0.5, NOW + HOUR) }, accounts: [] },
      ],
      ["claude"],
    );
    expect(pick(groups)).toEqual(["claude/-@server"]);
  });

  test("no reset times to compare: the first host listed wins", () => {
    const groups = stripGroups(
      [
        { host: "local", usage: { claude: { fiveHour: 0.2, sevenDay: 0.1 } }, accounts: [] },
        { host: "server", usage: { claude: { fiveHour: 0.6, sevenDay: 0.3 } }, accounts: [] },
      ],
      ["claude"],
    );
    expect(pick(groups)).toEqual(["claude/-@local"]);
  });

  test("added accounts get their own rows, labelled with their name, after Default", () => {
    const groups = stripGroups(
      [
        { host: "local", usage: { codex: codex(12, NOW + HOUR) }, accounts: [] },
        {
          host: "server",
          usage: { claude: claude(0.47, NOW + HOUR), accounts: { a1: claude(0.91, NOW + HOUR), a2: codex(77, NOW + HOUR) } },
          accounts: [
            { id: "a1", engine: "claude", name: "Work" },
            { id: "a2", engine: "codex", name: "Team" },
          ],
        },
      ],
      BOTH,
    );
    expect(pick(groups)).toEqual(["claude/-@server", "claude/Work@server", "codex/-@local", "codex/Team@server"]);
    expect(metersFor("claude", groups[1].usage)[0].percent).toBe(91);
    expect(metersFor("codex", groups[3].usage)[0].percent).toBe(77);
  });

  test("an account is matched across hosts by engine and name, not by its per-host id", () => {
    const groups = stripGroups(
      [
        { host: "local", usage: { accounts: { x: claude(0.2, NOW + HOUR) } }, accounts: [{ id: "x", engine: "claude", name: "work" }] },
        { host: "server", usage: { accounts: { y: claude(0.6, NOW + 3 * HOUR) } }, accounts: [{ id: "y", engine: "claude", name: "Work" }] },
      ],
      ["claude"],
    );
    // One Work, from the later report; Default is listed beside it, with nothing reported.
    expect(pick(groups)).toEqual(["claude/-@local", "claude/Work@server"]);
    expect(groups[0].reported).toBe(false);
  });

  test("an account the host no longer lists shows nothing; one with no data yet is listed as not reported", () => {
    const groups = stripGroups(
      [{ host: "local", usage: { accounts: { gone: claude(0.5, NOW + HOUR) } }, accounts: [{ id: "a1", engine: "claude", name: "Work" }] }],
      ["claude"],
    );
    // Default is listed next to it, also as not reported.
    expect(groups.map((g) => [g.engine, g.account, g.reported])).toEqual([
      ["claude", null, false],
      ["claude", "Work", false],
    ]);
    expect(metersFor("claude", groups[1].usage)).toEqual([]);
  });

  test("no hosts shown, no rows", () => {
    expect(stripGroups([], BOTH)).toEqual([]);
  });
});


test("OpenCode and Gemini accounts add no plan-usage rows; a Grok account is listed like Claude's", () => {
  // Grok reports its credit allowance (`_x.ai/billing`); the other two nothing.
  expect(stripGroups([{ host: "local", usage: {}, accounts: [
    { id: "g", engine: "gemini", name: "Work" },
    { id: "o", engine: "opencode", name: "Work" },
    { id: "x", engine: "grok", name: "Work" },
  ] }], ["claude", "codex", "opencode", "grok", "gemini"]).map((g) => `${g.engine}:${g.account ?? "Default"}:${g.reported}`)).toEqual(["grok:Default:false", "grok:Work:false"]);
});

test("the Subscriptions note names every engine without plan limits, Gemini included", () => {
  expect(noLimitsNote()).toBe("OpenCode and Gemini don't report subscription limits.");
  expect(noLimitsNote(["claude", "codex", "gemini"])).toBe("Gemini doesn't report subscription limits.");
  expect(noLimitsNote(["claude", "codex"])).toBe("");
});
