// Claude Code's model list (`supportedModels()`) in the picker's shape, so
// yardd lists what the account's plan has instead of a list of its own.
import type { ModelInfo } from "@anthropic-ai/claude-agent-sdk";

export type PickerModel = { id: string; name: string; efforts: string[]; defaultEffort?: string; isDefault?: boolean };

export const EFFORTS = ["low", "medium", "high", "xhigh", "max"];

/** Opus and Fable start at high, as the built-in list always did; the smaller
 *  models at medium. */
function defaultEffort(id: string, efforts: string[]): string | undefined {
  const preferred = /sonnet|haiku/.test(id) ? "medium" : "high";
  return efforts.includes(preferred) ? preferred : efforts[0];
}

export function pickerModels(list: ModelInfo[]): PickerModel[] {
  // A Claude Code that predates the effort fields says nothing for any model:
  // then every model keeps the usual efforts, as before.
  const saysEffort = list.some((m) => m?.supportsEffort !== undefined || m?.supportedEffortLevels !== undefined);
  // Alias rows (`opus`, `fable`) carry the full id they resolve to; the
  // `default` row names the plan's default, listed again under its own name.
  const idOf = (m: ModelInfo) => m?.resolvedModel || m?.value || "";
  const fallback = list.find((m) => m?.value === "default");
  const defaultId = fallback ? idOf(fallback) : "";
  const rows = list.filter((m) => m?.value !== "default" && idOf(m));
  if (fallback && defaultId && !rows.some((m) => idOf(m) === defaultId)) {
    // "Opus 5.5 · Best for everyday, complex tasks"
    rows.unshift({ ...fallback, displayName: fallback.description?.split(" · ")[0] || defaultId });
  }
  const out: PickerModel[] = [];
  for (const m of rows) {
    const id = idOf(m);
    if (out.some((o) => o.id === id)) continue;
    const efforts = !saysEffort
      ? EFFORTS
      : m.supportedEffortLevels?.length
        ? EFFORTS.filter((e) => m.supportedEffortLevels!.includes(e as any))
        : m.supportsEffort
          ? EFFORTS
          : [];
    const model: PickerModel = { id, name: m.displayName || id, efforts };
    const effort = defaultEffort(id, efforts);
    if (effort) model.defaultEffort = effort;
    if (id === defaultId) model.isDefault = true;
    out.push(model);
  }
  return out;
}
