<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { Check, Copy, PlugZap, RefreshCw, ShieldCheck, Terminal, Trash2 } from "@lucide/vue";

import Spinner from "@/components/ui/Spinner.vue";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { mcpAccounts, mcpClients, mcpInfo, mcpInstallClient, mcpRemoveClient, mcpSetAccountEnabled, mcpSetEnabled, mcpSetToolEnabled, mcpToolAccess } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { useLocaleStore } from "@/stores/locale";
import type { McpAccountInfo, McpClientInfo, McpInfo } from "@/types";

type ManagementTab = "access" | "permissions";

const locale = useLocaleStore();
const info = ref<McpInfo | null>(null);
const clients = ref<McpClientInfo[]>([]);
const accounts = ref<McpAccountInfo[]>([]);
const disabledTools = ref<string[]>([]);
const loading = ref(true);
const bridgeSaving = ref(false);
const clientSaving = ref<string | null>(null);
const accountSaving = ref<string | null>(null);
const toolSaving = ref<string | null>(null);
const managementTab = ref<ManagementTab>("access");
const toolRows = [
  { name: "cirrus_login", readOnly: false },
  { name: "cirrus_list_accounts", readOnly: true },
  { name: "cirrus_account_status", readOnly: true },
  { name: "cirrus_logout", readOnly: false },
  { name: "cirrus_list_devices", readOnly: true },
  { name: "cirrus_play_sound", readOnly: false },
  { name: "cirrus_list_photos", readOnly: true },
  { name: "cirrus_photos_total", readOnly: true },
  { name: "cirrus_photos_timeline", readOnly: true },
  { name: "cirrus_photos_month_exact", readOnly: true },
  { name: "cirrus_download_photo", readOnly: false },
  { name: "cirrus_cancel_download", readOnly: false },
  { name: "cirrus_list_download_tasks", readOnly: true },
  { name: "cirrus_get_account_avatar", readOnly: true },
  { name: "cirrus_get_contact_photo", readOnly: true },
  { name: "cirrus_list_notes", readOnly: true },
  { name: "cirrus_get_note", readOnly: true },
  { name: "cirrus_list_contacts", readOnly: true },
  { name: "cirrus_load_calendar", readOnly: true },
] as const;
const transportTab = ref<"stdio" | "http">("stdio");
const selectedClientId = ref("claude");

const bridgeEnabled = computed({
  get: () => info.value?.enabled ?? false,
  set: (value: boolean) => void updateBridge(value),
});
const configuredClients = computed(() => clients.value.filter((client) => client.configured).length);
const enabledAccounts = computed(() => accounts.value.filter((account) => account.enabled).length);
const enabledToolCount = computed(() => toolRows.filter((tool) => !disabledTools.value.includes(tool.name)).length);
const selectedClient = computed(() => clients.value.find((client) => client.id === selectedClientId.value) ?? clients.value[0] ?? null);
const selectedSnippet = computed(() => selectedClient.value?.configSnippet ?? null);
const bridgeStatus = computed(() => {
  if (loading.value) return locale.t("settings.mcpChecking");
  if (info.value?.running) return locale.t("settings.mcpRunning");
  if (info.value?.enabled) return locale.t("settings.mcpStarting");
  return locale.t("settings.mcpStopped");
});

async function loadAll(): Promise<void> {
  loading.value = true;
  try {
    const [nextInfo, nextClients, nextAccounts, nextToolAccess] = await Promise.all([mcpInfo(), mcpClients(), mcpAccounts(), mcpToolAccess()]);
    info.value = nextInfo;
    clients.value = nextClients;
    accounts.value = nextAccounts;
    disabledTools.value = [...nextToolAccess.disabledTools];
    if (!nextClients.some((client) => client.id === selectedClientId.value)) selectedClientId.value = nextClients[0]?.id ?? "claude";
  } catch (error) {
    toast.error(locale.t("settings.mcpLoadFailed", { error: error instanceof Error ? error.message : locale.t("app.unknownError") }));
  } finally {
    loading.value = false;
  }
}

