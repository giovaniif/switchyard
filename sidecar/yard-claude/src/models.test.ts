import { expect, test } from "bun:test";
import { pickerModels } from "./models";

const ALL = ["low", "medium", "high", "xhigh", "max"];

// The shape Claude Code 2.1.327 answers `supportedModels()` with on a Max plan
// (trimmed): alias rows carry the id they resolve to.
const reported = [
  { value: "default", resolvedModel: "claude-opus-5-5", displayName: "Default (recommended)", description: "Opus 5.5 · Best for everyday, complex tasks", supportsEffort: true, supportedEffortLevels: ALL },
  { value: "opus", resolvedModel: "claude-opus-5-5", displayName: "Opus 5.5", description: "For complex work", supportsEffort: true, supportedEffortLevels: ALL },
  { value: "fable", resolvedModel: "claude-fable-5-2", displayName: "Fable 5.2", description: "For your toughest challenges", supportsEffort: true, supportedEffortLevels: ALL },
  { value: "sonnet", resolvedModel: "claude-sonnet-5-5", displayName: "Sonnet 5.5", description: "Most efficient", supportsEffort: true, supportedEffortLevels: ALL },
  { value: "haiku", resolvedModel: "claude-haiku-5-5", displayName: "Haiku 5.5", description: "Fastest", supportsEffort: true, supportedEffortLevels: ALL },
  { value: "claude-haiku-4-5-20251001", resolvedModel: "claude-haiku-4-5-20251001", displayName: "Haiku 4.5", description: "Fastest" },
  { value: "claude-opus-4-6", resolvedModel: "claude-opus-4-6", displayName: "Opus 4.6", description: "Older", supportsEffort: true, supportedEffortLevels: ["low", "medium", "high", "max"] },
];

test("every model Claude reports, by its full id and name, the plan's default marked", () => {
  expect(pickerModels(reported as any)).toEqual([
    { id: "claude-opus-5-5", name: "Opus 5.5", efforts: ALL, defaultEffort: "high", isDefault: true },
    { id: "claude-fable-5-2", name: "Fable 5.2", efforts: ALL, defaultEffort: "high" },
    { id: "claude-sonnet-5-5", name: "Sonnet 5.5", efforts: ALL, defaultEffort: "medium" },
    { id: "claude-haiku-5-5", name: "Haiku 5.5", efforts: ALL, defaultEffort: "medium" },
    // Claude Code offers no effort on it.
    { id: "claude-haiku-4-5-20251001", name: "Haiku 4.5", efforts: [] },
    { id: "claude-opus-4-6", name: "Opus 4.6", efforts: ["low", "medium", "high", "max"], defaultEffort: "high" },
  ]);
});

test("a default row with no row of its own takes its name from the description", () => {
  const only = [{ value: "default", resolvedModel: "claude-opus-5-5", displayName: "Default (recommended)", description: "Opus 5.5 · Best for everyday, complex tasks", supportsEffort: true }];
  expect(pickerModels(only as any)).toEqual([{ id: "claude-opus-5-5", name: "Opus 5.5", efforts: ALL, defaultEffort: "high", isDefault: true }]);
});

test("a Claude Code that says nothing about effort keeps the usual efforts on every model", () => {
  const old = [
    { value: "claude-fable-5-2", displayName: "Fable 5.2", description: "" },
    { value: "claude-sonnet-5-5", displayName: "Sonnet 5.5", description: "" },
  ];
  expect(pickerModels(old as any)).toEqual([
    { id: "claude-fable-5-2", name: "Fable 5.2", efforts: ALL, defaultEffort: "high" },
    { id: "claude-sonnet-5-5", name: "Sonnet 5.5", efforts: ALL, defaultEffort: "medium" },
  ]);
});

test("rows without an id, and unknown effort levels, are left out", () => {
  const odd = [{ displayName: "Nothing" }, { value: "claude-x", displayName: "", description: "", supportedEffortLevels: ["low", "turbo"] }];
  expect(pickerModels(odd as any)).toEqual([{ id: "claude-x", name: "claude-x", efforts: ["low"], defaultEffort: "low" }]);
});
