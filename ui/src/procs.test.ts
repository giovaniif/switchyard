import { describe, expect, test } from "bun:test";
import type { ProcRow } from "./protocol";
import { procsBlank, procSummary, procTree, pruneKills, rowActions, shortCommand } from "./procs";

const row = (pid: number, ppid: number, cmd: string, extra: Partial<ProcRow> = {}): ProcRow => ({
  pid,
  ppid,
  name: cmd.split(" ")[0],
  cmd,
  cpu: 0,
  memBytes: 0,
  ageSecs: 1,
  origin: "agent",
  ...extra,
});

describe("procTree", () => {
  test("parents come before their children, children by pid, with their depth", () => {
    const rows = [row(30, 20, "sleep 30"), row(20, 10, "claude"), row(10, 1, "bun main.ts"), row(25, 20, "bash -c ls")];
    expect(procTree(rows).map((r) => [r.row.pid, r.depth])).toEqual([
      [10, 0],
      [20, 1],
      [25, 2],
      [30, 2],
    ]);
  });

  test("a process whose parent is not listed is a root; separate trees follow each other", () => {
    const rows = [row(50, 9, "zsh", { origin: "terminal" }), row(10, 1, "bun main.ts"), row(11, 10, "claude")];
    expect(procTree(rows).map((r) => [r.row.pid, r.depth])).toEqual([
      [10, 0],
      [11, 1],
      [50, 0],
    ]);
  });

  test("a parent loop cannot hang it", () => {
    expect(procTree([row(1, 2, "a"), row(2, 1, "b")]).length).toBe(2);
  });
});

describe("shortCommand", () => {
  test("long absolute paths shrink to their last segment, the rest stays", () => {
    expect(shortCommand("/home/giovani/.bun/bin/bun run /home/giovani/code/app/sidecar/main.ts --flag")).toBe("bun run main.ts --flag");
    expect(shortCommand("sleep 30")).toBe("sleep 30");
    expect(shortCommand("ls /tmp")).toBe("ls /tmp");
  });

  test("whitespace collapses and a long line is cut with an ellipsis", () => {
    expect(shortCommand("a   b\n c")).toBe("a b c");
    const long = shortCommand(`node ${"x".repeat(200)}`, 40);
    expect(long.length).toBe(40);
    expect(long.endsWith("…")).toBe(true);
  });
});

describe("procSummary", () => {
  test("counts processes and adds up CPU and memory", () => {
    const rows = [row(1, 0, "a", { cpu: 50, memBytes: 100 * 1024 ** 2 }), row(2, 1, "b", { cpu: 25, memBytes: 1.5 * 1024 ** 3 })];
    expect(procSummary(rows)).toBe("2 processes · 75% CPU · 1.6 GB");
    expect(procSummary([rows[0]])).toBe("1 process · 50% CPU · 100 MB");
    expect(procSummary([])).toBe("No processes");
  });
});

