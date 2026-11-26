import { describe, expect, test } from "bun:test";
import { splitRow, blockSegments, fencedBlocks } from "./markdown";

describe("fencedBlocks", () => {
  test("keeps surrounding prose and recognises a complete Mermaid fence", () => {
    expect(fencedBlocks("Before\n```mermaid\nflowchart LR\nA --> B\n```\nAfter")).toEqual([
      { kind: "text", text: "Before\n" },
      { kind: "code", language: "mermaid", body: "flowchart LR\nA --> B", complete: true },
      { kind: "text", text: "After" },
    ]);
  });
  test("waits for the closing fence while streaming, including an empty block", () => {
    expect(fencedBlocks("```mermaid\nflowchart LR\nA -->")).toEqual([
      { kind: "code", language: "mermaid", body: "flowchart LR\nA -->", complete: false },
    ]);
    expect(fencedBlocks("```mermaid\n")).toEqual([{ kind: "code", language: "mermaid", body: "", complete: false }]);
    expect(fencedBlocks("```mermaid\n```\n")).toEqual([{ kind: "code", language: "mermaid", body: "", complete: true }]);
  });
  test("supports tildes, case, CRLF and longer fences without parsing nested examples", () => {
    expect(fencedBlocks("  ~~~MERMAID title\r\nsequenceDiagram\r\n  ~~~~\r\n")).toEqual([
      { kind: "code", language: "mermaid", body: "sequenceDiagram", complete: true },
    ]);
    expect(fencedBlocks("````markdown\n```mermaid\nA\n```\n````")).toEqual([
      { kind: "code", language: "markdown", body: "```mermaid\nA\n```", complete: true },
    ]);
  });
  test("leaves inline backticks alone and preserves code indentation and language", () => {
    expect(fencedBlocks("Use `mermaid` and ```inline``` here.")).toEqual([{ kind: "text", text: "Use `mermaid` and ```inline``` here." }]);
    expect(fencedBlocks("```ts\n  const x = 1;\n```\n```\nplain\n```\n")).toEqual([
      { kind: "code", language: "ts", body: "  const x = 1;", complete: true },
      { kind: "code", language: "", body: "plain", complete: true },
    ]);
  });
});

describe("splitRow", () => {
  test("drops the outer pipes and trims cells", () => {
    expect(splitRow("| a | b |")).toEqual(["a", "b"]);
    expect(splitRow("a|b")).toEqual(["a", "b"]);
  });
  test("keeps escaped pipes and pipes in inline code", () => {
    expect(splitRow("| a \\| b | `x | y` |")).toEqual(["a | b", "`x | y`"]);
  });
  test("keeps empty cells", () => {
    expect(splitRow("| a | | c |")).toEqual(["a", "", "c"]);
  });
});

describe("blockSegments", () => {
  test("a table with alignments, short and long rows", () => {
    const segs = blockSegments(["| File | Result | Note |", "|---|:---:|--:|", "| a.ts | pass |", "| b.ts | fail | x | extra |"]);
    expect(segs).toEqual([
      {
        kind: "table",
        table: {
          header: ["File", "Result", "Note"],
          align: [null, "center", "right"],
          rows: [
            ["a.ts", "pass", ""],
            ["b.ts", "fail", "x"],
          ],
        },
      },
    ]);
  });
  test("text right before and after a table stays text", () => {
    const segs = blockSegments(["Results:", "| a | b |", "| - | - |", "| 1 | 2 |", "That is all."]);
    expect(segs.map((s) => s.kind)).toEqual(["text", "table", "text"]);
    expect(segs[0]).toEqual({ kind: "text", lines: ["Results:"] });
    expect(segs[2]).toEqual({ kind: "text", lines: ["That is all."] });
  });
  test("a pipe in prose is not a table", () => {
    expect(blockSegments(["use a | b to pipe", "then run it"])).toEqual([{ kind: "text", lines: ["use a | b to pipe", "then run it"] }]);
    expect(blockSegments(["a | b", "---"])).toEqual([{ kind: "text", lines: ["a | b", "---"] }]);
  });
  test("a delimiter row with another number of cells is not a table", () => {
    expect(blockSegments(["| a | b |", "|---|"]).every((s) => s.kind === "text")).toBe(true);
  });
  test("a header without body rows is still a table", () => {
    expect(blockSegments(["| a | b |", "|---|---|"])).toEqual([{ kind: "table", table: { header: ["a", "b"], align: [null, null], rows: [] } }]);
  });
});

describe("blockSegments: headings and lists without a blank line", () => {
  test("a heading over its text, a list under its lead-in", () => {
    expect(blockSegments(["## Summary", "Fixed the parser."])).toEqual([
      { kind: "heading", text: "Summary", level: 2 },
      { kind: "text", lines: ["Fixed the parser."] },
    ]);
    expect(blockSegments(["Changes:", "- src/a.ts", "* src/b.ts"])).toEqual([
      { kind: "text", lines: ["Changes:"] },
      { kind: "ul", items: ["src/a.ts", "src/b.ts"] },
    ]);
  });
  test("a numbered list that continues after a code block keeps its number", () => {
    expect(blockSegments(["2. Build the UI:"])).toEqual([{ kind: "ol", items: ["Build the UI:"], start: 2 }]);
    expect(blockSegments(["1. Install the deps:"])).toEqual([{ kind: "ol", items: ["Install the deps:"] }]);
  });
  test("a whole-paragraph list stays one list", () => {
    expect(blockSegments(["1. one", "2. two"])).toEqual([{ kind: "ol", items: ["one", "two"] }]);
  });
  test("a numbered list interrupting text must start at 1", () => {
    expect(blockSegments(["Steps:", "1. build", "2. test"])).toEqual([
      { kind: "text", lines: ["Steps:"] },
      { kind: "ol", items: ["build", "test"] },
    ]);
    expect(blockSegments(["It shipped in", "2024. Then it grew."])).toEqual([{ kind: "text", lines: ["It shipped in", "2024. Then it grew."] }]);
  });
  test("an indented line continues its item; a bullet switch starts a new list", () => {
    expect(blockSegments(["- first item", "  wraps here", "1. then numbered"])).toEqual([
      { kind: "ul", items: ["first item wraps here"] },
      { kind: "ol", items: ["then numbered"] },
    ]);
  });
  test("not headings or lists", () => {
    expect(blockSegments(["#hashtag", "**bold** start", "-1 degrees"])).toEqual([{ kind: "text", lines: ["#hashtag", "**bold** start", "-1 degrees"] }]);
  });
  test("a heading keeps its level", () => {
    expect(blockSegments(["# Report", "###### Fine print"])).toEqual([
      { kind: "heading", text: "Report", level: 1 },
      { kind: "heading", text: "Fine print", level: 6 },
    ]);
  });
  test("lines starting with > are one quote; a > later in a line is text", () => {
    expect(blockSegments(["Note:", "> Registrar names lag", ">by a day.", "Echo: > say hello"])).toEqual([
      { kind: "text", lines: ["Note:"] },
      { kind: "quote", lines: ["Registrar names lag", "by a day."] },
      { kind: "text", lines: ["Echo: > say hello"] },
    ]);
  });
  test("closing hashes are dropped from a heading", () => {
    expect(blockSegments(["### Notes ###"])).toEqual([{ kind: "heading", text: "Notes", level: 3 }]);
    expect(blockSegments(["## Use C#"])).toEqual([{ kind: "heading", text: "Use C#", level: 2 }]);
  });
});
