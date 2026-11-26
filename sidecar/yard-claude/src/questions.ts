// Claude's AskUserQuestion. Claude Code asks the host through the permission
// callback and reads the answers from the input the host gives back
// (`answers`: question text → the chosen label, labels joined with ", ", or
// what the user typed). Allowed without them, Claude hears "The user did not
// answer the questions."

export const ASK_TOOL = "AskUserQuestion";

export type Question = {
  question: string;
  header: string;
  multiSelect: boolean;
  options: { label: string; description: string }[];
};

const text = (v: unknown, max: number) => (typeof v === "string" ? v.slice(0, max) : "");

/** The questions as the UI shows them. Anything malformed is left out. */
export function questionsOf(input: any): Question[] {
  const list = Array.isArray(input?.questions) ? input.questions : [];
  return list
    .slice(0, 4)
    .map((q: any) => ({
      // Not cut: the answers are keyed by it.
      question: text(q?.question, Infinity),
      header: text(q?.header, 40),
      multiSelect: q?.multiSelect === true,
      options: (Array.isArray(q?.options) ? q.options : [])
        .slice(0, 8)
        .map((o: any) => ({ label: text(o?.label, 200), description: text(o?.description, 1000) }))
        .filter((o: { label: string }) => o.label),
    }))
    .filter((q: Question) => q.question);
}

/** The request yardd shows as a question card instead of "Allow this?". */
export function questionRequest(requestId: string, input: any, itemId?: string | null) {
  const questions = questionsOf(input);
  return {
    type: "approval_request",
    requestId,
    kind: "question",
    // Notifications, the hand-over summary and Slack's "needs your approval" read this.
    title: questions[0]?.question ?? "Claude has a question",
    detail: questions.length > 1 ? `and ${questions.length - 1} more` : "",
    questions,
    ...(itemId ? { itemId } : {}),
  };
}

/** The tool's input with the user's answers, only for questions it asked. */
export function answeredInput(input: any, answers: unknown): Record<string, unknown> {
  const asked = new Set(questionsOf(input).map((q) => q.question));
  const given: Record<string, string> = {};
  if (answers && typeof answers === "object") {
    for (const [q, a] of Object.entries(answers)) if (asked.has(q) && typeof a === "string" && a.trim()) given[q] = a;
  }
  return { ...input, answers: given };
}
