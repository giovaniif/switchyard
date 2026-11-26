import { describe, expect, test } from "bun:test";
import { answeredInput, questionRequest, questionsOf } from "./questions";
import { describe as describeTool } from "./tools";

const input = {
  questions: [
    { question: "Which database?", header: "Database", multiSelect: false, options: [{ label: "Postgres", description: "Now." }, { label: "SQLite", description: "One file." }] },
    { question: "Which checks?", header: "Checks", multiSelect: true, options: [{ label: "Unit", description: "" }, { label: "Lint", description: "" }] },
  ],
};

describe("AskUserQuestion", () => {
  test("becomes a question request naming its call", () => {
    const req = questionRequest("claude-1", input, "toolu_1");
    expect(req).toMatchObject({ type: "approval_request", requestId: "claude-1", kind: "question", title: "Which database?", detail: "and 1 more", itemId: "toolu_1" });
    expect(req.questions).toEqual(input.questions);
  });

  test("its step reads as the question, not as JSON", () => {
    expect(describeTool("AskUserQuestion", input, "/")).toEqual({ kind: "other", title: "Asked", detail: "Which database? (and 1 more)" });
  });

  test("malformed questions and options are left out", () => {
    expect(questionsOf({ questions: [{ question: "" }, { question: "Ok?", options: [{ label: "" }, { label: "Yes", description: 3 }] }, "x"] })).toEqual([
      { question: "Ok?", header: "", multiSelect: false, options: [{ label: "Yes", description: "" }] },
    ]);
    expect(questionsOf(null)).toEqual([]);
  });

  test("answers go into the input, only for questions it asked, only as text", () => {
    const out = answeredInput(input, { "Which database?": "SQLite", "Which checks?": "Unit, Lint", "Made up?": "yes", other: 3 });
    expect(out.answers).toEqual({ "Which database?": "SQLite", "Which checks?": "Unit, Lint" });
    expect(out.questions).toEqual(input.questions);
    expect(answeredInput(input, undefined).answers).toEqual({});
  });
});
