// The engines Switchyard drives, in one place (yardd's twin is crates/yardd/src/engines.rs).
import type { Engine, Models } from "./protocol";

/** Every engine, in the order pickers and the Hosts page list them. */
export const ENGINES: Engine[] = ["claude", "codex", "opencode", "grok", "gemini"];

/** Short names: "Hand off to Codex". */
export const ENGINE_NAME: Record<Engine, string> = { claude: "Claude", codex: "Codex", opencode: "OpenCode", grok: "Grok", gemini: "Gemini" };

/** Product names, for logins and the Hosts page. */
export const ENGINE_LABEL: Record<Engine, string> = { claude: "Claude Code", codex: "Codex", opencode: "OpenCode", grok: "Grok", gemini: "Gemini" };

/** How to install each CLI on a host. */
export const ENGINE_INSTALL: Record<Engine, string> = {
  claude: "npm i -g @anthropic-ai/claude-code",
  codex: "npm i -g @openai/codex",
  opencode: "npm i -g opencode-ai",
  grok: "npm i -g @xai-official/grok",
  gemini: "npm i -g @google/gemini-cli",
};

/** How to log each CLI in from a terminal, for a host the app cannot reach yet. */
export const ENGINE_LOGIN: Record<Engine, string> = {
  claude: "claude setup-token",
  codex: "codex login --device-auth",
  opencode: "opencode auth login",
  grok: "grok login --device-auth",
  gemini: "gemini",
};

/** Where a device-code login signs in, for "sign in with your … account". */
export const ENGINE_ACCOUNT: Partial<Record<Engine, string>> = { codex: "ChatGPT", grok: "Grok (x.ai)", claude: "Claude", gemini: "Google" };

/** OpenCode runs on providers, each logged in on its own: its Log in adds one. */
export const addsProviders = (engine: Engine) => engine === "opencode";

/** Whether a message sent mid-turn joins the running turn at the agent's next
 *  step (yardd's `engines::can_steer`). The ACP engines take no input inside
 *  a turn, so yardd sends it as the next one. */
export const canSteer = (engine: Engine) => engine === "claude" || engine === "codex";

/** When a queued message reaches the agent, as the queue and the send button say it. */
export const queuedHint = (engine: Engine) => (canSteer(engine) ? "the agent reads it after its current step" : "sent when the current turn ends");

/** Claude and Codex are always offered, as before there were more; any other
 *  engine only once its host reports models for it (it is installed there). */
const ALWAYS: Engine[] = ["claude", "codex"];

/** The engines a model picker lists on a host, `first` leading. */
export function pickerEngines(models: Models, first?: Engine): Engine[] {
  const listed = ENGINES.filter((e) => ALWAYS.includes(e) || (models[e]?.length ?? 0) > 0);
  return first ? [first, ...listed.filter((e) => e !== first)] : listed;
}

/** Which engine reviews work done on `engine`: Claude and Codex review each
 *  other, any other engine gets Claude, else the first one with models. */
export function reviewEngine(engine: Engine, models: Models): Engine | null {
  const preferred: Engine = engine === "claude" ? "codex" : "claude";
  if (preferred !== engine && (models[preferred]?.length ?? 0) > 0) return preferred;
  return ENGINES.find((e) => e !== engine && (models[e]?.length ?? 0) > 0) ?? null;
}

/** The effort a session shows: none when its model takes no effort (some
 *  OpenCode, Grok and Gemini ones), whatever the choice still carries from
 *  the model picked before. Matches EffortPicker, which then has no chip. */
export function shownEffort(models: Models, engine: Engine, model: string, effort: string): string {
  const efforts = models[engine]?.find((m) => m.id === model)?.efforts ?? [];
  return efforts.length > 0 ? effort : "";
}
