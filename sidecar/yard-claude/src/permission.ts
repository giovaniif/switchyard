/** yardd's permission modes, as the SDK names them. */
export const PERMISSION_MODE: Record<string, string> = {
  ask: "default",
  acceptEdits: "acceptEdits",
  plan: "plan",
  full: "bypassPermissions",
};

/** The SDK mode to switch to before a turn yardd sent with `permission`, or
 *  null to leave it. yardd names every turn's mode: a turn on a watched Slack
 *  channel's notice runs in Plan, the next goes back to the session's. Only
 *  the first turn may skip the switch, when `start` already set that mode;
 *  later ones always switch, since an approved plan changes the mode inside
 *  Claude Code without telling us. */
export function modeFor(wanted: unknown, startMode: string, firstTurn: boolean): string | null {
  if (typeof wanted !== "string" || !wanted) return null;
  if (firstTurn && wanted === startMode) return null;
  return PERMISSION_MODE[wanted] ?? "default";
}
