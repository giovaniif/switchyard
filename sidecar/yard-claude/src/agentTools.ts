/** Switchyard's own tools (see yardd's agent_tools.rs): an agent's
 * conversation gets them all, every other session `request_connector` alone.
 * yardd's `start` carries `tools`: the stdio MCP server to launch (`yardd
 * agent-tools` with a key that names this one conversation). Never given to
 * a restricted session. */
export const SERVER = "switchyard";

export type Start = { restricted?: boolean; tools?: Record<string, unknown> | null };

/** SDK options that add the server and let its tools run unasked: they act
 * only on the agent's own conversation, and yardd caps them. */
export function agentToolsProfile(start: Start): Record<string, unknown> {
  if (start.restricted || !start.tools || typeof start.tools !== "object") return {};
  return { mcpServers: { [SERVER]: start.tools }, allowedTools: [`mcp__${SERVER}`] };
}

export const isOwnTool = (name: string) => name.startsWith(`mcp__${SERVER}__`);

/** How a call reads in the thread, as yardd labels it for the other engines. */
export function ownToolLabel(name: string, input: any): { title: string; detail: string } | null {
  if (!isOwnTool(name)) return null;
  const s = (v: unknown) => (typeof v === "string" ? v.split(/\s+/).filter(Boolean).join(" ") : "");
  switch (name.slice(`mcp__${SERVER}__`.length)) {
    case "notify_owner":
      return { title: "Asked for you", detail: s(input?.message) };
    case "message_agent":
      return { title: `Messaged @${s(input?.to).replace(/^@/, "")}`, detail: s(input?.text) };
    case "list_agents":
      return { title: "Listed agents", detail: "" };
    case "remember":
      return { title: "Remembered", detail: s(input?.text) };
    case "set_memory":
      return { title: "Updated memory", detail: s(input?.text) };
    case "update_setup":
      return { title: "Updated its setup", detail: Object.keys(input ?? {}).join(", ") };
    case "slack_reply":
      return { title: "Posted in Slack", detail: s(input?.text) };
    case "request_connector":
      return { title: `Asked to connect ${s(input?.service)}`, detail: s(input?.reason) };
    default:
      return null;
  }
}
