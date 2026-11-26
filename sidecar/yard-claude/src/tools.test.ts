import { expect, test } from "bun:test";
import { toolStarted } from "./tools";

test("calls that leave work in the process after the turn are flagged for yardd", () => {
  const started = (name: string, input: unknown) => toolStarted({ id: "t", name, input }, "/w");
  expect(started("Bash", { command: "gh pr checks --watch", run_in_background: true }).keepsAlive).toBe("background");
  expect(started("Monitor", { command: "tail -f log" }).keepsAlive).toBe("monitor");
  expect(started("ScheduleWakeup", { delaySeconds: 600 }).keepsAlive).toBe("wakeup");
  expect(started("CronCreate", { cron: "*/5 * * * *" }).keepsAlive).toBe("cron");
  expect(started("CronDelete", { id: "c1" }).endsCron).toBe(true);
  // A command that ends with its call, and a background subagent (tracked apart), are not.
  expect(started("Bash", { command: "ls" }).keepsAlive).toBeUndefined();
  expect(started("Agent", { prompt: "x", run_in_background: true }).keepsAlive).toBeUndefined();
});
