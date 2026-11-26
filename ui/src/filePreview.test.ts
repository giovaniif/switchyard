import { describe as group, expect, test } from "bun:test";
import { decodeText, delimiterFor, describe, imageMime, linkedLine, parseDelimited, PREVIEW_MAX_BYTES, previewKind, relativeTo, shortPath, tableView } from "./filePreview";

const bytes = (s: string) => new TextEncoder().encode(s);

group("previewKind", () => {
  test("by extension for text, by content for binary", () => {
    expect(previewKind("README.md", bytes("# Hi"))).toBe("markdown");
    expect(previewKind("notes.MARKDOWN", bytes("x"))).toBe("markdown");
    expect(previewKind("domains.csv", bytes("a,b"))).toBe("table");
    expect(previewKind("domains.tsv", bytes("a\tb"))).toBe("table");
    expect(previewKind("main.ts", bytes("let x = 1"))).toBe("text");
    expect(previewKind("Makefile", bytes("all:"))).toBe("text");
    expect(previewKind("shot.PNG", new Uint8Array([0x89, 0x50, 0, 0]))).toBe("image");
    expect(previewKind("archive.zip", new Uint8Array([0x50, 0x4b, 0, 1]))).toBe("none");
    expect(previewKind("latin1.txt", new Uint8Array([0x63, 0x61, 0x66, 0xe9]))).toBe("none");
    expect(previewKind("huge.csv", new Uint8Array(PREVIEW_MAX_BYTES + 1).fill(0x61))).toBe("none");
  });

  test("SVG is not shown as an image", () => {
    expect(imageMime("logo.svg")).toBeNull();
    expect(imageMime("photo.jpeg")).toBe("image/jpeg");
  });

  test("a byte-order mark is dropped", () => {
    expect(decodeText(bytes("﻿org,name"))).toBe("org,name");
  });
});

group("parseDelimited", () => {
  test("quotes hold delimiters, quotes and line breaks; CRLF ends a row", () => {
    const text = 'id,name,note\r\n1,"Revival & Ember, Co.","said ""hi"""\r\n2,MasterBilt,"two\nlines"\n';
    expect(parseDelimited(text, ",")).toEqual([
      ["id", "name", "note"],
      ["1", "Revival & Ember, Co.", 'said "hi"'],
      ["2", "MasterBilt", "two\nlines"],
    ]);
  });

  test("empty fields stay, a last line without a newline counts", () => {
    expect(parseDelimited("a,,c\n,b,", ",")).toEqual([
      ["a", "", "c"],
      ["", "b", ""],
    ]);
    expect(parseDelimited("", ",")).toEqual([]);
  });

  test("the delimiter: tab for .tsv, semicolon when the header has more of them", () => {
    expect(delimiterFor("x.tsv", "a,b\tc")).toBe("\t");
    expect(delimiterFor("x.csv", "a;b;c\n1,5;2;3")).toBe(";");
    expect(delimiterFor("x.csv", "a,b;c\n")).toBe(",");
    expect(delimiterFor("x.csv", "")).toBe(",");
  });
});

group("tableView", () => {
  test("header, limited rows, full count and number columns", () => {
    const all = [["org", "name", "users", "growth"], ["1", "A", "1,200", "4.5%"], ["2", "B", "", "-3%"], ["3", "C", "17", "n/a"]];
    const view = tableView(all, 2);
    expect(view.header).toEqual(["org", "name", "users", "growth"]);
    expect(view.rows).toHaveLength(2);
    expect(view.total).toBe(3);
    expect(view.columns).toBe(4);
    expect(view.numeric).toEqual([true, false, true, true]);
    expect(tableView(all).numeric).toEqual([true, false, true, false]);
  });

  test("ragged rows widen the table; an empty file has no columns", () => {
    expect(tableView([["a"], ["1", "2", "3"]]).columns).toBe(3);
    expect(tableView([])).toEqual({ header: [], rows: [], total: 0, columns: 0, numeric: [] });
  });
});

group("paths", () => {
  test("links in a Markdown file resolve from its folder", () => {
    expect(relativeTo("/w/docs/guide.md", "img/a.png")).toBe("/w/docs/img/a.png");
    expect(relativeTo("/w/docs/guide.md", "../data/x.csv")).toBe("/w/data/x.csv");
    expect(relativeTo("/w/docs/guide.md", "./x.csv:4")).toBe("/w/docs/x.csv:4");
    expect(relativeTo("/w/docs/guide.md", "/etc/hosts")).toBe("/etc/hosts");
    expect(relativeTo("/w/docs/guide.md", "~/n.md")).toBe("~/n.md");
    expect(relativeTo("/w/docs/guide.md", "sandbox:/mnt/data/x.csv")).toBe("sandbox:/mnt/data/x.csv");
    expect(relativeTo("/guide.md", "../../x")).toBe("/x");
  });

  test("the line a link points at", () => {
    expect(linkedLine("src/a.ts:42")).toBe(42);
    expect(linkedLine("src/a.ts:42:7")).toBe(42);
    expect(linkedLine("src/a.ts:42-50")).toBe(42);
    expect(linkedLine("src/a.ts#L42")).toBe(42);
    expect(linkedLine("src/a.ts#L42-L50")).toBe(42);
    expect(linkedLine("src/a.ts")).toBeNull();
    expect(linkedLine("src/a.ts:0")).toBeNull();
    expect(linkedLine("sandbox:/mnt/data/a.csv")).toBeNull();
  });

  test("home becomes ~", () => {
    expect(shortPath("/home/me/wt/a.csv", "/home/me")).toBe("~/wt/a.csv");
    expect(shortPath("/home/me/wt/a.csv", "/home/me/")).toBe("~/wt/a.csv");
    expect(shortPath("/home/meg/a.csv", "/home/me")).toBe("/home/meg/a.csv");
    expect(shortPath("/tmp/a.csv", "")).toBe("/tmp/a.csv");
  });
});

group("describe", () => {
  test("names the kind and its size in rows or lines", () => {
    expect(describe("domains.csv", "table", { rows: 1, columns: 4 })).toBe("CSV · 1 row · 4 columns");
    expect(describe("big.tsv", "table", { rows: 12345, columns: 2 })).toBe("TSV · 12,345 rows · 2 columns");
    expect(describe("README.md", "markdown", {})).toBe("Markdown");
    expect(describe("main.ts", "text", { lines: 40 })).toBe("TS · 40 lines");
    expect(describe("Makefile", "text", { lines: 1 })).toBe("File · 1 line");
    expect(describe("a.zip", "none", {})).toBe("ZIP");
  });
});
