// Overload episodes as yardd records them (crates/yardd/src/pressure.rs):
// the words for the banner, the stalled-turn note and the System tab's
// history. Pure, so they are tested without a browser.
import type { Host, PressureEpisode, PressureProc, PressureSession } from "./protocol";
import { size } from "./hosts";

export type Culprit = ({ kind: "session"; session: PressureSession } | { kind: "proc"; proc: PressureProc }) & { byCpu: boolean };

/** Who used the most at the episode's worst sample: by CPU when load was the
 *  trouble, by memory otherwise. Sessions and stray processes compete. */
export function culprit(ep: PressureEpisode): Culprit | null {
  const byCpu = ep.reasons.includes("load");
  const key = (u: { cpuCores: number; memBytes: number }) => (byCpu ? u.cpuCores : u.memBytes);
  const session = [...ep.top.sessions].sort((a, b) => key(b) - key(a))[0];
  const proc = [...ep.top.procs].sort((a, b) => key(b) - key(a))[0];
  if (session && (!proc || key(session) >= key(proc))) return { kind: "session", session, byCpu };
  if (proc) return { kind: "proc", proc, byCpu };
  return null;
}

/** `TEST-hog` or `node, pid 9`. */
export const culpritName = (c: Culprit) => (c.kind === "session" ? c.session.title : `${c.proc.name}, pid ${c.proc.pid}`);

/** `(8.0 cores, 9.0 GB)`. */
export function useText(u: { cpuCores: number; memBytes: number }): string {
  return `${u.cpuCores.toFixed(1)} cores, ${size(u.memBytes)}`;
}

/** What is wrong right now, in the episode's own terms. */
export function pressureMetric(host: Host, ep: PressureEpisode): string {
  if (ep.reasons.includes("load")) return `load ${Math.round(host.load)}`;
  // yardd lists swap only beside memory: full swap alone is no overload.
  if (ep.reasons.includes("memory")) {
    const free = `${size(Math.max(0, host.memTotal - host.memUsed))} free`;
    return ep.reasons.includes("swap") && host.swapTotal ? `${free}, swap ${Math.round(((host.swapUsed ?? 0) / host.swapTotal) * 100)}% full` : free;
  }
  return "pressure forced on";
}

/** `box is overloaded · load 140 · mostly TEST-hog (8.0 cores, 9.0 GB)`, or
 *  null when the host is calm. */
export function bannerText(name: string, host: Host): string | null {
  const ep = host.episode;
  if (!ep) return null;
  const worst = culprit(ep);
  const parts = [`${name} is overloaded`, pressureMetric(host, ep)];
  if (worst) parts.push(`mostly ${culpritName(worst)} (${useText(worst.kind === "session" ? worst.session : worst.proc)})`);
  return parts.join(" · ");
}

/** The note under a running turn that has said nothing for a while on an
 *  overloaded host; null when yardd does not list the session as stalled. */
export function stallNote(host: Host | undefined, sessionId: string): string | null {
  const quiet = host?.stalled?.find((s) => s.id === sessionId);
  if (!host || !quiet) return null;
  const why = host.episode ? ` (${pressureMetric(host, host.episode)})` : "";
  const long = quiet.quietSecs >= 60 ? `${Math.floor(quiet.quietSecs / 60)} min` : `${quiet.quietSecs} s`;
  return `No progress for ${long}: the host is overloaded${why}.`;
}

/** `45 s`, `12 min`, `1 h 5 min`; an open episode adds "so far". */
export function episodeLength(ep: PressureEpisode, now: number): string {
  const ms = Math.max(0, (ep.end ?? now) - ep.start);
  const s = Math.round(ms / 1000);
  const m = Math.floor(s / 60);
  const text = s < 60 ? `${s} s` : m < 60 ? `${m} min` : `${Math.floor(m / 60)} h ${m % 60} min`;
  return ep.end == null ? `${text} so far` : text;
}

const gbOf = (bytes: number) => (bytes / 1024 ** 3).toFixed(1);

/** `Load 140 on 28 cores · memory 22.1 of 23.4 GB · swap 3.0 of 3.0 GB`. */
export function episodePeaks(ep: PressureEpisode): string {
  const p = ep.peak;
  const parts = [`Load ${Math.round(p.load)} on ${p.cores} cores`, `memory ${gbOf(p.memUsed)} of ${gbOf(p.memTotal)} GB`];
  if (p.swapTotal > 0) parts.push(`swap ${gbOf(p.swapUsed)} of ${gbOf(p.swapTotal)} GB`);
  return parts.join(" · ");
}

/** When it started, on the reader's clock: `Nov 27, 14:05`. */
export const episodeStart = (ep: PressureEpisode) =>
  new Date(ep.start).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });

/** The host message's recent episodes followed by older pages, each once,
 *  newest first. */
export function mergePages(recent: PressureEpisode[], older: PressureEpisode[]): PressureEpisode[] {
  const seen = new Set<string>();
  return [...recent, ...older]
    .filter((e) => (seen.has(e.id) ? false : (seen.add(e.id), true)))
    .sort((a, b) => b.start - a.start);
}
