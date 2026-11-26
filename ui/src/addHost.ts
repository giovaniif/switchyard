/** yardd refuses a host name longer than this (`host.add` in remote.rs). */
export const HOST_NAME_LIMIT = 40;

/** The name a new SSH host gets when none is typed: `user@box.tailnet.ts.net`
 * → `box`. An IP address stays whole: its first part names nothing, and every
 * tailnet address starts with `100.`, so a second one would be refused as a
 * duplicate name. */
export function nameFromDestination(dest: string): string {
  const host = dest.trim().replace(/^.*@/, "");
  if (/^\d{1,3}(\.\d{1,3}){3}$/.test(host)) return host;
  if (host.includes(":") && /^\[?[0-9a-f:.]+\]?$/i.test(host)) return host.replace(/^\[|\]$/g, "");
  return host.split(".")[0] ?? "";
}