async function updateBridge(enabled: boolean): Promise<void> {
  if (bridgeSaving.value) return;
  bridgeSaving.value = true;
  try {
    info.value = await mcpSetEnabled(enabled);
    clients.value = await mcpClients();
    toast.success(locale.t(enabled ? "settings.mcpEnabledToast" : "settings.mcpDisabledToast"));
  } catch (error) {
    toast.error(locale.t("settings.mcpUpdateFailed", { error: error instanceof Error ? error.message : locale.t("app.unknownError") }));
  } finally {
    bridgeSaving.value = false;
  }
}

function clientStatus(client: McpClientInfo): string {
  if (client.configured) return locale.t("settings.mcpClientConfigured");
  if (client.supportsWrite && client.canInstall) return locale.t("settings.mcpClientAvailable");
  if (client.supportsWrite && !client.installed) return locale.t("settings.mcpClientNotInstalled");
  return locale.t("settings.mcpClientCopyOnly");
}

function clientStatusTone(client: McpClientInfo): string {
  if (client.configured) return "text-success";
  if (client.supportsWrite && client.canInstall) return "text-primary";
  return "text-muted-foreground";
}

async function configureClient(client: McpClientInfo): Promise<void> {
  if (clientSaving.value) return;
  clientSaving.value = client.id;
  try {
    const updated = await mcpInstallClient(client.id);
    clients.value = clients.value.map((item) => (item.id === updated.id ? updated : item));
    toast.success(locale.t("settings.mcpClientInstalled", { client: client.name }));
  } catch (error) {
    toast.error(locale.t("settings.mcpClientInstallFailed", { error: error instanceof Error ? error.message : locale.t("app.unknownError") }));
  } finally {
    clientSaving.value = null;
  }
}

async function removeClient(client: McpClientInfo): Promise<void> {
  if (clientSaving.value) return;
  clientSaving.value = client.id;
  try {
    const updated = await mcpRemoveClient(client.id);
    clients.value = clients.value.map((item) => (item.id === updated.id ? updated : item));
    toast.success(locale.t("settings.mcpClientRemoved", { client: client.name }));
  } catch (error) {
    toast.error(locale.t("settings.mcpClientRemoveFailed", { error: error instanceof Error ? error.message : locale.t("app.unknownError") }));
  } finally {
    clientSaving.value = null;
  }
}

async function copySnippet(snippet: string | null): Promise<void> {
  if (!snippet) return;
  try {
    await navigator.clipboard.writeText(snippet);
    toast.success(locale.t("settings.mcpCopied"));
  } catch {
    toast.error(locale.t("settings.mcpCopyFailed"));
  }
}

function toolEnabled(tool: string): boolean {
  return !disabledTools.value.includes(tool);
}

async function setToolEnabled(tool: string, enabled: boolean): Promise<void> {
  if (toolSaving.value) return;
  toolSaving.value = tool;
  try {
    const next = await mcpSetToolEnabled(tool, enabled);
    disabledTools.value = [...next.disabledTools];
    toast.success(locale.t(enabled ? "settings.mcpToolEnabledToast" : "settings.mcpToolDisabledToast", { tool }));
  } catch (error) {
    toast.error(locale.t("settings.mcpToolUpdateFailed", { error: error instanceof Error ? error.message : locale.t("app.unknownError") }));
  } finally {
    toolSaving.value = null;
  }
}

async function setAccountEnabled(account: McpAccountInfo, enabled: boolean): Promise<void> {
  if (accountSaving.value) return;
  accountSaving.value = account.key;
  try {
    accounts.value = await mcpSetAccountEnabled(account.key, enabled);
    toast.success(locale.t(enabled ? "settings.mcpAccountEnabledToast" : "settings.mcpAccountDisabledToast", { account: account.appleId }));
  } catch (error) {
    toast.error(locale.t("settings.mcpAccountUpdateFailed", { error: error instanceof Error ? error.message : locale.t("app.unknownError") }));
  } finally {
    accountSaving.value = null;
  }
}

function accountName(account: McpAccountInfo): string {
  const name = [account.firstName, account.lastName].filter(Boolean).join(" ");
  return name || account.appleId.split("@")[0] || account.appleId;
}

onMounted(() => void loadAll());
</script>

