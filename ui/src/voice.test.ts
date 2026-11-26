import { describe, expect, test } from "bun:test";
import type { Item } from "./protocol";
import {
  SILENCE_MS,
  VOICE_OFF,
  heardText,
  isEcho,
  isStopWord,
  joinDictation,
  markEnd,
  recognitionProblem,
  speakable,
  speechChunks,
  voiceCue,
  voiceStep,
  type VoiceMode,
} from "./voice";

describe("speakable", () => {
  test("plain prose stays as it is", () => {
    expect(speakable("All tests pass. The build is green.")).toBe("All tests pass. The build is green.");
  });
  test("a fenced code block is not read out", () => {
    const md = "Here is the fix:\n\n```ts\nconst a = 1;\nconst b = 2;\n```\n\nThat's it.";
    expect(speakable(md)).toBe("Here is the fix: Code block omitted. That's it.");
  });
  test("an unclosed fence (a cut-off answer) is omitted too", () => {
    expect(speakable("Start:\n```sh\nrm -rf build")).toBe("Start: Code block omitted.");
  });
  test("inline code keeps its words without backticks", () => {
    expect(speakable("Run `bun test` in `ui`.")).toBe("Run bun test in ui.");
  });
  test("links read their text, bare URLs are named, images are dropped", () => {
    expect(speakable("See [the PR](https://github.com/x/y/pull/1) and https://example.com/a?b=c ![shot](a.png) now.")).toBe(
      "See the PR and a link now.",
    );
  });
  test("headings, emphasis, quotes and list markers go", () => {
    const md = "## Summary\n\n- **Fixed** the _flaky_ test\n- Removed ~~dead~~ code\n1. First\n> quoted line";
    expect(speakable(md)).toBe("Summary. Fixed the flaky test. Removed dead code. First. quoted line");
  });
  test("a table is announced, not read cell by cell", () => {
    const md = "Results:\n\n| Suite | Pass |\n|---|---|\n| ui | 120 |\n| yardd | 80 |\n\nAll good.";
    expect(speakable(md)).toBe("Results: Table omitted. All good.");
  });
  test("HTML-ish tags are dropped, their text kept", () => {
    expect(speakable("Press <kbd>Esc</kbd> to stop.")).toBe("Press Esc to stop.");
  });
  test("a long answer is cut at a sentence and says the rest is on screen", () => {
    const md = Array.from({ length: 200 }, (_, i) => `Sentence number ${i} is here.`).join(" ");
    const out = speakable(md, 300);
    expect(out.length).toBeLessThanOrEqual(340);
    expect(out.endsWith("The rest is on screen.")).toBe(true);
    expect(out).toMatch(/here\. The rest is on screen\.$/);
  });
  test("nothing speakable gives an empty string", () => {
    expect(speakable("```\ncode\n```").trim()).toBe("Code block omitted.");
    expect(speakable("   \n\n ")).toBe("");
  });
});

describe("speechChunks", () => {
  test("short text is one chunk", () => {
    expect(speechChunks("Hello there.")).toEqual(["Hello there."]);
  });
  test("long text splits at sentence ends, each under the cap", () => {
    const text = "One two three. Four five six! Seven eight nine? Ten eleven twelve.";
    const chunks = speechChunks(text, 30);
    expect(chunks.every((c) => c.length <= 30)).toBe(true);
    expect(chunks.join(" ")).toBe(text);
  });
  test("a sentence longer than the cap splits at spaces", () => {
    const text = "word ".repeat(40).trim();
    const chunks = speechChunks(text, 50);
    expect(chunks.every((c) => c.length <= 50)).toBe(true);
    expect(chunks.join(" ")).toBe(text);
  });
  test("empty text has no chunks", () => {
    expect(speechChunks("  ")).toEqual([]);
  });
});

describe("dictation text", () => {
  test("results join into one transcript, final and interim alike", () => {
    expect(heardText([{ text: "hello", final: true }, { text: " world", final: false }])).toBe("hello world");
  });
  test("results already used are skipped", () => {
    expect(heardText([{ text: "old", final: true }, { text: " new", final: true }], { i: 1, words: 0 })).toBe("new");
    expect(heardText([{ text: "old", final: true }, { text: " new", final: true }], markEnd([{ text: "old", final: true }]))).toBe("new");
  });
  test("WebKit's one growing result: words said after an echo still count", () => {
    // Safari and WKWebView report a single interim result whose transcript grows.
    const echo = [{ text: "Voice mode on", final: false }];
    const used = markEnd(echo);
    expect(heardText(echo, used)).toBe("");
    expect(heardText([{ text: "Voice mode on fix the tests", final: false }], used)).toBe("fix the tests");
    expect(heardText([{ text: "Voice mode on. Fix the tests.", final: true }], used)).toBe("Fix the tests.");
  });
  test("a final result is used up whole", () => {
    expect(markEnd([{ text: "one two", final: true }])).toEqual({ i: 1, words: 0 });
    expect(markEnd([])).toEqual({ i: 0, words: 0 });
    expect(markEnd([{ text: "one", final: true }, { text: " two three ", final: false }])).toEqual({ i: 1, words: 2 });
  });
  test("dictation goes after what is in the box, a space apart", () => {
    expect(joinDictation("", "fix the test")).toBe("fix the test");
    expect(joinDictation("Please", "fix the test")).toBe("Please fix the test");
    expect(joinDictation("Line one\n", "two")).toBe("Line one\ntwo");
    expect(joinDictation("Typed ", "spoken")).toBe("Typed spoken");
    expect(joinDictation("Typed", "")).toBe("Typed");
  });
});

