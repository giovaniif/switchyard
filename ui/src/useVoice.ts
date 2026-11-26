// The browser side of voice (see voice.ts for the rules): the Web Speech
// API's recognition for the microphone, speechSynthesis for reading answers.
// Both are looked up when used, never cached, so the page follows what the
// browser offers (and tests can stand in for them).

import { useEffect, useRef, useState } from "react";
import {
  heardText,
  isEcho,
  isStopWord,
  joinDictation,
  markEnd,
  recognitionProblem,
  SILENCE_MS,
  START,
  speechChunks,
  VOICE_OFF,
  voiceStep,
  type Cue,
  type Heard,
  type Mark,
  type VoiceEffect,
  type VoiceInput,
  type VoiceMode,
} from "./voice";

const recognitionClass = (): (new () => any) | undefined => (window as any).SpeechRecognition ?? (window as any).webkitSpeechRecognition;
const synthesis = (): SpeechSynthesis | undefined => (window as any).speechSynthesis;

/** Why the microphone cannot be used here, or null when it can. */
export function micUnavailable(): string | null {
  if (!recognitionClass()) return "Voice input is not available in this browser. Chrome, Edge and Safari have it.";
  if (!window.isSecureContext) return "Voice input needs a secure page (https or this computer).";
  return null;
}

/** A recognition that keeps ending right after it starts gives up after
 *  this many in a row. One that ran a while (a quiet minute) just restarts. */
const MAX_RESTARTS = 5;
const QUICK_END_MS = 1500;

export interface VoiceOptions {
  text(): string;
  setText(text: string): void;
  /** Send what is in the box; the composer calls `sent()` when it went. */
  submit(): void;
  /** The latest thing the session has to say, numbered so a repeat still counts. */
  cue?: { n: number; cue: Cue } | null;
  /** Voice mode turns off when this changes (another session). */
  scope?: string;
  /** The new-session page: a task voice mode sent goes on in its session. */
  carry?: boolean;
}

// The new-session page unmounts as the session it started opens. Voice mode
// waiting there for the answer goes on in the next reply box that mounts,
// so the answer is read and it listens again, as in a session.
const CARRY_MS = 30_000;
let carriedAt = 0;

