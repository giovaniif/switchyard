// Plan usage from the SDK's rate_limit_event, kept apart from main.ts so it
// can be tested without starting the SDK.
//
// A rate_limit_event describes ONE window (`rateLimitType`: five_hour,
// seven_day, seven_day_opus, seven_day_sonnet, ...) with `utilization` (a
// fraction) and `resetsAt` (epoch seconds). Some CLI builds instead send all
// windows at once as `unifiedWindows`. Either way the sidecar folds events into
// one picture, so the five-hour event does not wipe out what the weekly one
// said, and yardd relays it as the `claude` usage.

export interface UsageWindow {
  /** Fraction of the window used, 0 to 1. */
  utilization: number;
  /** Epoch seconds. */
  resetsAt: number | null;
  /** The meter's name, for a window the UI has no name of its own for. */
  label?: string;
}

export interface ClaudeUsage {
  status?: string;
  windows: Record<string, UsageWindow>;
}

/** Epoch seconds from a number, or from the ISO string the usage endpoint uses. */
function seconds(v: unknown): number | null {
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (typeof v === "string") {
    const ms = Date.parse(v);
    if (!Number.isNaN(ms)) return ms / 1000;
  }
  return null;
}

function windowOf(raw: any): UsageWindow | null {
  const utilization = raw?.utilization;
  if (typeof utilization !== "number" || !Number.isFinite(utilization)) return null;
  return { utilization, resetsAt: seconds(raw.resetsAt ?? raw.resets_at) };
}

/** `prev` with the windows `info` (an SDKRateLimitInfo) reports laid over it. */
export function mergeUsage(prev: ClaudeUsage | null, info: any): ClaudeUsage {
  const windows: Record<string, UsageWindow> = { ...(prev?.windows ?? {}) };
  const unified = info?.unifiedWindows;
  if (unified && typeof unified === "object") {
    for (const [name, raw] of Object.entries(unified)) {
      const w = windowOf(raw);
      if (w) windows[name] = w;
    }
  }
  if (typeof info?.rateLimitType === "string") {
    const w = windowOf(info);
    if (w) windows[info.rateLimitType] = w;
  }
  const status = typeof info?.status === "string" ? info.status : prev?.status;
  return status === undefined ? { windows } : { status, windows };
}

const slug = (s: string) => s.trim().toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_|_$/g, "");
const GROUPS: Record<string, string> = { session: "Session", weekly: "Week", daily: "Day", monthly: "Monthly" };

/** The plan limits from the SDK's usage report (`rate_limits.limits`, what
 *  `/usage` shows), in the same shape as the rate_limit_event picture: the
 *  session window is `five_hour`, the overall week `seven_day`, a per-model
 *  week `seven_day_<model>`. Any other row the server sends (a surface's week,
 *  a kind it adds later) is kept under its kind with a label built from the
 *  server's group and scope names, so it shows without a release. The report
 *  needs no turn, so an account that has not run anything yet still has its
 *  limits. Null when it has none. */
export function fromUsageReport(report: any): ClaudeUsage | null {
  const limits = report?.rate_limits?.limits;
  if (!Array.isArray(limits)) return null;
  const windows: Record<string, UsageWindow> = {};
  for (const l of limits) {
    if (typeof l?.percent !== "number" || !Number.isFinite(l.percent)) continue;
    const model = l.scope?.model?.display_name ?? l.scope?.model?.id;
    const surface = l.scope?.surface?.display_name;
    let key: string | null = null;
    let label: string | undefined;
    if (l.kind === "session") key = "five_hour";
    else if (l.kind === "weekly_all") key = "seven_day";
    else if (l.kind === "weekly_scoped" && typeof model === "string") {
      // The family, as turn events name the window ("Opus 5.5" -> seven_day_opus),
      // so both sources write the same meter.
      const family = model.trim().split(/[\s-]+/)[0].toLowerCase().replace(/[^a-z0-9]+/g, "");
      if (family) key = `seven_day_${family}`;
    } else if (l.kind === "weekly_scoped" && typeof surface === "string" && slug(surface)) {
      key = `seven_day_${slug(surface)}`;
      label = `Week · ${surface.trim()}`;
    } else if (typeof l.kind === "string" && slug(l.kind)) {
      const scope = typeof model === "string" ? model : typeof surface === "string" ? surface : "";
      key = scope ? `${slug(l.kind)}_${slug(scope)}` : slug(l.kind);
      const words = (s: string) => s.replace(/_/g, " ").replace(/^./, (c) => c.toUpperCase());
      const group = typeof l.group === "string" && l.group ? GROUPS[l.group] ?? words(l.group) : words(l.kind);
      label = scope ? `${group} · ${scope.trim()}` : group;
    }
    if (!key || windows[key]) continue;
    windows[key] = { utilization: l.percent / 100, resetsAt: seconds(l.resets_at), ...(label ? { label } : {}) };
  }
  return Object.keys(windows).length ? { windows } : null;
}
