// Voice in the composer: dictation, and a hands-free mode that sends after a
// pause and reads the answer aloud. Everything here is pure so it can be
// tested; the browser's speech APIs are driven from Voice.tsx.

import type { Item } from "./protocol";
import { plainLine } from "./notify";

/** How long a pause in speech has to be before voice mode sends. */
export const SILENCE_MS = 1500;

/** Longest answer read aloud; the rest stays on screen. */
export const SPEAK_MAX = 2500;

const CODE = "Code block omitted.";
const TABLE = "Table omitted.";
const ENDS_SENTENCE = /[.!?:;…]["')\]]*$/;
const FENCE = /^\s{0,3}(```|~~~)/;
const TABLE_DELIMITER = /^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)+\|?\s*$/;
const HEADING = /^\s{0,3}#{1,6}\s+(.*?)(\s+#+)?\s*$/;
const LIST_ITEM = /^\s*(?:[-*+]|\d{1,9}[.)])\s+(.*)$/;
const QUOTE = /^\s{0,3}>\s?(.*)$/;

const sentence = (s: string) => (s && !ENDS_SENTENCE.test(s) ? `${s}.` : s);

/** One line of Markdown as words: links read as their text, a bare URL as
 *  "a link", code and emphasis marks dropped. */
function inlineSpeech(line: string): string {
  return line
    .replace(/!\[[^\]]*\]\([^)]*\)/g, "")
    .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1")
    .replace(/<?https?:\/\/[^\s>)]+>?/g, "a link")
    .replace(/`([^`]*)`/g, "$1")
    .replace(/<\/?[a-zA-Z][^>]*>/g, "")
    .replace(/\*\*|__|~~/g, "")
    .replace(/(^|[\s(])[*_](?=\S)/g, "$1")
    .replace(/(\S)[*_](?=[\s).,!?:;]|$)/g, "$1");
}

/** An agent's Markdown answer as text a speech engine can read: prose only.
 *  Code blocks and tables are named, not read; a long answer is cut at a
 *  sentence and says the rest is on screen. */
export function speakable(markdown: string, max = SPEAK_MAX): string {
  const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
  const parts: string[] = [];
  // Text of the paragraph being read; a block marker or a blank line ends it.
  let para: string[] = [];
  const endPara = () => {
    const text = para.join(" ").trim();
    if (text) parts.push(sentence(text));
    para = [];
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    if (FENCE.test(line)) {
      endPara();
      i++;
      while (i < lines.length && !FENCE.test(lines[i])) i++;
      parts.push(CODE);
      continue;
    }
    if (line.includes("|") && TABLE_DELIMITER.test(lines[i + 1] ?? "")) {
      endPara();
      i += 2;
      while (i < lines.length && lines[i].includes("|") && lines[i].trim()) i++;
      i--;
      parts.push(TABLE);
      continue;
    }
    if (!line.trim()) {
      endPara();
      continue;
    }
    const heading = line.match(HEADING);
    const item = !heading && line.match(LIST_ITEM);
    if (heading || item) {
      endPara();
      const text = inlineSpeech((heading ?? item)![1]).trim();
      if (text) parts.push(sentence(text));
      continue;
    }
    const quote = line.match(QUOTE);
    para.push(inlineSpeech(quote ? quote[1] : line).trim());
  }
  // The last paragraph keeps its own ending: it may be a fragment on purpose.
  const tail = para.join(" ").trim();
  if (tail) parts.push(tail);
  const text = parts.join(" ").replace(/\s+/g, " ").trim();
  return cap(text, max);
}

function cap(text: string, max: number): string {
  if (text.length <= max) return text;
  const head = text.slice(0, max);
  const end = Math.max(head.lastIndexOf(". "), head.lastIndexOf("! "), head.lastIndexOf("? "));
  const cut = end > max / 2 ? head.slice(0, end + 1) : head.slice(0, head.lastIndexOf(" ")).trimEnd() + "…";
  return `${cut} The rest is on screen.`;
}

/** Text in pieces a speech engine reads reliably: Chrome stops a long
 *  utterance after about 15 seconds. Splits at sentence ends, then spaces. */
export function speechChunks(text: string, max = 200): string[] {
  const sentences = text.match(/[^.!?]+(?:[.!?]+["')\]]*|$)/g)?.map((s) => s.trim()).filter(Boolean) ?? [];
  const pieces = sentences.flatMap((s) => {
    if (s.length <= max) return [s];
    const out: string[] = [];
    let cur = "";
    for (const word of s.split(/\s+/)) {
      if (cur && cur.length + 1 + word.length > max) {
        out.push(cur);
        cur = word;
      } else cur = cur ? `${cur} ${word}` : word;
    }
    if (cur) out.push(cur);
    return out;
  });
  const chunks: string[] = [];
  for (const p of pieces) {
    const last = chunks[chunks.length - 1];
    if (last !== undefined && last.length + 1 + p.length <= max) chunks[chunks.length - 1] = `${last} ${p}`;
    else chunks.push(p);
  }
  return chunks;
}

/** A speech recognition result, as the browser reports it. */
export interface Heard {
  text: string;
  final: boolean;
}

/** How much of the results is used up: every result before `i`, and the
 *  first `words` words of result `i`. Chrome gives each phrase its own
 *  result, but WebKit (Safari, the desktop and iOS apps) keeps one result
 *  whose transcript grows for as long as it listens, so an index alone would
 *  skip everything said after an echo. */
export interface Mark {
  i: number;
  words: number;
}

export const START: Mark = { i: 0, words: 0 };

const spokenWords = (text: string) => text.trim().split(/\s+/).filter(Boolean);

/** What has been said since `from`, interim words included. */
export function heardText(results: Heard[], from: Mark = START): string {
  const head = results[from.i] ? spokenWords(results[from.i].text).slice(from.words).join(" ") : "";
  const rest = results.slice(from.i + 1).map((r) => r.text.trim());
  return [head, ...rest].filter(Boolean).join(" ");
}

/** Everything heard so far, used up. A result still growing is marked by its
 *  words, so the words it gains later still count. */
export function markEnd(results: Heard[]): Mark {
  const last = results[results.length - 1];
  if (!last || last.final) return { i: results.length, words: 0 };
  return { i: results.length - 1, words: spokenWords(last.text).length };
}

/** Dictated words go after what is already in the box. */
export function joinDictation(base: string, spoken: string): string {
  if (!spoken) return base;
  if (!base || /\s$/.test(base)) return base + spoken;
  return `${base} ${spoken}`;
}

const words = (s: string) =>
  s
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s']/gu, " ")
    .split(/\s+/)
    .filter(Boolean);

/** Whether what the microphone picked up is the answer coming out of the
 *  speakers rather than the user: most of its words are in the answer. */
export function isEcho(heard: string, speaking: string): boolean {
  const got = words(heard);
  if (got.length === 0) return true;
  const said = new Set(words(speaking));
  return got.filter((w) => said.has(w)).length / got.length >= 0.7;
}

/** "Stop" said over an answer ends the speech; it is not a message. */
export function isStopWord(text: string): boolean {
  return /^(?:ok(?:ay)? )?(?:stop(?: it| talking| speaking)?|quiet|be quiet|silence|shh+|hush|cancel|enough)$/.test(words(text).join(" "));
}

/** What to tell the user when recognition fails, or null when it is not a
 *  failure (a pause with no speech, or our own stop). */
export function recognitionProblem(code: string): string | null {
  switch (code) {
    case "no-speech":
    case "aborted":
      return null;
    case "not-allowed":
      return "The microphone is blocked. Allow it in the site's settings, or for Switchyard in System Settings › Privacy & Security › Microphone.";
    case "service-not-allowed":
      return "Speech recognition is not allowed here. Allow it in System Settings › Privacy & Security › Speech Recognition, or try another browser.";
    case "audio-capture":
      return "No microphone was found.";
    case "network":
      return "Speech recognition needs a network connection.";
    default:
      return `Voice input stopped (${code}).`;
  }
}

export type CueKind = "answer" | "approval" | "failed" | "stopped";

/** What voice mode says when a session changes: its answer, an approval it
 *  waits for, a failure. A turn the user stopped says nothing. */
export interface Cue {
  kind: CueKind;
  text: string;
}

/** The current turn starts after the last prompt; a message steered in
 *  mid-turn does not start one. */
function turnStart(items: Item[]): number {
  for (let i = items.length - 1; i >= 0; i--) {
    const it = items[i];
    if (it.kind === "user" && !it.steered) return i + 1;
  }
  return 0;
}

function failure(turn: Item[]): Cue {
  const err = [...turn].reverse().find((it) => it.kind === "error");
  const why = err?.kind === "error" ? plainLine(err.message, 200) : "";
  return { kind: "failed", text: why ? `The turn failed: ${why}` : "The turn failed." };
}

/** What voice mode would say about the session as its items stand: an
 *  approval waiting, or how the current turn ended. `key` names that moment,
 *  so the page speaks when it changes. Read from the items rather than the
 *  status: a quick turn can go running and idle within one render. */
export function voiceCue(items: Item[]): { key: string; cue: Cue } | null {
  const turn = items.slice(turnStart(items));
  const ask = [...turn].reverse().find((it) => it.kind === "approval" && it.decision === null && !it.unanswered);
  if (ask?.kind === "approval") {
    const what = plainLine([ask.title, ask.detail].filter(Boolean).join(": "), 200);
    return { key: `approval:${ask.key}`, cue: { kind: "approval", text: what ? `Needs your approval: ${what}` : "Needs your approval." } };
  }
  const end = [...turn].reverse().find((it) => it.kind === "turn");
  if (end?.kind !== "turn" || end.status === "running") return null;
  const key = `turn:${end.key}:${end.status}`;
  if (end.status === "interrupted") return { key, cue: { kind: "stopped", text: "" } };
  if (end.status === "failed") return { key, cue: failure(turn) };
  const answer = [...turn].reverse().find((it) => it.kind === "assistant" && it.text.trim());
  const text = answer?.kind === "assistant" ? speakable(answer.text) : "";
  return { key, cue: { kind: "answer", text: text || "Done." } };
}

export type VoicePhase = "off" | "listening" | "waiting" | "speaking";

/** Hands-free mode. `after` is where speaking goes when it ends: back to
 *  listening, or to waiting when it announced an approval (which is answered
 *  on screen, never by voice). */
export interface VoiceMode {
  phase: VoicePhase;
  after: "listen" | "wait";
}

export const VOICE_OFF: VoiceMode = { phase: "off", after: "listen" };

export type VoiceInput =
  | { type: "on" }
  | { type: "off" }
  /** New words that are the user's, not an echo of the answer. */
  | { type: "heard" }
  | { type: "silence"; hasText: boolean }
  /** The composer sent a message (after the pause, or by hand). */
  | { type: "sent" }
  | { type: "cue"; cue: Cue }
  /** The speech engine finished (or dropped) what it was saying. */
  | { type: "spoken" }
  | { type: "stop" }
  /** Recognition cannot go on (blocked, no microphone). */
  | { type: "failed" };

export type VoiceEffect =
  | { do: "listen" }
  | { do: "unlisten" }
  | { do: "speak"; text: string }
  | { do: "hush" }
  | { do: "arm" }
  | { do: "disarm" }
  | { do: "send" };

const GREETING = "Voice mode on.";

/** One step of voice mode: the next mode, and what the page must do. The
 *  microphone stays on while an answer is read so the user can talk over
 *  it; it is off while the agent works. */
export function voiceStep(mode: VoiceMode, input: VoiceInput): { mode: VoiceMode; effects: VoiceEffect[] } {
  const same = { mode, effects: [] as VoiceEffect[] };
  const speak = (text: string, after: VoiceMode["after"]) => ({
    mode: { phase: "speaking" as const, after },
    effects: [...(mode.phase === "speaking" ? [{ do: "hush" } as const] : []), { do: "listen" } as const, { do: "speak", text } as const],
  });
  // Where speaking goes once it is over.
  const settle = (effects: VoiceEffect[]) =>
    mode.after === "wait"
      ? { mode: { phase: "waiting" as const, after: "listen" as const }, effects: [...effects, { do: "unlisten" } as const] }
      : { mode: { phase: "listening" as const, after: "listen" as const }, effects };

  if (input.type === "on") return mode.phase === "off" ? speak(GREETING, "listen") : same;
  if (mode.phase === "off") return same;
  switch (input.type) {
    case "off":
    case "failed":
      return { mode: VOICE_OFF, effects: [{ do: "disarm" }, { do: "hush" }, { do: "unlisten" }] };
    case "heard":
      if (mode.phase === "listening") return { mode, effects: [{ do: "arm" }] };
      if (mode.phase === "speaking") return { mode: { phase: "listening", after: "listen" }, effects: [{ do: "hush" }, { do: "arm" }] };
      return same;
    case "silence":
      return mode.phase === "listening" && input.hasText ? { mode, effects: [{ do: "disarm" }, { do: "send" }] } : same;
    case "sent":
      if (mode.phase === "waiting") return same;
      return {
        mode: { phase: "waiting", after: "listen" },
        effects: [...(mode.phase === "speaking" ? [{ do: "hush" } as const] : []), { do: "disarm" }, { do: "unlisten" }],
      };
    case "cue":
      if (input.cue.kind === "stopped") return mode.phase === "waiting" ? { mode: { phase: "listening", after: "listen" }, effects: [{ do: "listen" }] } : same;
      return speak(input.cue.text, input.cue.kind === "approval" ? "wait" : "listen");
    case "spoken":
      return mode.phase === "speaking" ? settle([]) : same;
    case "stop":
      return mode.phase === "speaking" ? settle([{ do: "hush" }]) : same;
  }
}
