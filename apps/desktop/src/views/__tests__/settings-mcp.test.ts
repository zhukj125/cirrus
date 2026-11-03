import { describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";

import McpCallHistoryDialog from "@/components/mcp/McpCallHistoryDialog.vue";
import * as api from "@/lib/api";
import SettingsView from "@/views/SettingsView.vue";

const client = {
  id: "codex",
  name: "Codex",
  installed: true,
  configured: true,
  canInstall: true,
  supportsWrite: true,
  configPath: String.raw`C:\Users\me\.codex\config.toml`,
  configSnippet: '[mcp_servers.cirrus]\ncommand = "C:\\\\Apps\\\\Cirrus\\\\cirrus-mcp.exe"\n',
  blockedReason: null,
};

vi.mock("@/lib/api", () => ({
  openExternal: vi.fn(async () => undefined),
  mcpInfo: vi.fn(async () => ({
    enabled: true,
    running: true,
    port: 45678,
    endpoint: "http://127.0.0.1:45678",
    command: String.raw`C:\Apps\Cirrus\cirrus-mcp.exe`,
    configToml: '[mcp_servers.cirrus]\ncommand = "C:\\\\Apps\\\\Cirrus\\\\cirrus-mcp.exe"\n',
    httpRunning: true,
    httpPort: 5226,
    httpEndpoint: "http://127.0.0.1:5226/mcp",
    httpAccessToken: "test-http-token",
    httpError: null,
    version: "1.5.0",
  })),
  mcpSetEnabled: vi.fn(async (enabled: boolean) => ({
    enabled,
    running: enabled,
    port: enabled ? 45678 : null,
    endpoint: enabled ? "http://127.0.0.1:45678" : null,
    command: String.raw`C:\Apps\Cirrus\cirrus-mcp.exe`,
    configToml: '[mcp_servers.cirrus]\ncommand = "C:\\\\Apps\\\\Cirrus\\\\cirrus-mcp.exe"\n',
    httpRunning: enabled,
    httpPort: enabled ? 5226 : null,
    httpEndpoint: enabled ? "http://127.0.0.1:5226/mcp" : null,
    httpAccessToken: enabled ? "test-http-token" : null,
    httpError: null,
    version: "1.5.0",
  })),
  mcpToolAccess: vi.fn(async () => ({ disabledTools: [] })),
  mcpSetToolEnabled: vi.fn(async () => ({ disabledTools: [] })),
  mcpServerStatus: vi.fn(async () => ({
    installed: true,
    installedVersion: "1.5.0",
    bundledVersion: "1.5.0",
    updateAvailable: false,
    activeSource: "installed" as const,
    installDir: String.raw`C:\Users\me\.cirrus\bin`,
    launcherPath: String.raw`C:\Users\me\.cirrus\bin\cirrus-mcp-server.exe`,
    nativePath: String.raw`C:\Users\me\.cirrus\bin\cirrus-mcp.exe`,
  })),
  mcpInstallServer: vi.fn(),
  mcpUninstallServer: vi.fn(),
  mcpClients: vi.fn(async () => [client]),
  mcpInstallClient: vi.fn(async () => client),
  mcpRemoveClient: vi.fn(async () => ({ ...client, configured: false })),
  mcpCallHistory: vi.fn(async () => [
    {
      id: "history-1",
      toolName: "cirrus_list_devices",
      accountKey: "dsid:111",
      clientName: "Codex",
      clientVersion: "1.0",
      startedAt: 1_700_000_000_000,
      durationMs: 42,
      success: true,
      error: null,
      requestJson: '{"accountKey":"dsid:111"}',
      responseJson: '{"deviceCount":1}',
    },
    {
      id: "history-2",
      toolName: "cirrus_login",
      accountKey: "dsid:222",
      clientName: "Cursor",
      clientVersion: "2.0",
      startedAt: 1_700_000_001_000,
      durationMs: 81,
      success: false,
      error: "Account was not found in Cirrus",
      requestJson: '{"requestId":"request-2"}',
      responseJson: null,
    },
  ]),
  mcpClearCallHistory: vi.fn(async () => undefined),
  mcpAccounts: vi.fn(async () => [
    { key: "dsid:111", appleId: "one@example.com", firstName: "One", lastName: null, lastUsedAt: 1, enabled: true, signedIn: true, avatarDataUrl: "data:image/jpeg;base64,avatar" },
    { key: "dsid:222", appleId: "two@example.com", firstName: "Two", lastName: null, lastUsedAt: 2, enabled: false, signedIn: true, avatarDataUrl: null },
  ]),
  mcpSetAccountEnabled: vi.fn(async (_key: string, enabled: boolean) => [
    { key: "dsid:111", appleId: "one@example.com", firstName: "One", lastName: null, lastUsedAt: 1, enabled, signedIn: true, avatarDataUrl: "data:image/jpeg;base64,avatar" },
    { key: "dsid:222", appleId: "two@example.com", firstName: "Two", lastName: null, lastUsedAt: 2, enabled: false, signedIn: true, avatarDataUrl: null },
  ]),
}));

describe("SettingsView MCP", () => {
  it("shows app integrations, account access, and copyable stdio config", async () => {
    const wrapper = mount(SettingsView, { props: { initialCategory: "mcp" }, global: { plugins: [createPinia()] } });
    await flushPromises();

    expect(wrapper.text()).toContain("Running · local access only");
    expect(wrapper.text()).toContain("MCP engine");
    expect(wrapper.text()).toContain("Independent install");
    expect(wrapper.text()).toContain("1.5.0");
    expect(wrapper.text()).toContain("Codex");
    expect(wrapper.text()).toContain("Configured");
    expect(wrapper.text()).toContain("[mcp_servers.cirrus]");
    expect(wrapper.get('[role="switch"]').attributes("aria-checked")).toBe("true");

    const httpTab = wrapper.findAll('[role="tab"]').find((item) => item.text().includes("Streamable HTTP"));
    expect(httpTab).toBeDefined();
    await httpTab!.trigger("click");
    expect(wrapper.text()).toContain("http://127.0.0.1:5226/mcp");
    expect(wrapper.text()).toContain("Bearer test-http-token");

    const permissionsTab = wrapper.findAll('[role="tab"]').find((item) => item.text().includes("Permissions"));
    expect(permissionsTab).toBeDefined();
    await permissionsTab!.trigger("click");
    expect(wrapper.get('img[src="data:image/jpeg;base64,avatar"]').exists()).toBe(true);
    expect(wrapper.text()).toContain("one@example.com");
    expect(wrapper.text()).toContain("two@example.com");
    expect(wrapper.text()).toContain("MCP allowed");
    expect(wrapper.text()).toContain("MCP blocked");

    await wrapper.get('[aria-label="Toggle MCP access for two@example.com"]').trigger("click");
    expect(api.mcpSetAccountEnabled).toHaveBeenCalledWith("dsid:222", true);

    wrapper.unmount();

    const historyPanel = mount(McpCallHistoryDialog, {
      props: { open: true },
      global: { plugins: [createPinia()], stubs: { Teleport: true } },
    });
    await flushPromises();
    expect(historyPanel.text()).toContain("cirrus_list_devices");
    expect(historyPanel.text()).toContain("cirrus_login");

    const successTab = historyPanel.findAll('[role="tab"]').find((item) => item.text().includes("Succeeded"));
    expect(successTab).toBeDefined();
    await successTab!.trigger("click");
    expect(historyPanel.text()).toContain("cirrus_list_devices");
    expect(historyPanel.text()).not.toContain("Account was not found in Cirrus");

    const failureTab = historyPanel.findAll('[role="tab"]').find((item) => item.text().includes("Failed"));
    expect(failureTab).toBeDefined();
    await failureTab!.trigger("click");
    expect(historyPanel.text()).toContain("Account was not found in Cirrus");
    expect(historyPanel.text()).not.toContain("cirrus_list_devices");

    historyPanel.unmount();
  });
});