export function useVoice(opts: VoiceOptions) {
  const o = useRef(opts);
  o.current = opts;
  const [dictating, setDictating] = useState(false);
  const [mode, setMode] = useState<VoiceMode>(VOICE_OFF);
  const [error, setError] = useState<string | null>(null);
  // Checked on every render: cheap, and it follows the browser.
  const unavailable = micUnavailable();

  const modeRef = useRef<VoiceMode>(VOICE_OFF);
  const want = useRef<"none" | "dictate" | "voice">("none");
  const rec = useRef<any>(null);
  // Results of the recognition that is stopping still land (its last words).
  const stopping = useRef<any>(null);
  // The box held `base` when this recognition started; results before `from` are used up.
  const base = useRef("");
  const from = useRef<Mark>(START);
  const results = useRef<Heard[]>([]);
  const restarts = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const restartTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // What is being read aloud, to tell an echo from the user talking over it.
  const speaking = useRef("");
  const speechGen = useRef(0);
  const utterances = useRef<SpeechSynthesisUtterance[]>([]);

  const dispatch = (input: VoiceInput) => {
    const step = voiceStep(modeRef.current, input);
    modeRef.current = step.mode;
    setMode(step.mode);
    step.effects.forEach(run);
  };

  const run = (effect: VoiceEffect) => {
    switch (effect.do) {
      case "listen":
        want.current = "voice";
        listen();
        return;
      case "unlisten":
        if (want.current === "voice") want.current = "none";
        unlisten(true);
        return;
      case "speak":
        say(effect.text);
        return;
      case "hush":
        hush();
        return;
      case "arm":
        if (timer.current) clearTimeout(timer.current);
        timer.current = setTimeout(() => {
          timer.current = null;
          dispatch({ type: "silence", hasText: o.current.text().trim() !== "" });
        }, SILENCE_MS);
        return;
      case "disarm":
        if (timer.current) clearTimeout(timer.current);
        timer.current = null;
        return;
      case "send":
        o.current.submit();
        return;
    }
  };

  const onResults = (list: Heard[]) => {
    results.current = list;
    const spoken = heardText(list, from.current);
    if (!spoken) return;
    const phase = modeRef.current.phase;
    if (want.current === "voice" && phase === "speaking") {
      if (isEcho(spoken, speaking.current)) {
        from.current = markEnd(list);
        return;
      }
      if (isStopWord(spoken)) {
        // Wait for the final words: "stop" may go on to be a real request.
        if (list.slice(from.current.i).every((r) => r.final)) {
          from.current = markEnd(list);
          dispatch({ type: "stop" });
        }
        return;
      }
      base.current = o.current.text();
      dispatch({ type: "heard" });
    } else if (want.current === "voice") {
      if (phase !== "listening") return;
      dispatch({ type: "heard" });
    }
    o.current.setText(joinDictation(base.current, spoken));
  };

  const fail = (problem: string) => {
    setError(problem);
    if (want.current === "voice") dispatch({ type: "failed" });
    want.current = "none";
    setDictating(false);
    unlisten(true);
  };

  const listen = () => {
    if (rec.current) return;
    const Rec = recognitionClass();
    if (!Rec) return fail(micUnavailable() ?? "Voice input is not available.");
    let r: any;
    try {
      r = new Rec();
      r.continuous = true;
      r.interimResults = true;
    } catch (e) {
      return fail(`Voice input could not start: ${String((e as Error)?.message ?? e)}`);
    }
    rec.current = r;
    const startedAt = Date.now();
    base.current = o.current.text();
    from.current = START;
    results.current = [];
    r.onresult = (e: any) => {
      if (r !== rec.current && r !== stopping.current) return;
      const list: Heard[] = Array.from(e.results as ArrayLike<any>, (res: any) => ({ text: String(res?.[0]?.transcript ?? ""), final: !!res?.isFinal }));
      onResults(list);
    };
    r.onerror = (e: any) => {
      if (r !== rec.current) return;
      const problem = recognitionProblem(String(e?.error ?? "unknown"));
      if (problem) fail(problem);
    };
    r.onend = () => {
      if (stopping.current === r) stopping.current = null;
      if (r !== rec.current) return;
      rec.current = null;
      // Recognition ends on its own after a pause (Safari) or a minute
      // (Chrome); keep the microphone open for as long as it is wanted.
      if (want.current === "none") {
        setDictating(false);
        return;
      }
      restarts.current = Date.now() - startedAt < QUICK_END_MS ? restarts.current + 1 : 0;
      if (restarts.current > MAX_RESTARTS) return fail("Voice input keeps stopping. Try again.");
      restartTimer.current = setTimeout(() => {
        restartTimer.current = null;
        if (want.current !== "none") listen();
      }, 250);
    };
    try {
      r.start();
      setError(null);
    } catch (e) {
      rec.current = null;
      fail(`Voice input could not start: ${String((e as Error)?.message ?? e)}`);
    }
  };

  /** `abort` drops words not yet final (voice mode, after sending); a plain
   *  stop lets the last words arrive (dictation). */
  const unlisten = (abort: boolean) => {
    if (restartTimer.current) clearTimeout(restartTimer.current);
    restartTimer.current = null;
    const r = rec.current;
    rec.current = null;
    if (!r) return;
    try {
      if (abort) r.abort();
      else {
        stopping.current = r;
        r.stop();
      }
    } catch {
      // Already ended.
    }
  };

  const say = (text: string) => {
    hush();
    const gen = speechGen.current;
    const synth = synthesis();
    const Utterance = (window as any).SpeechSynthesisUtterance;
    const chunks = speechChunks(text);
    speaking.current = text;
    const done = () => {
      if (gen === speechGen.current) dispatch({ type: "spoken" });
    };
    if (!synth || !Utterance || chunks.length === 0) {
      setTimeout(done, 0);
      return;
    }
    utterances.current = chunks.map((chunk, i) => {
      const u: SpeechSynthesisUtterance = new Utterance(chunk);
      if (i === chunks.length - 1) u.onend = done;
      // A cancel reports "interrupted"/"canceled": that is ours, not an end.
      u.onerror = (e: any) => {
        if (e?.error !== "interrupted" && e?.error !== "canceled") done();
      };
      return u;
    });
    utterances.current.forEach((u) => synth.speak(u));
  };

  const hush = () => {
    speechGen.current++;
    speaking.current = "";
    utterances.current = [];
    try {
      synthesis()?.cancel();
    } catch {
      // Nothing was speaking.
    }
  };

  const stopAll = () => {
    want.current = "none";
    if (timer.current) clearTimeout(timer.current);
    timer.current = null;
    unlisten(true);
    hush();
  };

  // Another session, or the composer going away, ends it all.
  useEffect(() => {
    if (!o.current.carry && carriedAt && Date.now() - carriedAt < CARRY_MS) {
      modeRef.current = { phase: "waiting", after: "listen" };
      setMode(modeRef.current);
    }
    carriedAt = 0;
    return () => {
      if (o.current.carry && modeRef.current.phase === "waiting") carriedAt = Date.now();
      stopAll();
      modeRef.current = VOICE_OFF;
      setMode(VOICE_OFF);
      setDictating(false);
    };
  }, [opts.scope]);

  // What the session has to say, while voice mode is on.
  const lastCue = useRef(opts.cue?.n ?? 0);
  useEffect(() => {
    const pause = () => {
      stopAll();
      modeRef.current = VOICE_OFF;
      setMode(VOICE_OFF);
      setDictating(false);
    };
    const hidden = () => { if (document.hidden) pause(); };
    window.addEventListener("yard:pause", pause);
    document.addEventListener("visibilitychange", hidden);
    return () => {
      window.removeEventListener("yard:pause", pause);
      document.removeEventListener("visibilitychange", hidden);
    };
  }, []);
  useEffect(() => {
    const c = opts.cue;
    if (!c || c.n === lastCue.current) return;
    lastCue.current = c.n;
    if (modeRef.current.phase !== "off") dispatch({ type: "cue", cue: c.cue });
  }, [opts.cue?.n]);

  const startDictation = () => {
    setError(null);
    const why = micUnavailable();
    if (why) return setError(why);
    want.current = "dictate";
    restarts.current = 0;
    setDictating(true);
    listen();
  };
  const stopDictation = () => {
    if (want.current === "dictate") want.current = "none";
    setDictating(false);
    unlisten(false);
  };

  const voiceOn = mode.phase !== "off";
  const toggleVoice = () => {
    setError(null);
    if (voiceOn) return dispatch({ type: "off" });
    const why = micUnavailable();
    if (why) return setError(why);
    if (dictating) stopDictation();
    restarts.current = 0;
    dispatch({ type: "on" });
  };

  // Escape stops dictation, then the answer being read, then voice mode.
  useEffect(() => {
    if (!dictating && !voiceOn) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || e.defaultPrevented) return;
      if (dictating) stopDictation();
      else if (modeRef.current.phase === "speaking") dispatch({ type: "stop" });
      else dispatch({ type: "off" });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dictating, voiceOn]);

  return {
    unavailable,
    error,
    dictating,
    toggleDictation: () => (dictating ? stopDictation() : startDictation()),
    mode,
    canSpeak: !!synthesis(),
    toggleVoice,
    stopSpeaking: () => dispatch({ type: "stop" }),
    /** The composer sent a message. */
    sent: () => {
      if (modeRef.current.phase !== "off") return dispatch({ type: "sent" });
      // Words still on their way would refill the emptied box.
      if (want.current === "dictate") want.current = "none";
      setDictating(false);
      stopping.current = null;
      unlisten(true);
    },
    /** The user typed: dictation carries on after their text. */
    typed: (text: string) => {
      base.current = text;
      from.current = markEnd(results.current);
    },
  };
}
