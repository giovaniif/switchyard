import { expect, test } from "bun:test";
import { mobileHost, mobileLink, phonePage, phonePageLink, phoneState, phoneTargets, testFlightLink } from "./mobile";
import { initial, reduce } from "./store";

test("phone setup accepts only private-network HTTPS page origins", () => {
  expect(mobileHost(" https://yard.example.ts.net/ ")).toBe("https://yard.example.ts.net");
  for (const value of ["http://yard.test", "https://localhost", "https://127.0.0.1", "https://[::1]", "https://user:pass@yard.test", "https://yard.test/path", "https://yard.test/?token=x"]) {
    expect(mobileHost(value)).toBeNull();
  }
  const link = new URL(mobileLink("https://yard.test:8443")!);
  expect(link.protocol).toBe("switchyard:");
  expect(link.searchParams.get("host")).toBe("https://yard.test:8443");
  expect(mobileLink("http://localhost:47717")).toBeNull();
});

test("only real TestFlight invitations become install links", () => {
  expect(testFlightLink("https://testflight.apple.com/join/AbCd1234")).toBe("https://testflight.apple.com/join/AbCd1234");
  for (const value of [undefined, "javascript:alert(1)", "https://testflight.apple.com.evil/join/AbCd1234", "https://testflight.apple.com/", "https://testflight.apple.com/join/REPLACE_ME"]) expect(testFlightLink(value)).toBeNull();
});

test("setup state follows the hub, survives snapshots and ignores remote phones", () => {
  const connected = reduce(initial, { t: "server", msg: { op: "mobile.status", mobile: { configured: true } } });
  expect(connected.mobile.configured).toBe(true);
  expect(reduce(connected, { t: "server", msg: { op: "mobile.status", hostId: "remote", mobile: { configured: false } } }).mobile.configured).toBe(true);
  expect(reduce(initial, { t: "server", msg: { op: "snapshot", sessions: [], mobile: { configured: true } } }).mobile.configured).toBe(true);
  expect(reduce(connected, { t: "server", msg: { op: "snapshot", sessions: [] } }).mobile.configured).toBe(false);
});

test("the desktop app offers the hosts' own HTTPS addresses, not its loopback page", () => {
  const hosts = [
    { id: "local", name: "This Mac", mobile: { configured: false } },
    { id: "h1", name: "server", mobile: { configured: true, origins: ["https://switchyard.example.ts.net", "http://100.64.0.1:47717"] } },
  ];
  const targets = phoneTargets(hosts, "http://127.0.0.1:47717");
  expect(targets.map((t) => [t.hostId, t.origin])).toEqual([["h1", "https://switchyard.example.ts.net"]]);
});

test("the web page offers its own address first, once", () => {
  const hosts = [{ id: "local", name: "server", mobile: { configured: false, origins: ["https://yard.test"] } }];
  expect(phoneTargets(hosts, "https://yard.test").map((t) => t.origin)).toEqual(["https://yard.test"]);
  expect(phoneTargets([], "https://yard.test")).toEqual([]);
});

test("a host's phones read as connected now, last seen, or none yet", () => {
  const now = 10 * 60 * 60_000;
  expect(phoneState({ configured: true, live: ["Ana's iPhone"], devices: [{ name: "Ana's iPhone", lastSeen: now }] }, now)).toEqual({ kind: "live", text: "Ana's iPhone connected now" });
  expect(phoneState({ configured: true, live: [], devices: [{ name: "iPhone", lastSeen: now - 3 * 60 * 60_000 }] }, now).text).toBe("iPhone · last seen 3 h ago");
  // An older yardd only knows that one connected.
  expect(phoneState({ configured: true }, now)).toEqual({ kind: "seen", text: "An iPhone has connected" });
  expect(phoneState(undefined, now).kind).toBe("never");
});

test("the QR code opens the host's page, which any iPhone Camera opens even without the app", () => {
  // A switchyard:// code gives the Camera nothing to offer until the app is installed.
  expect(phonePageLink("https://yard.test")).toBe("https://yard.test/?phone");
  const withInvite = phonePageLink("https://yard.test:8443", "https://testflight.apple.com/join/AbCd1234")!;
  expect(new URL(withInvite).searchParams.get("testflight")).toBe("https://testflight.apple.com/join/AbCd1234");
  expect(phonePageLink("https://yard.test", "https://evil.test/join/AbCd1234")).toBe("https://yard.test/?phone");
  expect(phonePageLink("http://localhost:47717")).toBeNull();

  expect(phonePage(withInvite)).toEqual({
    app: "switchyard://connect?host=https%3A%2F%2Fyard.test%3A8443",
    invite: "https://testflight.apple.com/join/AbCd1234",
    browser: "https://yard.test:8443/",
  });
  // A forged invitation in the address is dropped, and plain pages are not the phone page.
  expect(phonePage("https://yard.test/?phone&testflight=javascript:alert(1)")?.invite).toBeNull();
  expect(phonePage("https://yard.test/")).toBeNull();
  // Served over plain http there is nothing the app could connect to.
  expect(phonePage("http://100.64.0.1:47717/?phone")?.app).toBeNull();
});
