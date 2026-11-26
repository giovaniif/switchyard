import { describe, expect, test } from "bun:test";
import { leadRow, subscriptionRows, triggerMeter, type SubscriptionSource } from "./UsageMeters";

const NOW = 1_800_000_000_000;
const at = (ms: number) => Math.floor(ms / 1000);
const claude = (five: number) => ({
  windows: {
    five_hour: { utilization: five, resetsAt: at(NOW + 3_600_000) },
    seven_day: { utilization: 0.17, resetsAt: at(NOW + 3 * 86_400_000) },
  },
});
const source = (loggedIn: boolean | null, usage = claude(0.47)): SubscriptionSource => ({
  host: "local",
  usage: { claude: usage },
  accounts: [],
  engines: {
    claude: { installed: true, loggedIn },
    codex: { installed: true, loggedIn },
  },
});

describe("triggerMeter", () => {
  test("shows the fullest meter of a signed-in account", () => {
    expect(triggerMeter(subscriptionRows([source(true)], NOW))?.percent).toBe(47);
  });

  test("leaves out an account that needs sign-in, as the popover does", () => {
    // Meters outlive a lost sign-in; the popover hides them ("Sign in to see usage"),
    // so the closed trigger must not show their percent either.
    expect(triggerMeter(subscriptionRows([source(false)], NOW))).toBeUndefined();
  });

  test("a login state not read yet still shows the last report", () => {
    expect(triggerMeter(subscriptionRows([source(null)], NOW))?.percent).toBe(47);
  });
});

describe("leadRow", () => {
  const work = (five: number, loggedIn: boolean | null = true): SubscriptionSource => ({
    host: "local",
    usage: { claude: undefined, accounts: { acct_work: claude(five) } },
    accounts: [{ id: "acct_work", engine: "claude", name: "TEST-Work" }],
    engines: {
      claude: { installed: true, loggedIn: true },
      codex: { installed: true, loggedIn: true },
      accounts: { acct_work: { installed: true, loggedIn } },
    },
  });

  test("with nothing selected, leads with the account whose meter the trigger shows", () => {
    // Default has no report and sorts first; the trigger reads TEST-Work's 91%,
    // so the popover must open on TEST-Work, not on "No limits reported yet".
    const rows = subscriptionRows([work(0.91)], NOW);
    expect(rows[0].account).toBeNull();
    expect(triggerMeter(rows)?.percent).toBe(91);
    expect(leadRow(rows)?.account).toBe("TEST-Work");
  });

  test("falls back to the first row when no account reports limits", () => {
    const rows = subscriptionRows([{ ...source(true), usage: {} }], NOW);
    expect(triggerMeter(rows)).toBeUndefined();
    expect(leadRow(rows)).toBe(rows[0]);
  });

  test("skips the stale report of an account that needs sign-in, as the trigger does", () => {
    const rows = subscriptionRows([work(0.91, false)], NOW);
    expect(triggerMeter(rows)).toBeUndefined();
    expect(leadRow(rows)).toBe(rows[0]);
  });
});