describe("rowActions", () => {
  // Sidecar 10 and its CLI 11 are the engine's; 12 is a command with a child 13.
  const rows = [
    row(10, 1, "bun main.ts", { engine: true, start: 100 }),
    row(11, 10, "claude", { engine: true, start: 101 }),
    row(12, 11, "npm run dev", { start: 102 }),
    row(13, 12, "node server.js", { start: 103 }),
    row(20, 5, "bash", { origin: "terminal", start: 200 }),
  ];
  const at = (pid: number) => rows.find((r) => r.pid === pid)!;

  test("the engine's own processes offer nothing", () => {
    expect(rowActions(at(10), rows)).toEqual({ stop: false, withChildren: false, force: false, forceTree: false });
    expect(rowActions(at(11), rows)).toEqual({ stop: false, withChildren: false, force: false, forceTree: false });
    // Whatever the tab thinks happened.
    expect(rowActions(at(11), rows, { phase: "done", result: "running", signal: "term", tree: false }).force).toBe(false);
  });

  test("a process offers Stop, and Stop with children only when it has some", () => {
    expect(rowActions(at(12), rows)).toEqual({ stop: true, withChildren: true, force: false, forceTree: false });
    expect(rowActions(at(13), rows)).toEqual({ stop: true, withChildren: false, force: false, forceTree: false });
    expect(rowActions(at(20), rows)).toEqual({ stop: true, withChildren: false, force: false, forceTree: false });
    // An older yardd sends neither `engine` nor `start`.
    expect(rowActions(row(30, 1, "sleep 30"), [row(30, 1, "sleep 30")]).stop).toBe(true);
  });

  test("while the request is out, nothing is offered", () => {
    const a = rowActions(at(13), rows, { phase: "sending", signal: "term", tree: false, start: 103 });
    expect(a).toEqual({ stop: false, withChildren: false, force: false, forceTree: false, note: { text: "Stopping…" } });
    expect(rowActions(at(13), rows, { phase: "sending", signal: "kill", tree: false }).note?.text).toBe("Force killing…");
  });

  test("still running after SIGTERM offers Force kill and nothing else", () => {
    const a = rowActions(at(12), rows, { phase: "done", result: "running", signal: "term", tree: true, start: 102 });
    expect(a).toEqual({ stop: false, withChildren: false, force: true, forceTree: true, note: { text: "Still running after SIGTERM." } });
    expect(rowActions(at(12), rows, { phase: "done", result: "running", signal: "kill", tree: false }).note?.text).toBe("Still running after SIGKILL.");
  });

  test("a refusal or a failure says why and offers Stop again", () => {
    const refused = rowActions(at(13), rows, { phase: "done", result: "refused", reason: "pid 13 is now a different process", signal: "term", tree: false });
    expect(refused).toEqual({ stop: true, withChildren: false, force: false, forceTree: false, note: { text: "Not stopped: pid 13 is now a different process.", bad: true } });
    const failed = rowActions(at(13), rows, { phase: "done", result: "failed", reason: "Box runs an older Switchyard.", signal: "term", tree: false });
    expect(failed.note).toEqual({ text: "Box runs an older Switchyard.", bad: true });
    expect(failed.stop).toBe(true);
  });

  test("a stopped process says so until it leaves the list", () => {
    expect(rowActions(at(13), rows, { phase: "done", result: "killed", signal: "term", tree: false }).note?.text).toBe("Stopped.");
    expect(rowActions(at(12), rows, { phase: "done", result: "killed", signal: "kill", tree: true }).note?.text).toBe("Stopped with its children.");
  });

  test("a kill of an earlier process with the same pid is not this row's", () => {
    const a = rowActions(at(13), rows, { phase: "done", result: "running", signal: "term", tree: false, start: 50 });
    expect(a).toEqual({ stop: true, withChildren: false, force: false, forceTree: false });
  });
});

describe("pruneKills", () => {
  test("drops kills of processes that left the list, keeps one under way", () => {
    const kills = {
      12: { phase: "done", result: "running", signal: "term", tree: false },
      13: { phase: "done", result: "killed", signal: "term", tree: false },
      14: { phase: "sending", signal: "term", tree: false },
    } as const;
    const rows = [row(12, 1, "npm run dev")];
    expect(Object.keys(pruneKills(kills, rows))).toEqual(["12", "14"]);
    // Nothing to drop: the same object, so React state does not change.
    const same = { 12: kills[12] };
    expect(pruneKills(same, rows)).toBe(same);
  });
});

describe("procsBlank", () => {
  const rows = [row(10, 1, "sleep 30")];
  test("the list, a wait, or nothing running, while the host is online", () => {
    expect(procsBlank(rows, true)).toBeNull();
    expect(procsBlank(undefined, true)).toBe("Loading…");
    expect(procsBlank([], true)).toBe("Nothing is running for this session.");
  });
  test("an offline host says so rather than a stale list, a wait that never ends, or nothing running", () => {
    for (const r of [rows, undefined, []]) expect(procsBlank(r, false)).toBe("Processes are available when this host reconnects.");
  });
});
