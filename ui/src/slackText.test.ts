import { describe, expect, test } from "bun:test";
import { emojiChar, parseSlack, plainSlack } from "./slackText";

describe("Slack markup", () => {
  test("reads bold, italics, strikes and code, not mid-word marks", () => {
    expect(parseSlack("a *bold* _it_ ~gone~ `x*y*`")).toEqual([
      { t: "text", text: "a " },
      { t: "bold", children: [{ t: "text", text: "bold" }] },
      { t: "text", text: " " },
      { t: "italic", children: [{ t: "text", text: "it" }] },
      { t: "text", text: " " },
      { t: "strike", children: [{ t: "text", text: "gone" }] },
      { t: "text", text: " " },
      { t: "code", text: "x*y*" },
    ]);
    expect(parseSlack("2*3*4 and snake_case_name")).toEqual([{ t: "text", text: "2*3*4 and snake_case_name" }]);
    expect(parseSlack("* not bold *")).toEqual([{ t: "text", text: "* not bold *" }]);
  });

  test("keeps code blocks as they are", () => {
    expect(parseSlack("look:\n```\nlet a = *b*; <@U1>\n```\ndone")).toEqual([
      { t: "text", text: "look:\n" },
      { t: "pre", text: "let a = *b*; <@U1>\n" },
      { t: "text", text: "\ndone" },
    ]);
  });

  test("reads links, mentions and specials, and only safe links are links", () => {
    expect(parseSlack("<https://example.com|the site> <mailto:a@b.co|mail> <javascript:alert(1)|click>")).toEqual([
      { t: "link", href: "https://example.com", children: [{ t: "text", text: "the site" }] },
      { t: "text", text: " " },
      { t: "link", href: "mailto:a@b.co", children: [{ t: "text", text: "mail" }] },
      { t: "text", text: " click" },
    ]);
    expect(parseSlack("hi <@U1> in <#C1|general>, <!here> <!subteam^S1|@devs>")).toEqual([
      { t: "text", text: "hi " },
      { t: "mention", kind: "user", id: "U1", label: "" },
      { t: "text", text: " in " },
      { t: "mention", kind: "channel", id: "C1", label: "#general" },
      { t: "text", text: ", " },
      { t: "mention", kind: "special", id: "here", label: "@here" },
      { t: "text", text: " " },
      { t: "mention", kind: "group", id: "S1", label: "@devs" },
    ]);
  });

  test("decodes Slack's escapes as text, never as markup", () => {
    expect(parseSlack("a &lt;b&gt; &amp; c")).toEqual([{ t: "text", text: "a <b> & c" }]);
    expect(parseSlack("&lt;script&gt;alert(1)&lt;/script&gt;")).toEqual([{ t: "text", text: "<script>alert(1)</script>" }]);
  });

  test("reads quotes and emoji", () => {
    expect(parseSlack("&gt; quoted *line*\nreply :tada: :custom-party:")).toEqual([
      { t: "quote", children: [{ t: "text", text: "quoted " }, { t: "bold", children: [{ t: "text", text: "line" }] }] },
      { t: "text", text: "reply " },
      { t: "emoji", name: "tada", char: "🎉" },
      { t: "text", text: " " },
      { t: "emoji", name: "custom-party", char: null },
    ]);
    expect(emojiChar("thumbsup::skin-tone-3")).toBe("👍");
  });

  test("reads as one line, with people's names", () => {
    expect(plainSlack("*hi* <@U1>\n:tada: <https://x.io|x>", { U1: "Ana" })).toBe("hi @Ana 🎉 x");
  });
});