describe("echo and stop words", () => {
  test("hearing the answer being spoken is an echo", () => {
    expect(isEcho("all tests pass the build", "All tests pass. The build is green.")).toBe(true);
  });
  test("the user's own words are not", () => {
    expect(isEcho("wait run the linter first", "All tests pass. The build is green.")).toBe(false);
  });
  test("nothing heard is no barge-in either", () => {
    expect(isEcho("  ", "anything")).toBe(true);
  });
  test("stop words end the speech instead of becoming a message", () => {
    expect(isStopWord("Stop")).toBe(true);
    expect(isStopWord(" stop. ")).toBe(true);
    expect(isStopWord("okay stop")).toBe(true);
    expect(isStopWord("be quiet")).toBe(true);
    expect(isStopWord("stop the server and restart it")).toBe(false);
  });
});

describe("recognitionProblem", () => {
  test("a blocked microphone says how to allow it", () => {
    expect(recognitionProblem("not-allowed")).toMatch(/blocked/i);
    expect(recognitionProblem("service-not-allowed")).toMatch(/not allowed|blocked/i);
  });
  test("no microphone and no network are named", () => {
    expect(recognitionProblem("audio-capture")).toMatch(/microphone/i);
    expect(recognitionProblem("network")).toMatch(/network/i);
  });
  test("silence and our own abort are not problems", () => {
    expect(recognitionProblem("no-speech")).toBeNull();
    expect(recognitionProblem("aborted")).toBeNull();
  });
  test("anything else still says something", () => {
    expect(recognitionProblem("language-not-supported")).toMatch(/language-not-supported/);
  });
});

const user = (text: string, key = "u"): Item => ({ kind: "user", key, text });
const answer = (text: string, key = "a"): Item => ({ kind: "assistant", key, itemId: key, text, streaming: false });
const turn = (status: string, key = "t"): Item => ({ kind: "turn", key, start: 1, end: 2, status });

describe("voiceCue", () => {
  // Read from the session's items, not its status: a quick turn can go
  // running and idle within one render, and must still be spoken.
  const cue = (items: Item[]) => voiceCue(items)?.cue ?? null;
  test("a finished turn speaks its last answer", () => {
    const items = [user("old", "u0"), answer("Old answer", "a0"), turn("completed", "t0"), user("hi"), turn("completed"), answer("First."), answer("**Final** answer.", "a2")];
    expect(cue(items)).toEqual({ kind: "answer", text: "Final answer." });
  });
  test("each ending is told apart by its turn", () => {
    expect(voiceCue([user("hi"), turn("completed", "t1")])?.key).not.toBe(voiceCue([user("hi"), turn("completed", "t2")])?.key);
  });
  test("a turn with no answer just says done", () => {
    expect(cue([user("hi"), turn("completed")])).toEqual({ kind: "answer", text: "Done." });
  });
  test("a running turn, or a prompt not started yet, says nothing", () => {
    expect(cue([user("hi"), turn("running"), answer("partial")])).toBeNull();
    expect(cue([user("a", "u0"), turn("completed", "t0"), answer("Earlier"), user("b")])).toBeNull();
    expect(cue([])).toBeNull();
  });
  test("an approval is announced with what it wants to run", () => {
    const ask: Item = { kind: "approval", key: "p", requestId: "r", approvalKind: "command", title: "echo approved", detail: "needs to run a command", decision: null };
    expect(cue([user("hi"), turn("running"), ask])).toEqual({ kind: "approval", text: "Needs your approval: echo approved: needs to run a command" });
    expect(cue([user("hi"), turn("running"), { ...ask, decision: "allow" } as Item])).toBeNull();
  });
  test("a failed turn says why", () => {
    const items: Item[] = [user("hi"), turn("failed"), { kind: "error", key: "e", message: "fake failure: the model is unavailable" }];
    expect(cue(items)).toEqual({ kind: "failed", text: "The turn failed: fake failure: the model is unavailable" });
  });
  test("a turn the user stopped is silent but hands back the mic", () => {
    expect(cue([user("hi"), turn("interrupted")])).toEqual({ kind: "stopped", text: "" });
  });
});

const run = (inputs: Parameters<typeof voiceStep>[1][], from: VoiceMode = VOICE_OFF) => {
  let mode = from;
  const effects: string[] = [];
  for (const input of inputs) {
    const step = voiceStep(mode, input);
    mode = step.mode;
    effects.push(...step.effects.map((e) => (e.do === "speak" ? `speak:${e.text}` : e.do)));
  }
  return { mode, effects };
};

