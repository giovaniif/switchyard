import { describe, expect, test } from "bun:test";
import { appendToComposer, cleanQuote, COMMENTS_HEADER, draftOf, keepDraft, onAppend, quoteInComposer, quoteSegments, returnToComposer, SELECTION_CAP, splitComments, splitQuotes, withQuotes, type Addition, snippetBlock, fenceSegments } from "./compose";

describe("cleanQuote", () => {
  test("keeps a selection's lines as they are", () => {
    expect(cleanQuote("Run the migration first.\nThen restart.")).toBe("Run the migration first.\nThen restart.");
  });
  test("keeps paragraph breaks, trims the ends", () => {
    expect(cleanQuote("\n  one\n\n\n\ntwo  \n\n")).toBe("  one\n\ntwo");
  });
  test("normalises Windows line ends", () => {
    expect(cleanQuote("a\r\nb")).toBe("a\nb");
  });
  test("cuts a huge selection to the cap", () => {
    expect(cleanQuote("x".repeat(SELECTION_CAP + 500)).length).toBe(SELECTION_CAP);
  });
  test("nothing selected quotes nothing", () => {
    expect(cleanQuote("  \n ")).toBe("");
  });
});

describe("withQuotes", () => {
  test("each quote becomes a Markdown blockquote before the reply, a blank line apart", () => {
    expect(withQuotes(["Run the migration first.\nThen restart.", "Use the flag."], "Why both?")).toBe(
      "> Run the migration first.\n> Then restart.\n\n> Use the flag.\n\nWhy both?",
    );
  });
  test("a blank line inside a quote stays inside it", () => {
    expect(withQuotes(["one\n\ntwo"], "ok")).toBe("> one\n>\n> two\n\nok");
  });
  test("no quotes sends the text as typed", () => {
    expect(withQuotes([], "hi")).toBe("hi");
  });
  test("quotes alone are the whole message", () => {
    expect(withQuotes(["a", "b"], "")).toBe("> a\n\n> b");
  });
});

describe("splitQuotes", () => {
  test("takes the leading blockquotes back out as quotes", () => {
    expect(splitQuotes("> Run the migration first.\n> Then restart.\n\n> Use the flag.\n\nWhy both?")).toEqual({
      quotes: [{ text: "Run the migration first.\nThen restart." }, { text: "Use the flag." }],
      text: "Why both?",
    });
  });
  test("round-trips what withQuotes sends", () => {
    const quotes = ["one\n\ntwo", "  indented", "> nested"];
    expect(splitQuotes(withQuotes(quotes, "reply\n> not a leading quote"))).toEqual({ quotes: quotes.map((text) => ({ text })), text: "reply\n> not a leading quote" });
  });
  test("a quote with no space after > still counts", () => {
    expect(splitQuotes(">a\n>b\n\nok")).toEqual({ quotes: [{ text: "a\nb" }], text: "ok" });
  });
  test("quotes alone leave no text", () => {
    expect(splitQuotes("> a\n\n> b")).toEqual({ quotes: [{ text: "a" }, { text: "b" }], text: "" });
  });
  test("a quote that runs straight into text stays text", () => {
    expect(splitQuotes("> a\nreply")).toEqual({ quotes: [], text: "> a\nreply" });
  });
  test("a message without quotes is all text", () => {
    expect(splitQuotes("plain\n\n> later")).toEqual({ quotes: [], text: "plain\n\n> later" });
  });
});

describe("comments on parts of an answer", () => {
  const parts = [
    { text: "Run the migration first.\nThen restart.", note: "Which migration? There are two." },
    { text: "Use the flag.", note: "Which flag?\n\nThe docs list --force and --yes." },
    { text: "Then deploy." },
  ];
  test("each part and its own comment are one numbered item, after a header and before the text", () => {
    expect(withQuotes(parts, "Otherwise fine.")).toBe(
      [
        COMMENTS_HEADER,
        "",
        "1. > Run the migration first.\n   > Then restart.\n   Which migration? There are two.",
        "",
        "2. > Use the flag.\n   Which flag?\n\n   The docs list --force and --yes.",
        "",
        "3. > Then deploy.\n   (no comment on this part)",
        "",
        "Otherwise fine.",
      ].join("\n"),
    );
  });
  test("without any comment, quotes go as before", () => {
    expect(withQuotes([{ text: "a" }, { text: "b", note: "  " }], "why?")).toBe("> a\n\n> b\n\nwhy?");
  });
  test("round-trip: Edit and Stop give the parts back with their comments", () => {
    const sent = withQuotes(parts, "Otherwise fine.\n\n> a quote I typed");
    expect(splitQuotes(sent)).toEqual({
      quotes: [parts[0], parts[1], { text: "Then deploy." }],
      text: "Otherwise fine.\n\n> a quote I typed",
    });
    expect(splitQuotes(withQuotes(parts, ""))).toEqual({ quotes: [parts[0], parts[1], { text: "Then deploy." }], text: "" });
  });
  test("a quote with blank lines and a comment that starts with > stay apart", () => {
    const q = [{ text: "one\n\ntwo", note: "> not part of the quote" }];
    expect(splitQuotes(withQuotes(q, "ok"))).toEqual({ quotes: q, text: "ok" });
  });
  test("only a message that starts with the header is read as comments", () => {
    expect(splitComments("hello\n" + COMMENTS_HEADER)).toBeNull();
    expect(splitComments(COMMENTS_HEADER + "\n\njust text")).toBeNull();
  });
});

