import { describe, expect, it, vi } from "vitest";
import { flushPromises, mount } from "@vue/test-utils";
import { createPinia } from "pinia";

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
    version: "1.4.2",
  })),
  mcpSetEnabled: vi.fn(async (enabled: boolean) => ({
    enabled,
    running: enabled,
    port: enabled ? 45678 : null,
    endpoint: enabled ? "http://127.0.0.1:45678" : null,
    command: String.raw`C:\Apps\Cirrus\cirrus-mcp.exe`,
    configToml: '[mcp_servers.cirrus]\ncommand = "C:\\\\Apps\\\\Cirrus\\\\cirrus-mcp.exe"\n',
    version: "1.4.2",
  })),
  mcpToolAccess: vi.fn(async () => ({ disabledTools: [] })),
  mcpSetToolEnabled: vi.fn(async () => ({ disabledTools: [] })),
  mcpClients: vi.fn(async () => [client]),
  mcpInstallClient: vi.fn(async () => client),
  mcpRemoveClient: vi.fn(async () => ({ ...client, configured: false })),
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
    expect(wrapper.text()).toContain("Codex");
    expect(wrapper.text()).toContain("Configured");
    expect(wrapper.text()).toContain("[mcp_servers.cirrus]");
    expect(wrapper.get('[role="switch"]').attributes("aria-checked")).toBe("true");

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
  });
});
