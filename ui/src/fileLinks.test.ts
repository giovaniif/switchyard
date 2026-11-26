import { describe, expect, test } from "bun:test";
import { fileError, fileTarget } from "./fileLinks";

describe("fileTarget", () => {
  test("every dialect agents use for a file is a file", () => {
    for (const dest of [
      "/home/me/wt/app/report.csv",
      "report.csv",
      "./report.csv",
      "../out/report.csv",
      "src/a.ts:42",
      "src/a.ts:42:7",
      "src/a.ts#L42-L50",
      "Makefile:12",
      "~/notes/todo.md",
      "my%20report.csv",
      "my report.csv",
      "file:///home/me/report.csv",
      "file://localhost/home/me/report.csv",
      "sandbox:/mnt/data/scale_connected_domains.csv",
    ]) {
      expect([dest, fileTarget(dest)]).toEqual([dest, dest]);
    }
    expect(fileTarget("</home/me/a b.csv>")).toBe("/home/me/a b.csv");
  });

  test("other schemes, anchors and remote files are not", () => {
    for (const dest of [
      "javascript:alert(1)",
      "JavaScript:alert(1)",
      "mailto:me@example.com",
      "vscode://file/x",
      "data:text/html,hi",
      "file://example.com/etc/passwd",
      "sandbox:",
      "#intro",
      "?q=1",
      "//example.com/x",
      "C:\\Users\\x.txt",
      "",
      "  ",
      "a\nb",
    ]) {
      expect([dest, fileTarget(dest)]).toEqual([dest, null]);
    }
  });
});

describe("fileError", () => {
  test("says what went wrong, and names an outdated host", () => {
    expect(fileError({ error: "/w/gone.csv was not found" })).toBe("/w/gone.csv was not found");
    expect(fileError({ op: "error", message: "no such session" })).toBe("No such session");
    expect(fileError({ op: "error", message: "unknown op `file.get`" })).toBe("That host runs an older Switchyard. Update it to download files.");
    expect(fileError({})).toBe("The file could not be read.");
  });
});
