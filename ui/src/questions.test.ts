import { describe, expect, test } from "bun:test";
import { answerOf, answersFrom, noPick, toggle, typeOther } from "./questions";
import type { Question } from "./protocol";

const one: Question = { question: "Which database?", header: "Database", multiSelect: false, options: [{ label: "Postgres", description: "" }, { label: "SQLite", description: "" }] };
const many: Question = { question: "Which checks?", header: "Checks", multiSelect: true, options: [{ label: "Unit", description: "" }, { label: "Lint", description: "" }, { label: "E2E", description: "" }] };

describe("question answers", () => {
  test("one choice: an option replaces the last, typed text replaces the option", () => {
    let p = toggle(one, noPick, "Postgres");
    p = toggle(one, p, "SQLite");
    expect(answerOf(one, p)).toBe("SQLite");
    p = typeOther(one, p, "  DuckDB ");
    expect(p.labels).toEqual([]);
    expect(answerOf(one, p)).toBe("DuckDB");
    // Picking an option again clears what was typed.
    expect(toggle(one, p, "Postgres")).toEqual({ labels: ["Postgres"], other: "" });
  });

  test("several: labels toggle and join in the options' order, typed text last", () => {
    let p = toggle(many, noPick, "E2E");
    p = toggle(many, p, "Unit");
    expect(answerOf(many, p)).toBe("Unit, E2E");
    p = toggle(many, p, "E2E");
    p = typeOther(many, p, "Fuzzing");
    expect(answerOf(many, p)).toBe("Unit, Fuzzing");
  });

  test("answers only once every question has one", () => {
    expect(answersFrom([one, many], { "Which database?": toggle(one, noPick, "SQLite") })).toBeNull();
    expect(answersFrom([one, many], { "Which database?": toggle(one, noPick, "SQLite"), "Which checks?": toggle(many, noPick, "Lint") })).toEqual({
      "Which database?": "SQLite",
      "Which checks?": "Lint",
    });
    expect(answersFrom([one], { "Which database?": typeOther(one, noPick, "   ") })).toBeNull();
  });
});