describe("quoteSegments", () => {
  test("splits a sent message into its quotes and its text", () => {
    expect(quoteSegments("> Run the migration first.\n> Then restart.\n\n> Use the flag.\n\nWhy both?")).toEqual([
      { quote: true, text: "Run the migration first.\nThen restart." },
      { quote: true, text: "Use the flag." },
      { quote: false, text: "Why both?" },
    ]);
  });
  test("a quote in the middle of a message", () => {
    expect(quoteSegments("About this:\n> a\n>\n> b\nand that.")).toEqual([
      { quote: false, text: "About this:" },
      { quote: true, text: "a\n\nb" },
      { quote: false, text: "and that." },
    ]);
  });
  test("plain text is one segment, untouched", () => {
    expect(quoteSegments("one\n\n  two")).toEqual([{ quote: false, text: "one\n\n  two" }]);
  });
  test("> inside a fenced block is not a quote", () => {
    const text = "Output:\n```\n$ echo \\\n> hi\n```";
    expect(quoteSegments(text)).toEqual([{ quote: false, text }]);
  });
});

describe("composer additions", () => {
  const listen = (sessionId: string) => {
    const got: Addition[] = [];
    const stop = onAppend(sessionId, (a) => got.push(a));
    return { got, stop };
  };
  test("a quoted selection arrives as a quote, not as text", () => {
    const { got, stop } = listen("s1");
    quoteInComposer("s1", "\nRun the migration first.\n");
    quoteInComposer("s1", "   ");
    stop();
    expect(got).toEqual([{ quotes: [{ text: "Run the migration first.", range: undefined }], focusQuote: true }]);
  });
  test("terminal text arrives as text", () => {
    const { got, stop } = listen("s2");
    appendToComposer("s2", "```\nls\n```\n");
    stop();
    expect(got).toEqual([{ text: "```\nls\n```\n" }]);
  });
  test("a message given back for editing brings its quotes back as quotes", () => {
    const { got, stop } = listen("s3");
    returnToComposer("s3", "> Use the flag.\n\nWhy?");
    stop();
    expect(got).toEqual([{ quotes: [{ text: "Use the flag." }], text: "Why?" }]);
  });
  test("a message given back for editing brings its files back too", () => {
    const file = { id: "a1", name: "shot.png", mime: "image/png", size: 42, path: "/data/attachments/a1/shot.png" };
    const { got, stop } = listen("s5");
    returnToComposer("s5", "Look", [file]);
    returnToComposer("s5", "", [file]);
    stop();
    expect(got).toEqual([{ text: "Look", attachments: [file] }, { text: "", attachments: [file] }]);
  });
  test("only the session's own composer hears it", () => {
    const { got, stop } = listen("s4");
    quoteInComposer("other", "x");
    stop();
    expect(got).toEqual([]);
  });
});

describe("drafts", () => {
  test("a session's draft is kept until cleared, apart from others", () => {
    keepDraft("s1", { text: "half a reply" });
    keepDraft("s2", { text: "other" });
    expect(draftOf<{ text: string }>("s1")?.text).toBe("half a reply");
    keepDraft("s1", null);
    expect(draftOf("s1")).toBeUndefined();
    expect(draftOf<{ text: string }>("s2")?.text).toBe("other");
    expect(draftOf(undefined)).toBeUndefined();
  });
});

describe("terminal snippets", () => {
  test("go after the quotes and before the text, as a labelled fenced block", () => {
    expect(withQuotes(["why?"], "look", [{ label: "Terminal 1", text: "$ ls\na b" }])).toBe("> why?\n\nTerminal 1, 2 lines:\n```text\n$ ls\na b\n```\n\nlook");
  });
  test("alone they still send", () => {
    expect(withQuotes([], "", [{ label: "Terminal 1", text: "ok" }])).toBe("Terminal 1, 1 line:\n```text\nok\n```");
  });
  test("backticks in the output get a longer fence", () => {
    expect(snippetBlock({ label: "T", text: "a ```b``` c" })).toBe("T, 1 line:\n````text\na ```b``` c\n````");
  });
});

describe("fenceSegments", () => {
  test("a closed fenced block is code; the text around it stays text", () => {
    expect(fenceSegments("Terminal 1, 2 lines:\n```text\n$ ls\na b\n```\n\nwhy?")).toEqual([
      { code: false, text: "Terminal 1, 2 lines:" },
      { code: true, text: "$ ls\na b" },
      { code: false, text: "why?" },
    ]);
  });
  test("a longer fence holds shorter backtick runs", () => {
    expect(fenceSegments("````text\na ```\nb\n````")).toEqual([{ code: true, text: "a ```\nb" }]);
  });
  test("an unclosed fence is plain text", () => {
    expect(fenceSegments("look:\n```\nno end")).toEqual([{ code: false, text: "look:\n```\nno end" }]);
  });
  test("no fence: one text segment, untouched", () => {
    expect(fenceSegments("  one\n\ntwo")).toEqual([{ code: false, text: "  one\n\ntwo" }]);
  });
});
