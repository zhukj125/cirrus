<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { RefreshCw, Trash2, X } from "@lucide/vue";

import Spinner from "@/components/ui/Spinner.vue";
import { Button } from "@/components/ui/button";
import { mcpCallHistory, mcpClearCallHistory } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { useLocaleStore } from "@/stores/locale";
import type { McpCallHistoryEntry } from "@/types";

type HistoryFilter = "all" | "success" | "failure";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: [] }>();

const locale = useLocaleStore();
const callHistory = ref<McpCallHistoryEntry[]>([]);
const loading = ref(false);
const clearing = ref(false);
const filter = ref<HistoryFilter>("all");
const succeededCount = computed(() => callHistory.value.filter((entry) => entry.success).length);
const failedCount = computed(() => callHistory.value.length - succeededCount.value);
const filteredHistory = computed(() => {
  if (filter.value === "success") return callHistory.value.filter((entry) => entry.success);
  if (filter.value === "failure") return callHistory.value.filter((entry) => !entry.success);
  return callHistory.value;
});

const filters = computed<Array<{ value: HistoryFilter; label: string; count: number }>>(() => [
  { value: "all", label: locale.t("settings.mcpHistoryFilterAll"), count: callHistory.value.length },
  { value: "success", label: locale.t("settings.mcpHistoryFilterSuccess"), count: succeededCount.value },
  { value: "failure", label: locale.t("settings.mcpHistoryFilterFailure"), count: failedCount.value },
]);

async function refreshHistory(): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  try {
    callHistory.value = await mcpCallHistory();
  } catch (error) {
    toast.error(locale.t("settings.mcpHistoryLoadFailed", { error: error instanceof Error ? error.message : locale.t("app.unknownError") }));
  } finally {
    loading.value = false;
  }
}

async function clearHistory(): Promise<void> {
  if (clearing.value) return;
  clearing.value = true;
  try {
    await mcpClearCallHistory();
    callHistory.value = [];
    filter.value = "all";
    toast.success(locale.t("settings.mcpHistoryCleared"));
  } catch (error) {
    toast.error(locale.t("settings.mcpHistoryClearFailed", { error: error instanceof Error ? error.message : locale.t("app.unknownError") }));
  } finally {
    clearing.value = false;
  }
}

function historyTime(timestamp: number): string {
  return new Date(timestamp).toLocaleString(locale.locale, { hour12: false });
}

function historyPayload(value: string | null): string {
  if (!value) return "—";
  try {
    return JSON.stringify(JSON.parse(value), null, 2);
  } catch {
    return value;
  }
}

function handleKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape" && props.open) emit("close");
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      filter.value = "all";
      void refreshHistory();
    }
  },
  { immediate: true },
);

onMounted(() => window.addEventListener("keydown", handleKeydown));
onUnmounted(() => window.removeEventListener("keydown", handleKeydown));
</script>

