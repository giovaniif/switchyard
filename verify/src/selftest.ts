// `selftest`: the CLI's unit tests, and with --smoke a real round trip:
// an isolated yardd, a seed, a screenshot and a drive, then teardown.

import { join } from "node:path";
import { type Args, run, VERIFY } from "./common";
import type { Problem, Result } from "./out";

export function selftest(a: Args): Result {
  const problems: Problem[] = [];
  const t = run(["bun", "test"], { cwd: VERIFY, timeoutMs: 300000 });
  const summary = (t.err + t.out).match(/(\d+) pass[\s\S]*?(\d+) fail/);
  const unit = { exit: t.code, pass: Number(summary?.[1] ?? 0), fail: Number(summary?.[2] ?? 0), tail: t.code ? (t.err + t.out).slice(-3000) : undefined };
  if (t.code !== 0) problems.push({ code: "unit-tests", msg: `${unit.fail} unit test(s) failed`, hint: "cd verify && bun test" });
  let smoke: Record<string, unknown> | null = null;
  if (a.flags.smoke) {
    const yv = join(VERIFY, "yv");
    const env = { YV_ISO: "selftest" };
    const steps: Record<string, unknown>[] = [];
    const step = (args: string[]) => {
      const r = run([yv, ...args], { env, timeoutMs: 600000 });
      let j: any = null;
      try {
        j = JSON.parse(r.out);
      } catch {}
      steps.push({ cmd: `yv ${args.join(" ")}`, exit: r.code, problems: j?.problems ?? null });
      return r.code === 0;
    };
    const ok =
      step(["iso", "up"]) &&
      step(["iso", "seed"]) &&
      step(["state", "--expect", '{"sessions":[{"title":"TEST-say hello","status":"idle"}]}']) &&
      step(["shot", "session:{{sid_echo}}"]) &&
      step(["drive", "session:{{sid_cmd}}", "--viewport", "desktop", "--steps", '[{"click":"role:button:Allow once"},{"expect":"css:.thread","text":"Ran it."}]']) &&
      // A hand-off keeps the title, so a session route by id must not open the other one.
      step(["api", '{"op":"session.handoff","sessionId":"{{sid_echo}}","engine":"claude","model":"claude-haiku-5-5","effort":"low"}', "--wait", '{"op":"created"}']) &&
      step(["drive", "session:{{sid_echo}}", "--viewport", "desktop", "--steps", '[{"expect":"role:button:Model Fake Fast"},{"expect":"role:button:Effort Low"}]']) &&
      step(["drive", "session:{{sid_echo}}", "--viewport", "mobile", "--steps", '[{"click":"role:button:Session setup"},{"expect":"role:dialog:Session setup"},{"expect":"role:button:Model Fake Fast"},{"expect":"role:button:Effort Low"}]']);
    step(["iso", "down"]);
    smoke = { ok, steps };
    if (!ok) problems.push({ code: "smoke", msg: "the smoke round trip failed", hint: "see smoke.steps" });
  }
  return { ok: problems.length === 0, unit, smoke, problems };
}
