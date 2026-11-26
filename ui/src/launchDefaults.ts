import type { Choice } from "./sessionDefaults";
import { ENGINES } from "./engines";

/** Switchy had its own defaults until the host agent replaced it; the agent keeps its settings in yardd. */
export type DefaultTarget = "sessions";
export interface LaunchDefaults {
  host: string;
  choice: Choice;
}
const key = (target: DefaultTarget) => `yard.defaults.${target}`;

export function loadDefaults(target: DefaultTarget): LaunchDefaults | null {
  try {
    const saved = JSON.parse(localStorage.getItem(key(target)) ?? "null");
    const c = saved?.choice;
    if (
      typeof saved?.host !== "string" ||
      !saved.host ||
      !c ||
      !ENGINES.includes(c.engine) ||
      typeof c.model !== "string" ||
      !c.model ||
      typeof c.effort !== "string"
    )
      return null;
    if (target === "sessions" && saved.host === "all") return null;
    return {
      host: saved.host,
      choice: { engine: c.engine, model: c.model, effort: c.effort },
    };
  } catch {
    return null;
  }
}

export function saveDefaults(
  target: DefaultTarget,
  value: LaunchDefaults | null,
): boolean {
  try {
    if (value) localStorage.setItem(key(target), JSON.stringify(value));
    else localStorage.removeItem(key(target));
    return true;
  } catch {
    return false;
  }
}
