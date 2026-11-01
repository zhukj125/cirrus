export const MCP_CALL_HISTORY_OPEN_EVENT = "cirrus:open-mcp-call-history";

export function openMcpCallHistory(): void {
  window.dispatchEvent(new CustomEvent(MCP_CALL_HISTORY_OPEN_EVENT));
}
