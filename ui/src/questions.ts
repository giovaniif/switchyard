import type { Question } from "./protocol";

/** What the user picked for one question: option labels, and typed text for Other. */
export interface Pick {
  labels: string[];
  other: string;
}

export const noPick: Pick = { labels: [], other: "" };

/** Picking an option. One choice: it replaces the pick and clears Other. Several: it toggles. */
export function toggle(q: Question, pick: Pick, label: string): Pick {
  if (!q.multiSelect) return { labels: [label], other: "" };
  return { ...pick, labels: pick.labels.includes(label) ? pick.labels.filter((l) => l !== label) : [...pick.labels, label] };
}

/** Typing Other. One choice: it replaces a picked option. */
export function typeOther(q: Question, pick: Pick, other: string): Pick {
  return q.multiSelect ? { ...pick, other } : { labels: other.trim() ? [] : pick.labels, other };
}

/** The answer Claude Code reads: the label, labels joined with ", " in the
 *  options' order, or the typed text (last, after any labels). "" when none. */
export function answerOf(q: Question, pick: Pick = noPick): string {
  const other = pick.other.trim();
  if (!q.multiSelect) return other || pick.labels[0] || "";
  const labels = q.options.map((o) => o.label).filter((l) => pick.labels.includes(l));
  return [...labels, other].filter(Boolean).join(", ");
}

/** Every question's answer, or null while one has none. */
export function answersFrom(questions: Question[], picks: Record<string, Pick>): Record<string, string> | null {
  const answers: Record<string, string> = {};
  for (const q of questions) {
    const a = answerOf(q, picks[q.question]);
    if (!a) return null;
    answers[q.question] = a;
  }
  return answers;
}
