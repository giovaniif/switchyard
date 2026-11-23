import { expect, test } from "bun:test";
import { agentToolsProfile, isOwnTool, ownToolLabel } from "./agentTools";
import { describe as describeTool } from "./tools";

const server = { type: "stdio", command: "/bin/yardd", args: ["agent-tools"], env: { YARD_TOOLS_KEY: "k" } };

test("an agent's conversation gets the server, allowed unasked", () => {
  expect(agentToolsProfile({ tools: server })).toEqual({ mcpServers: { switchyard: server }, allowedTools: ["mcp__switchyard"] });
});

test("other sessions and restricted ones get nothing", () => {
  expect(agentToolsProfile({})).toEqual({});
  expect(agentToolsProfile({ tools: null })).toEqual({});
  expect(agentToolsProfile({ restricted: true, tools: server })).toEqual({});
});

test("calls read as what they did", () => {
  expect(isOwnTool("mcp__switchyard__remember")).toBe(true);
  expect(isOwnTool("mcp__other__remember")).toBe(false);
  expect(ownToolLabel("mcp__switchyard__message_agent", { to: "@Reviewer", text: "look at\n#12" })).toEqual({ title: "Messaged @Reviewer", detail: "look at #12" });
  expect(describeTool("mcp__switchyard__notify_owner", { message: "Need a key" }, "/")).toEqual({ kind: "other", title: "Asked for you", detail: "Need a key" });
  expect(describeTool("mcp__chart__show_chart", {}, "/").title).toBe("show_chart");
});
