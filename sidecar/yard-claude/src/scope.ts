/** The MCP servers an agent's or automation's session may use (yardd's
 * scope.rs). yardd's `start` carries `mcp`: `{ only: [names] }` is exactly
 * those servers, `{ except: [names] }` every server but those; absent or null,
 * the session is not scoped.
 *
 * Claude Code enforces it as policy: `managedSettings` (the SDK's policy tier
 * from the parent process) with `allowedMcpServers` or `deniedMcpServers` by
 * server name. That covers every source Claude Code loads servers from: user,
 * project and local config, plugins, servers passed in `mcpServers`, and
 * claude.ai connectors; a server left out never starts. Switchyard's own tools
 * server is always allowed. With no claude.ai connector on an `only` list,
 * `ENABLE_CLAUDEAI_MCP_SERVERS=false` also keeps Claude Code from fetching
 * the account's connectors at all. A machine whose administrator installed
 * managed settings of their own drops these (Claude Code keeps the admin's). */
export type McpScope = { only?: unknown; except?: unknown } | null | undefined;

const names = (v: unknown): string[] | null => (Array.isArray(v) ? v.filter((n): n is string => typeof n === "string" && n.length > 0) : null);

/** claude.ai connectors are listed as `claude.ai <name>`. */
export const isClaudeAi = (name: string) => /^claude\.ai /i.test(name);

export function scopeProfile(scope: McpScope, ownServer: string | null): { managedSettings?: Record<string, unknown>; env?: Record<string, string> } {
  if (!scope || typeof scope !== "object") return {};
  const only = names(scope.only);
  if (only) {
    const allowed = ownServer && !only.includes(ownServer) ? [...only, ownServer] : only;
    return {
      managedSettings: { allowedMcpServers: allowed.map((serverName) => ({ serverName })) },
      ...(only.some(isClaudeAi) ? {} : { env: { ENABLE_CLAUDEAI_MCP_SERVERS: "false" } }),
    };
  }
  const except = (names(scope.except) ?? []).filter((n) => n !== ownServer);
  if (!except.length) return {};
  return { managedSettings: { deniedMcpServers: except.map((serverName) => ({ serverName })) } };
}