<template>
  <Teleport to="body">
    <Transition name="mcp-history-panel">
      <div v-if="props.open" class="fixed inset-x-0 top-8 bottom-0 z-[4000] bg-black/15" @click.self="emit('close')">
        <aside class="mcp-history-panel absolute inset-y-0 right-0 flex w-full max-w-[500px] flex-col border-l border-border bg-card text-card-foreground shadow-[-18px_0_44px_rgba(0,0,0,0.16)]" role="dialog" aria-modal="true" :aria-label="locale.t('settings.mcpHistory')">
          <header class="flex items-start justify-between gap-3 border-b border-border px-4 py-3.5">
            <div class="min-w-0">
              <p class="text-sm font-semibold">{{ locale.t("settings.mcpHistory") }}</p>
              <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ locale.t("settings.mcpHistoryDesc", { count: callHistory.length }) }}</p>
            </div>
            <button type="button" class="grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="locale.t('common.close')" @click="emit('close')">
              <X :size="15" />
            </button>
          </header>

          <div class="flex flex-wrap items-center justify-between gap-2 border-b border-border bg-muted/20 px-4 py-2">
            <div class="flex items-center gap-1" role="tablist" :aria-label="locale.t('settings.mcpHistoryFilter')">
              <button
                v-for="item in filters"
                :key="item.value"
                type="button"
                role="tab"
                class="flex h-7 items-center gap-1.5 rounded-md px-2 text-xs transition-colors"
                :class="filter === item.value ? 'bg-background font-medium text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'"
                :aria-selected="filter === item.value"
                @click="filter = item.value"
              >
                {{ item.label }}
                <span class="rounded-full bg-muted px-1.5 text-2xs">{{ item.count }}</span>
              </button>
            </div>
            <div class="flex items-center gap-1.5">
              <Button variant="ghost" size="sm" class="h-7 px-2 text-xs" :disabled="loading" @click="refreshHistory">
                <RefreshCw :size="12" :class="loading ? 'animate-spin' : ''" />
                {{ locale.t("settings.mcpHistoryRefresh") }}
              </Button>
              <Button variant="ghost" size="sm" class="h-7 px-2 text-xs text-destructive" :disabled="clearing || callHistory.length === 0" @click="clearHistory">
                <Trash2 :size="12" />
                {{ locale.t("settings.mcpHistoryClear") }}
              </Button>
            </div>
          </div>

          <div class="min-h-0 flex-1 overflow-auto p-3">
            <div v-if="loading && callHistory.length === 0" class="grid h-40 place-items-center text-xs text-muted-foreground">
              <Spinner :size="18" />
            </div>
            <div v-else-if="filteredHistory.length === 0" class="grid h-40 place-items-center rounded-lg border border-dashed border-border text-xs text-muted-foreground">
              {{ locale.t("settings.mcpHistoryEmpty") }}
            </div>
            <div v-else class="space-y-2">
              <details v-for="entry in filteredHistory" :key="entry.id" class="group overflow-hidden rounded-lg border border-border bg-background">
                <summary class="flex cursor-pointer list-none items-center gap-3 px-3 py-3 transition-colors hover:bg-accent/40">
                  <span class="size-2 shrink-0 rounded-full" :class="entry.success ? 'bg-success' : 'bg-destructive'"></span>
                  <span class="min-w-0 flex-1">
                    <span class="block truncate font-mono text-xs font-medium">{{ entry.toolName }}</span>
                    <span class="mt-1 block truncate text-2xs text-muted-foreground"> {{ entry.clientName ?? "MCP client" }}{{ entry.clientVersion ? ` · ${entry.clientVersion}` : "" }} </span>
                  </span>
                  <span class="shrink-0 text-right text-2xs text-muted-foreground">
                    <span class="block">{{ historyTime(entry.startedAt) }}</span>
                    <span class="block">{{ entry.durationMs }} ms</span>
                  </span>
                </summary>
                <div class="space-y-2 border-t border-border bg-muted/15 px-3 py-3 text-xs">
                  <p v-if="entry.accountKey" class="text-muted-foreground">{{ entry.accountKey }}</p>
                  <p v-if="entry.error" class="break-words text-destructive">{{ entry.error }}</p>
                  <details v-if="entry.requestJson">
                    <summary class="cursor-pointer font-medium">{{ locale.t("settings.mcpHistoryRequest") }}</summary>
                    <pre class="mt-1 max-h-40 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background p-2 font-mono text-2xs">{{ historyPayload(entry.requestJson) }}</pre>
                  </details>
                  <details v-if="entry.responseJson">
                    <summary class="cursor-pointer font-medium">{{ locale.t("settings.mcpHistoryResponse") }}</summary>
                    <pre class="mt-1 max-h-56 overflow-auto whitespace-pre-wrap rounded-md border border-border bg-background p-2 font-mono text-2xs">{{ historyPayload(entry.responseJson) }}</pre>
                  </details>
                </div>
              </details>
            </div>
          </div>
        </aside>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.mcp-history-panel-enter-active,
.mcp-history-panel-leave-active {
  transition: opacity 160ms ease;
}

.mcp-history-panel-enter-active .mcp-history-panel,
.mcp-history-panel-leave-active .mcp-history-panel {
  transition: transform 220ms cubic-bezier(0.22, 1, 0.36, 1);
}

.mcp-history-panel-enter-from,
.mcp-history-panel-leave-to {
  opacity: 0;
}

.mcp-history-panel-enter-from .mcp-history-panel,
.mcp-history-panel-leave-to .mcp-history-panel {
  transform: translateX(100%);
}
</style>
