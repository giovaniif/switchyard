import { describe, expect, test } from "bun:test";
import { renderToStaticMarkup } from "react-dom/server";
import { Subscriptions, type SubscriptionSource } from "./UsageMeters";

const at = (ms: number) => Math.floor(ms / 1000);
const source = (loggedIn: boolean): SubscriptionSource => ({
  host: "local",
  usage: { claude: { windows: { five_hour: { utilization: 0.91, resetsAt: at(Date.now() + 3_600_000) } } } },
  accounts: [],
  engines: { claude: { installed: true, loggedIn } },
});

describe("Subscriptions page", () => {
  test("a signed-in account shows its usage", () => {
    const html = renderToStaticMarkup(<Subscriptions sources={[source(true)]} onManage={() => {}} />);
    expect(html).toContain("91% used");
  });

  test("an account that needs sign-in offers Sign in, not its last usage, as the popover does", () => {
    // The last report outlives a lost sign-in (#358); the popover and the trigger hide it.
    const html = renderToStaticMarkup(<Subscriptions sources={[source(false)]} onManage={() => {}} />);
    expect(html).toContain("Needs sign-in");
    expect(html).not.toContain("% used");
    expect(html).toContain("Sign in to see usage");
    expect(html).toContain('aria-label="Sign in — Claude · Default"');
  });
});