describe("voiceStep", () => {
  test("turning on says so while already listening for a barge-in", () => {
    const { mode, effects } = run([{ type: "on" }]);
    expect(mode.phase).toBe("speaking");
    expect(effects).toEqual(["listen", "speak:Voice mode on."]);
  });
  test("after the greeting it listens", () => {
    expect(run([{ type: "on" }, { type: "spoken" }]).mode.phase).toBe("listening");
  });
  test("speech arms the silence timer, every new word re-arms it", () => {
    const { effects } = run([{ type: "on" }, { type: "spoken" }, { type: "heard" }, { type: "heard" }]);
    expect(effects.slice(-2)).toEqual(["arm", "arm"]);
  });
  test("silence with words sends; without words it keeps listening", () => {
    const listening = run([{ type: "on" }, { type: "spoken" }]).mode;
    expect(run([{ type: "silence", hasText: false }], listening)).toEqual({ mode: listening, effects: [] });
    const sent = run([{ type: "silence", hasText: true }], listening);
    expect(sent.effects).toEqual(["disarm", "send"]);
  });
  test("once sent it stops listening and waits for the agent", () => {
    const listening = run([{ type: "on" }, { type: "spoken" }]).mode;
    const { mode, effects } = run([{ type: "sent" }], listening);
    expect(mode.phase).toBe("waiting");
    expect(effects).toEqual(["disarm", "unlisten"]);
  });
  test("the answer is spoken, then it listens again", () => {
    const waiting = run([{ type: "on" }, { type: "spoken" }, { type: "sent" }]).mode;
    const spoken = run([{ type: "cue", cue: { kind: "answer", text: "All done." } }], waiting);
    expect(spoken.mode.phase).toBe("speaking");
    expect(spoken.effects).toEqual(["listen", "speak:All done."]);
    expect(run([{ type: "spoken" }], spoken.mode).mode.phase).toBe("listening");
  });
  test("an approval is announced and then it waits: never listens for a yes", () => {
    const waiting = run([{ type: "on" }, { type: "spoken" }, { type: "sent" }]).mode;
    const { mode, effects } = run([{ type: "cue", cue: { kind: "approval", text: "Needs your approval: x" } }, { type: "spoken" }], waiting);
    expect(mode.phase).toBe("waiting");
    expect(effects).toEqual(["listen", "speak:Needs your approval: x", "unlisten"]);
  });
  test("a stopped turn goes straight back to listening", () => {
    const waiting = run([{ type: "on" }, { type: "spoken" }, { type: "sent" }]).mode;
    const { mode, effects } = run([{ type: "cue", cue: { kind: "stopped", text: "" } }], waiting);
    expect(mode.phase).toBe("listening");
    expect(effects).toEqual(["listen"]);
  });
  test("the user talking over the answer silences it and is heard", () => {
    const speaking = run([{ type: "cue", cue: { kind: "answer", text: "Long answer." } }], run([{ type: "on" }, { type: "spoken" }, { type: "sent" }]).mode).mode;
    const { mode, effects } = run([{ type: "heard" }], speaking);
    expect(mode.phase).toBe("listening");
    expect(effects).toEqual(["hush", "arm"]);
  });
  test("Stop silences the answer and listens", () => {
    const speaking = run([{ type: "on" }]).mode;
    const { mode, effects } = run([{ type: "stop" }], speaking);
    expect(mode.phase).toBe("listening");
    expect(effects).toEqual(["hush"]);
  });
  test("sending by hand while it speaks silences it", () => {
    const speaking = run([{ type: "on" }]).mode;
    expect(run([{ type: "sent" }], speaking).effects).toEqual(["hush", "disarm", "unlisten"]);
  });
  test("turning off stops everything, from any phase", () => {
    for (const from of [run([{ type: "on" }]).mode, run([{ type: "on" }, { type: "spoken" }]).mode, run([{ type: "on" }, { type: "spoken" }, { type: "sent" }]).mode]) {
      const { mode, effects } = run([{ type: "off" }], from);
      expect(mode).toEqual(VOICE_OFF);
      expect(effects).toEqual(["disarm", "hush", "unlisten"]);
    }
  });
  test("a recognition failure turns it off", () => {
    expect(run([{ type: "on" }, { type: "failed" }]).mode).toEqual(VOICE_OFF);
  });
  test("while off, nothing reacts", () => {
    for (const input of [{ type: "heard" }, { type: "silence", hasText: true }, { type: "sent" }, { type: "spoken" }, { type: "stop" }, { type: "cue", cue: { kind: "answer", text: "x" } }] as const) {
      expect(run([input])).toEqual({ mode: VOICE_OFF, effects: [] });
    }
  });
  test("the silence wait is about a second and a half", () => {
    expect(SILENCE_MS).toBe(1500);
  });
});