<template>
  <div class="flex flex-col gap-5 pb-4">
    <header class="flex items-start justify-between gap-4">
      <div>
        <h2 class="text-lg font-semibold tracking-tight">{{ locale.t("settings.mcp") }}</h2>
        <p class="mt-1 text-sm leading-6 text-muted-foreground">{{ locale.t("settings.mcpManagementDesc") }}</p>
      </div>
      <button type="button" class="grid size-8 shrink-0 place-items-center rounded-md border border-border bg-background text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="locale.t('settings.mcpRefresh')" @click="loadAll">
        <RefreshCw :size="14" :class="loading ? 'animate-spin' : ''" />
      </button>
    </header>

    <section class="overflow-hidden rounded-md border border-border bg-background">
      <div class="flex items-start gap-3 border-b border-border px-4 py-3.5">
        <span class="mt-0.5 grid size-8 shrink-0 place-items-center rounded-md bg-secondary text-foreground">
          <PlugZap :size="15" />
        </span>
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-2">
            <p class="text-sm font-medium">Cirrus MCP</p>
            <span class="inline-flex items-center gap-1.5 rounded-full border border-border px-2 py-0.5 text-2xs text-muted-foreground">
              <span class="size-1.5 rounded-full" :class="info?.running ? 'bg-success' : 'bg-muted-foreground/40'"></span>
              {{ bridgeStatus }}
            </span>
            <span v-if="info?.port" class="font-mono text-2xs text-muted-foreground">127.0.0.1:{{ info.port }}</span>
          </div>
          <p class="mt-1 text-xs text-muted-foreground">{{ locale.t("settings.mcpEnableDesc") }}</p>
        </div>
        <Switch v-model="bridgeEnabled" class="mt-0.5 shrink-0" :disabled="loading || bridgeSaving || !info" :aria-label="locale.t('settings.mcpEnable')" />
      </div>

      <div class="flex border-b border-border px-3" role="tablist" :aria-label="locale.t('settings.mcpManagementTabs')">
        <button
          type="button"
          role="tab"
          class="relative flex h-10 items-center gap-1.5 px-2.5 text-sm transition-colors"
          :class="managementTab === 'access' ? 'font-medium text-foreground' : 'text-muted-foreground hover:text-foreground'"
          :aria-selected="managementTab === 'access'"
          @click="managementTab = 'access'"
        >
          <PlugZap :size="14" />
          {{ locale.t("settings.mcpAccessTab") }}
          <span v-if="managementTab === 'access'" class="absolute inset-x-2.5 bottom-0 h-0.5 rounded-full bg-primary"></span>
        </button>
        <button
          type="button"
          role="tab"
          class="relative flex h-10 items-center gap-1.5 px-2.5 text-sm transition-colors"
          :class="managementTab === 'permissions' ? 'font-medium text-foreground' : 'text-muted-foreground hover:text-foreground'"
          :aria-selected="managementTab === 'permissions'"
          @click="managementTab = 'permissions'"
        >
          <ShieldCheck :size="14" />
          {{ locale.t("settings.mcpPermissionsTab") }}
          <span v-if="managementTab === 'permissions'" class="absolute inset-x-2.5 bottom-0 h-0.5 rounded-full bg-primary"></span>
        </button>
      </div>

      <div v-if="managementTab === 'access'" class="space-y-5 p-4">
        <div class="flex items-center gap-1 overflow-x-auto border-b border-border" role="tablist" :aria-label="locale.t('settings.mcpTransportTabs')">
          <button type="button" role="tab" class="relative flex h-8 shrink-0 items-center gap-1.5 px-2 text-xs transition-colors" :class="transportTab === 'stdio' ? 'font-medium text-foreground' : 'text-muted-foreground'" :aria-selected="transportTab === 'stdio'" @click="transportTab = 'stdio'">
            <Terminal :size="13" />
            {{ locale.t("settings.mcpStdioTab") }}
            <span v-if="transportTab === 'stdio'" class="absolute inset-x-1 bottom-0 h-0.5 rounded-full bg-primary"></span>
          </button>
          <button type="button" role="tab" class="relative flex h-8 shrink-0 items-center gap-1.5 px-2 text-xs text-muted-foreground/55" disabled :title="locale.t('settings.mcpHttpUnsupported')">
            HTTP
            <span class="rounded bg-muted px-1 py-px text-[9px]">{{ locale.t("settings.mcpComingSoon") }}</span>
          </button>
        </div>

        <div class="grid gap-2 rounded-md border border-border bg-muted/20 px-3 py-2.5 text-xs sm:grid-cols-3">
          <div>
            <p class="text-2xs text-muted-foreground">{{ locale.t("settings.mcpTransport") }}</p>
            <p class="mt-1 font-medium">{{ locale.t("settings.mcpStdioTab") }}</p>
          </div>
          <div>
            <p class="text-2xs text-muted-foreground">{{ locale.t("settings.mcpClients") }}</p>
            <p class="mt-1 font-medium">{{ configuredClients }} / {{ clients.length }}</p>
          </div>
          <div>
            <p class="text-2xs text-muted-foreground">{{ locale.t("settings.mcpAccounts") }}</p>
            <p class="mt-1 font-medium">{{ enabledAccounts }} / {{ accounts.length }}</p>
          </div>
        </div>

        <div class="space-y-2.5">
          <div>
            <p class="text-sm font-medium">{{ locale.t("settings.mcpConfig") }}</p>
            <p class="mt-1 text-xs text-muted-foreground">{{ locale.t("settings.mcpConfigOptionsHint") }}</p>
          </div>

          <div class="flex gap-1 overflow-x-auto pb-1" role="tablist" :aria-label="locale.t('settings.mcpClients')">
            <button
              v-for="client in clients"
              :key="client.id"
              type="button"
              role="tab"
              class="h-7 shrink-0 rounded-md border px-2.5 text-xs transition-colors"
              :class="selectedClient?.id === client.id ? 'border-primary/30 bg-primary/8 font-medium text-primary' : 'border-border bg-background text-muted-foreground hover:bg-accent hover:text-foreground'"
              :aria-selected="selectedClient?.id === client.id"
              @click="selectedClientId = client.id"
            >
              {{ client.name }}
            </button>
          </div>

          <div v-if="selectedClient" class="overflow-hidden rounded-md border border-border bg-background">
            <div class="flex flex-wrap items-start justify-between gap-3 border-b border-border bg-muted/20 px-3 py-2.5">
              <div class="min-w-0">
                <p class="text-xs font-medium">{{ selectedClient.name }}</p>
                <p class="mt-0.5 text-2xs leading-4 text-muted-foreground">{{ locale.t(`settings.mcpPath.${selectedClient.id}`) }}</p>
                <p v-if="selectedClient.configPath" class="mt-1 truncate font-mono text-[10px] text-muted-foreground/70">{{ selectedClient.configPath }}</p>
              </div>
              <div class="flex shrink-0 items-center gap-1.5">
                <span class="text-2xs" :class="clientStatusTone(selectedClient)">{{ clientStatus(selectedClient) }}</span>
                <Button v-if="selectedClient.supportsWrite && selectedClient.configured" variant="ghost" size="sm" class="h-7 px-2 text-xs text-muted-foreground hover:text-destructive" :disabled="clientSaving !== null" @click="removeClient(selectedClient)">
                  <Spinner v-if="clientSaving === selectedClient.id" :size="11" />
                  <Trash2 v-else :size="12" />
                </Button>
                <Button v-else-if="selectedClient.supportsWrite" variant="outline" size="sm" class="h-7 px-2 text-xs" :disabled="!selectedClient.canInstall || clientSaving !== null" @click="configureClient(selectedClient)">
                  <Spinner v-if="clientSaving === selectedClient.id" :size="11" />
                  <Check v-else :size="12" />
                  {{ locale.t("settings.mcpWriteConfig") }}
                </Button>
                <Button variant="outline" size="sm" class="h-7 px-2 text-xs" :disabled="!selectedSnippet" @click="copySnippet(selectedSnippet)">
                  <Copy :size="12" />
                  {{ locale.t("settings.mcpCopy") }}
                </Button>
              </div>
            </div>
            <pre class="max-h-64 overflow-auto whitespace-pre p-3 font-mono text-xs leading-5 text-muted-foreground">{{ selectedSnippet ?? locale.t("settings.mcpUnavailable") }}</pre>
          </div>
        </div>

        <div class="flex items-start gap-2 rounded-md border border-border bg-muted/20 px-3 py-2.5 text-xs leading-5 text-muted-foreground">
          <Terminal :size="14" class="mt-0.5 shrink-0" />
          <span>{{ locale.t("settings.mcpClientRestartHint") }}</span>
        </div>
      </div>

      <div v-else class="space-y-5 p-4">
        <div class="grid gap-2 rounded-md border border-border bg-muted/20 px-3 py-2.5 text-xs sm:grid-cols-3">
          <div>
            <p class="text-2xs text-muted-foreground">{{ locale.t("settings.mcpAccounts") }}</p>
            <p class="mt-1 font-medium">{{ enabledAccounts }} {{ locale.t("settings.mcpEnabledCount") }}</p>
          </div>
          <div>
            <p class="text-2xs text-muted-foreground">{{ locale.t("settings.mcpTools") }}</p>
            <p class="mt-1 font-medium">{{ enabledToolCount }} / {{ toolRows.length }}</p>
          </div>
          <div>
            <p class="text-2xs text-muted-foreground">{{ locale.t("settings.mcpAccountDefault") }}</p>
            <p class="mt-1 font-medium">{{ locale.t("settings.mcpAccountAllowed") }}</p>
          </div>
        </div>

        <section class="space-y-2.5">
          <div>
            <p class="text-sm font-medium">{{ locale.t("settings.mcpAccountScope") }}</p>
            <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.mcpAccountsDesc") }}</p>
          </div>
          <div v-if="accounts.length > 0" class="divide-y divide-border overflow-hidden rounded-md border border-border bg-background">
            <div v-for="account in accounts" :key="account.key" class="flex items-center gap-3 px-3 py-2.5">
              <img v-if="account.avatarDataUrl" :src="account.avatarDataUrl" alt="" class="size-8 shrink-0 rounded-full object-cover" />
              <span v-else class="grid size-8 shrink-0 place-items-center rounded-full bg-secondary text-xs font-semibold">{{ accountName(account).charAt(0).toUpperCase() }}</span>
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm font-medium">{{ accountName(account) }}</p>
                <p class="truncate text-xs text-muted-foreground">{{ account.appleId }}</p>
              </div>
              <span class="hidden shrink-0 text-2xs text-muted-foreground sm:block">{{ account.signedIn ? locale.t("settings.mcpAccountSignedIn") : locale.t("settings.mcpAccountSignedOut") }}</span>
              <span class="shrink-0 text-2xs" :class="account.enabled ? 'text-primary' : 'text-warning'">{{ account.enabled ? locale.t("settings.mcpAccountAllowed") : locale.t("settings.mcpAccountBlocked") }}</span>
              <Spinner v-if="accountSaving === account.key" :size="12" />
              <Switch v-else :model-value="account.enabled" :aria-label="locale.t('settings.mcpToggleAccount', { account: account.appleId })" @update:model-value="(value) => setAccountEnabled(account, value)" />
            </div>
          </div>
          <div v-else class="rounded-md border border-dashed border-border px-3 py-6 text-center text-xs text-muted-foreground">{{ locale.t("settings.mcpNoAccounts") }}</div>
        </section>

        <section class="space-y-2.5">
          <div>
            <p class="text-sm font-medium">{{ locale.t("settings.mcpTools") }}</p>
            <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.mcpToolPermissionsDesc") }}</p>
          </div>
          <div class="grid gap-2 sm:grid-cols-2">
            <div v-for="tool in toolRows" :key="tool.name" class="flex items-center gap-2 rounded-md border border-border bg-background px-2.5 py-2">
              <div class="min-w-0 flex-1">
                <p class="truncate font-mono text-2xs font-medium">{{ tool.name }}</p>
                <p class="mt-0.5 text-[10px] text-muted-foreground">{{ tool.readOnly ? locale.t("settings.mcpReadOnly") : locale.t("settings.mcpWriteTool") }}</p>
              </div>
              <Spinner v-if="toolSaving === tool.name" :size="12" />
              <Switch v-else :model-value="toolEnabled(tool.name)" :aria-label="tool.name" @update:model-value="(value) => setToolEnabled(tool.name, value)" />
            </div>
          </div>
        </section>

        <div class="flex items-start gap-2 rounded-md border border-border bg-muted/20 px-3 py-2.5 text-xs leading-5 text-muted-foreground">
          <ShieldCheck :size="14" class="mt-0.5 shrink-0" />
          <span>{{ locale.t("settings.mcpLoginOnly") }}</span>
        </div>
      </div>
    </section>
  </div>
</template>
