import { describe, expect, test } from "bun:test";
import type { Host, PressureEpisode } from "./protocol";
import { bannerText, culprit, episodeLength, episodePeaks, mergePages, stallNote } from "./pressure";

const GB = 1024 ** 3;
const episode = (over: Partial<PressureEpisode> = {}): PressureEpisode => ({
  id: "1",
  start: 0,
  end: null,
  lastSeen: 0,
  reasons: ["load"],
  peak: { load: 140, cores: 28, memUsed: 22.1 * GB, memTotal: 23.4 * GB, swapUsed: 3 * GB, swapTotal: 3 * GB },
  top: {
    at: 0,
    load: 140,
    memUsed: 22 * GB,
    swapUsed: 3 * GB,
    sessions: [
      { id: "s1", title: "TEST-hog", engine: "claude", cpuCores: 8, memBytes: 9 * GB },
      { id: "s2", title: "TEST-small", engine: "codex", automation: "Nightly", cpuCores: 0.5, memBytes: 12 * GB },
    ],
    procs: [{ pid: 4242, name: "yardd", cmd: "/tmp/target/debug/yardd", cpuCores: 3, memBytes: 2 * GB }],
  },
  ...over,
});
const host = (over: Partial<Host> = {}): Host => ({
  id: "local",
  name: "box",
  cpu: 99,
  cores: 28,
  load: 139.6,
  memUsed: 22 * GB,
  memTotal: 23.4 * GB,
  diskUsed: 0,
  diskTotal: 1,
  cpuHistory: [],
  episode: episode(),
  ...over,
});

describe("the worst contributor", () => {
  test("by CPU when load is the trouble, among sessions and stray processes alike", () => {
    expect(culprit(episode())).toEqual({ kind: "session", session: episode().top.sessions[0], byCpu: true });
    const stray = episode({ top: { ...episode().top, procs: [{ pid: 7, name: "cargo", cmd: "cargo test", cpuCores: 20, memBytes: GB }] } });
    expect(culprit(stray)).toMatchObject({ kind: "proc", proc: { pid: 7 } });
  });
  test("by memory when memory is", () => {
    expect(culprit(episode({ reasons: ["memory", "swap"] }))).toMatchObject({ kind: "session", session: { id: "s2" }, byCpu: false });
  });
  test("nobody when nothing was measured", () => {
    expect(culprit(episode({ top: { ...episode().top, sessions: [], procs: [] } }))).toBeNull();
  });
});

describe("the banner", () => {
  test("names the host, the load and the worst session", () => {
    expect(bannerText("box", host())).toBe("box is overloaded · load 140 · mostly TEST-hog (8.0 cores, 9.0 GB)");
  });
  test("memory and swap say how much is left; a stray process is named with its pid", () => {
    const h = host({ memUsed: 22.6 * GB, episode: episode({ reasons: ["memory", "swap"], top: { ...episode().top, sessions: [], procs: [{ pid: 9, name: "node", cmd: "node server.js", cpuCores: 0.1, memBytes: 6 * GB }] } }) });
    expect(bannerText("box", h)).toBe("box is overloaded · 819 MB free · mostly node, pid 9 (0.1 cores, 6.0 GB)");
    // yardd counts full swap only with memory low too: both are said.
    expect(bannerText("box", host({ memUsed: 22.6 * GB, swapUsed: 2.9 * GB, swapTotal: 3 * GB, episode: episode({ reasons: ["memory", "swap"] }) }))).toBe(
      "box is overloaded · 819 MB free, swap 97% full · mostly TEST-small (0.5 cores, 12.0 GB)",
    );
  });
  test("quiet when there is no episode", () => {
    expect(bannerText("box", host({ episode: null }))).toBeNull();
    expect(bannerText("box", host({ episode: undefined }))).toBeNull();
  });
});

describe("the stalled note", () => {
  test("for a session yardd lists as quiet while the host is overloaded", () => {
    const h = host({ stalled: [{ id: "s1", quietSecs: 312 }] });
    expect(stallNote(h, "s1")).toBe("No progress for 5 min: the host is overloaded (load 140).");
    expect(stallNote(h, "s2")).toBeNull();
    expect(stallNote(host({ stalled: [{ id: "s1", quietSecs: 42 }], episode: episode({ reasons: ["memory"] }), memUsed: 22.4 * GB }), "s1")).toBe("No progress for 42 s: the host is overloaded (1.0 GB free).");
    expect(stallNote(host({ stalled: [] }), "s1")).toBeNull();
    expect(stallNote(undefined, "s1")).toBeNull();
  });
});

describe("the history", () => {
  test("how long an episode lasted, or has lasted so far", () => {
    expect(episodeLength(episode({ start: 0, end: 45_000 }), 0)).toBe("45 s");
    expect(episodeLength(episode({ start: 0, end: 12 * 60_000 + 10_000 }), 0)).toBe("12 min");
    expect(episodeLength(episode({ start: 0, end: 65 * 60_000 }), 0)).toBe("1 h 5 min");
    expect(episodeLength(episode({ start: 0, end: null }), 3 * 60_000)).toBe("3 min so far");
  });
  test("its peaks", () => {
    expect(episodePeaks(episode())).toBe("Load 140 on 28 cores · memory 22.1 of 23.4 GB · swap 3.0 of 3.0 GB");
    expect(episodePeaks(episode({ peak: { ...episode().peak, swapTotal: 0, swapUsed: 0 } }))).toBe("Load 140 on 28 cores · memory 22.1 of 23.4 GB");
  });
  test("older pages join the recent ones without repeats, newest first", () => {
    const e = (id: string, start: number) => episode({ id, start, end: start + 1 });
    expect(mergePages([e("c", 3), e("b", 2)], [e("b", 2), e("a", 1)]).map((x) => x.id)).toEqual(["c", "b", "a"]);
  });
});
