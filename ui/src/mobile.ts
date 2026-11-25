export interface PhoneDevice {
  name: string;
  /** When its app last connected or left, yardd's clock in ms. */
  lastSeen: number;
}

/** A host's phones, from yardd's `mobile` (snapshot) and `mobile.status`. */
export interface MobileStatus {
  configured: boolean;
  testFlightUrl?: string | null;
  /** Phones that connected, most recent first. Missing from an older yardd. */
  devices?: PhoneDevice[];
  /** Phones whose app is connected now. */
  live?: string[];
  /** The host's HTTPS addresses a phone can open (its YARD_ALLOW_ORIGIN). */
  origins?: string[];
}

/** One place a phone can connect to, for the setup card's QR code. */
export interface PhoneTarget {
  hostId: string;
  hostName: string;
  origin: string;
  status: MobileStatus;
}

/** Where a phone can connect: the page's own address when a phone can open
 *  it (the web page), then each host's HTTPS addresses. The desktop app's page
 *  is on loopback, so there it is the hosts' addresses alone. */
export function phoneTargets(hosts: { id: string; name: string; mobile?: MobileStatus }[], pageOrigin: string): PhoneTarget[] {
  const out: PhoneTarget[] = [];
  const add = (hostId: string, hostName: string, origin: string | null, status: MobileStatus) => {
    if (origin && !out.some((t) => t.origin === origin)) out.push({ hostId, hostName, origin, status });
  };
  const local = hosts.find((h) => h.id === "local");
  if (local) add("local", local.name, mobileHost(pageOrigin), local.mobile ?? { configured: false });
  for (const h of hosts) for (const o of h.mobile?.origins ?? []) add(h.id, h.name, mobileHost(o), h.mobile ?? { configured: false });
  return out;
}

/** What the card says about a host's phones. */
export function phoneState(status: MobileStatus | undefined, now: number): { kind: "live" | "seen" | "never"; text: string } {
  const live = status?.live ?? [];
  if (live.length) return { kind: "live", text: `${live[0]}${live.length > 1 ? ` and ${live.length - 1} more` : ""} connected now` };
  const last = status?.devices?.[0];
  if (last) return { kind: "seen", text: `${last.name} · last seen ${ago(now - last.lastSeen)}` };
  if (status?.configured) return { kind: "seen", text: "An iPhone has connected" };
  return { kind: "never", text: "No phone connected yet" };
}

function ago(ms: number): string {
  const m = Math.floor(ms / 60_000);
  if (m < 1) return "just now";
  if (m < 60) return `${m} min ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h} h ago`;
  const d = Math.floor(h / 24);
  return `${d} day${d === 1 ? "" : "s"} ago`;
}

/** Phones cannot reach the desktop's loopback, and setup links carry no credentials. */
export function mobileHost(value: string): string | null {
  try {
    const url = new URL(value.trim());
    const host = url.hostname.toLowerCase();
    if (url.protocol !== "https:" || url.username || url.password || url.pathname !== "/" || url.search || url.hash) return null;
    if (host === "localhost" || host.endsWith(".localhost") || host === "[::1]" || host === "0.0.0.0" || /^127\./.test(host)) return null;
    return url.origin;
  } catch {
    return null;
  }
}

export function mobileLink(value: string): string | null {
  const host = mobileHost(value);
  return host ? `switchyard://connect?host=${encodeURIComponent(host)}` : null;
}

export function testFlightLink(value?: string): string | null {
  return value && /^https:\/\/testflight\.apple\.com\/join\/[A-Za-z0-9]{8}$/.test(value) ? value : null;
}

export function inMobileApp(): boolean {
  return typeof window !== "undefined" && (window as any).SwitchyardMobile?.version === 1;
}
