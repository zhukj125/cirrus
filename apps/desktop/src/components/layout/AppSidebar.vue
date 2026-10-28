<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { CalendarDays, Check, ChevronUp, Download, Images, LogOut, MapPin, Monitor, Moon, NotebookPen, Users, PanelLeftClose, PanelLeftOpen, Plus, Sun } from "@lucide/vue";

import { StorageKey, type ShellView, type ThemeMode } from "@/constants";
import { readStored, writeStored } from "@/lib/common/storage";
import { recentTimeLabel } from "@/lib/time";
import { formatSpeed, useDownloadsStore } from "@/stores/downloads";
import { useLocaleStore } from "@/stores/locale";
import { useThemeStore } from "@/stores/theme";
import type { AccountInfo, KnownAccount } from "@/types";

const props = withDefaults(
  defineProps<{
    view: ShellView;
    account?: AccountInfo | null;
    deviceCount?: number | null;
    accounts?: KnownAccount[];
    activeAccountKey?: string | null;
    accountsLoading?: boolean;
    avatars?: Record<string, string>;
  }>(),
  {
    account: null,
    deviceCount: null,
    accounts: () => [],
    activeAccountKey: null,
    accountsLoading: false,
    avatars: () => ({}),
  },
);

const emit = defineEmits<{ navigate: [view: ShellView]; logout: []; switchAccount: [key: string]; addAccount: []; validateAccounts: [] }>();

const theme = useThemeStore();
const locale = useLocaleStore();
const downloads = useDownloadsStore();
const accountsOpen = ref(false);
const menuRoot = ref<HTMLElement | null>(null);
const collapsed = ref(readStored(StorageKey.sidebarCollapsed) === "true");

function toggleCollapsed(): void {
  collapsed.value = !collapsed.value;
  accountsOpen.value = false;
  writeStored(StorageKey.sidebarCollapsed, collapsed.value ? "true" : "false");
}

function onDocumentPointerDown(event: PointerEvent): void {
  if (!accountsOpen.value || !menuRoot.value) return;
  if (!menuRoot.value.contains(event.target as Node)) accountsOpen.value = false;
}

onMounted(() => document.addEventListener("pointerdown", onDocumentPointerDown));
onUnmounted(() => document.removeEventListener("pointerdown", onDocumentPointerDown));

function toggleAccounts(): void {
  accountsOpen.value = !accountsOpen.value;
  if (accountsOpen.value) emit("validateAccounts");
}

function selectAccount(account: KnownAccount): void {
  accountsOpen.value = false;
  if (account.key !== props.activeAccountKey) emit("switchAccount", account.key);
}

function addAccount(): void {
  accountsOpen.value = false;
  emit("addAccount");
}

const displayName = computed(() => {
  const { firstName, lastName, appleId } = props.account ?? {};
  const full = [firstName, lastName].filter(Boolean).join(" ");
  if (full) return { text: full, title: full };
  if (appleId) {
    const local = appleId.split("@")[0] ?? appleId;
    return { text: local, title: appleId };
  }
  return { text: locale.t("sidebar.signedIn"), title: "" };
});

const appleId = computed(() => props.account?.appleId ?? null);
const currentInitial = computed(() => displayName.value.text.trim().charAt(0).toUpperCase() || "?");
const currentAvatar = computed(() => (props.activeAccountKey ? props.avatars[props.activeAccountKey] : null));

const THEME_OPTIONS = computed<Array<{ value: ThemeMode; label: string; icon: typeof Sun }>>(() => [
  { value: "system", label: locale.t("theme.system"), icon: Monitor },
  { value: "light", label: locale.t("theme.light"), icon: Sun },
  { value: "dark", label: locale.t("theme.dark"), icon: Moon },
]);

const NAV_ITEMS = computed<Array<{ value: ShellView; label: string; icon: typeof MapPin }>>(() => [
  { value: "findmy", label: locale.t("nav.devices"), icon: MapPin },
  { value: "photos-gallery", label: locale.t("nav.photos"), icon: Images },
  { value: "notes", label: locale.t("nav.notes"), icon: NotebookPen },
  { value: "contacts", label: locale.t("nav.contacts"), icon: Users },
  { value: "calendar", label: locale.t("nav.calendar"), icon: CalendarDays },
]);

const completedDownloadCount = computed(() => downloads.tasks.filter((task) => task.status === "done").length);
const downloadEntryTitle = computed(() => {
  const summary = downloads.summary;
  return summary ? locale.t("sidebar.tasks", { count: summary.tasks, finished: summary.finished, total: summary.total ?? locale.t("sidebar.unknown") }) : locale.t("sidebar.openDownloads");
});
const downloadEntryAria = computed(() => {
  const summary = downloads.summary;
  return summary ? locale.t("sidebar.downloadsAria", { progress: summary.percent === null ? locale.t("sidebar.progressUnknown") : `${summary.percent}%` }) : locale.t("sidebar.openDownloads");
});
const downloadStatusText = computed(() => {
  const summary = downloads.summary;
  if (summary) return summary.running > 0 ? locale.t("sidebar.active", { count: summary.running }) : locale.t("sidebar.paused");
  return completedDownloadCount.value > 0 ? locale.t("sidebar.completedCount", { count: completedDownloadCount.value }) : locale.t("sidebar.noTasks");
});

function accountName(account: KnownAccount): string {
  const full = [account.firstName, account.lastName].filter(Boolean).join(" ");
  return full || account.appleId.split("@")[0] || account.appleId;
}

function accountStatus(account: KnownAccount): string {
  if (account.lastKnownStatus === "valid") return locale.t("account.valid");
  if (account.lastKnownStatus === "expired") return locale.t("account.expired");
  return locale.t("account.unknown");
}

function accountStatusClass(account: KnownAccount): string {
  if (account.lastKnownStatus === "valid") return "text-success";
  if (account.lastKnownStatus === "expired") return "text-warning";
  return "text-muted-foreground";
}
</script>

<template>
  <aside class="flex shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground transition-[width] duration-200 select-none" :class="collapsed ? 'w-14' : 'w-[200px]'">
    <div class="flex items-center gap-1 pt-3 pb-1" :class="collapsed ? 'justify-center px-1.5' : 'px-3'">
      <p v-if="!collapsed" class="min-w-0 flex-1 truncate px-1 text-2xs font-semibold tracking-[0.08em] text-muted-foreground">{{ locale.t("sidebar.browse") }}</p>
      <button
        type="button"
        class="grid size-7 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-sidebar-accent hover:text-sidebar-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        :title="collapsed ? locale.t('sidebar.expand') : locale.t('sidebar.collapse')"
        :aria-label="collapsed ? locale.t('sidebar.expand') : locale.t('sidebar.collapse')"
        :aria-expanded="!collapsed"
        @click="toggleCollapsed"
      >
        <PanelLeftOpen v-if="collapsed" :size="14" />
        <PanelLeftClose v-else :size="14" />
      </button>
    </div>
    <nav class="flex flex-col gap-1 pt-1" :class="collapsed ? 'px-1.5' : 'px-2'">
      <button
        v-for="item in NAV_ITEMS"
        :key="item.value"
        type="button"
        class="relative flex h-9 w-full items-center rounded-md text-base transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        :class="[props.view === item.value ? 'bg-sidebar-accent font-medium text-sidebar-accent-foreground' : 'text-muted-foreground hover:bg-sidebar-accent/60 hover:text-sidebar-foreground', collapsed ? 'justify-center px-0' : 'gap-2.5 px-3']"
        :title="collapsed ? item.label : undefined"
        :aria-label="item.label"
        @click="emit('navigate', item.value)"
      >
        <span v-if="props.view === item.value" class="absolute inset-y-2 left-0 w-0.5 rounded-full bg-primary" aria-hidden="true" />
        <component :is="item.icon" :size="15" class="shrink-0" :class="props.view === item.value ? 'text-primary' : ''" />
        <span v-if="!collapsed">{{ item.label }}</span>
        <span v-if="!collapsed && item.value === 'findmy' && props.deviceCount" class="tabular ml-auto text-xs text-muted-foreground">{{ props.deviceCount }}</span>
      </button>
    </nav>

    <div class="flex-1" />

    <button
      type="button"
      class="relative mb-2 border border-sidebar-border bg-card/60 text-left shadow-sm transition-colors hover:bg-card focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
      :class="collapsed ? 'mx-1.5 grid size-10 place-items-center rounded-lg p-0' : 'mx-2 rounded-lg px-2.5 py-2'"
      :title="downloadEntryTitle"
      :aria-label="collapsed ? downloadEntryAria : undefined"
      @click="downloads.openDialog()"
    >
      <template v-if="collapsed">
        <span class="relative grid size-5 place-items-center rounded-md bg-primary/10 text-primary">
          <Download :size="13" />
          <span v-if="downloads.summary?.running" class="absolute -top-0.5 -right-0.5 size-1.5 rounded-full bg-success-solid ring-2 ring-card" />
        </span>
        <span class="absolute right-1.5 bottom-1 left-1.5 h-0.5 overflow-hidden rounded-full bg-sidebar-accent">
          <span class="block h-full rounded-full bg-primary transition-[width] duration-200" :style="{ width: `${downloads.summary?.percent ?? 0}%` }" />
        </span>
      </template>
      <template v-else>
        <span class="flex items-center gap-1.5 text-xs font-medium">
          <span class="grid size-5 shrink-0 place-items-center rounded-md bg-primary/10 text-primary"><Download :size="12" /></span>
          <span class="truncate">{{ locale.t("sidebar.downloads") }}</span>
          <span v-if="downloads.downloadSpeed > 0" class="tabular ml-auto shrink-0 text-2xs text-primary">{{ formatSpeed(downloads.downloadSpeed) }}</span>
          <span class="tabular shrink-0 text-xs text-muted-foreground" :class="downloads.downloadSpeed > 0 ? '' : 'ml-auto'">{{ downloads.summary?.percent == null ? "—" : `${downloads.summary.percent}%` }}</span>
        </span>
        <span class="mt-1.5 flex items-center justify-between gap-2 text-2xs text-muted-foreground">
          <span>{{ downloadStatusText }}</span>
          <span v-if="downloads.summary" class="tabular">{{ downloads.summary.finished }} / {{ downloads.summary.total ?? locale.t("sidebar.unknown") }}</span>
        </span>
        <span class="mt-2 block h-1.5 w-full overflow-hidden rounded-full bg-sidebar-accent">
          <span class="block h-full rounded-full bg-primary transition-[width] duration-200" :style="{ width: `${downloads.summary?.percent ?? 0}%` }" />
        </span>
      </template>
    </button>

    <div :class="collapsed ? 'px-1.5 pb-2' : 'px-3 pb-2'">
      <p v-if="!collapsed" class="pb-1.5 text-2xs font-semibold tracking-[0.08em] text-muted-foreground">{{ locale.t("theme.label") }}</p>
      <div class="rounded-md bg-sidebar-accent p-0.5" :class="collapsed ? 'flex flex-col gap-0.5' : 'flex gap-0.5'">
        <button
          v-for="option in THEME_OPTIONS"
          :key="option.value"
          type="button"
          :title="option.label"
          :aria-label="option.label"
          :aria-pressed="theme.mode === option.value"
          class="grid h-6 place-items-center rounded-[5px] transition-colors focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          :class="[theme.mode === option.value ? 'bg-card text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground', collapsed ? 'w-full' : 'flex-1']"
          @click="theme.setMode(option.value, $event)"
        >
          <component :is="option.icon" :size="13" />
        </button>
      </div>
    </div>

    <div ref="menuRoot" class="relative border-t border-sidebar-border py-2.5" :class="collapsed ? 'px-1.5' : 'px-3'">
      <div v-if="accountsOpen" id="account-switcher" role="dialog" :aria-label="locale.t('sidebar.switchAccountAria')" class="absolute bottom-full left-1.5 z-50 mb-1 w-64 overflow-hidden rounded-lg border border-border bg-card text-card-foreground shadow-xl" @keydown.esc="accountsOpen = false">
        <div class="flex items-center justify-between border-b border-border px-3 py-2">
          <span class="text-sm font-semibold">{{ locale.t("account.switch") }}</span>
          <span v-if="accountsLoading" class="text-2xs text-muted-foreground">{{ locale.t("account.checking") }}</span>
        </div>

        <div role="listbox" :aria-label="locale.t('sidebar.knownAccountsAria')" class="max-h-64 overflow-y-auto p-1">
          <button
            v-for="item in accounts"
            :key="item.key"
            type="button"
            role="option"
            :aria-selected="item.key === activeAccountKey"
            class="flex w-full items-center gap-2 rounded-md px-2 py-2 text-left transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            :aria-label="locale.t('sidebar.switchAccountItemAria', { account: item.appleId, status: accountStatus(item) })"
            @click="selectAccount(item)"
          >
            <img v-if="avatars[item.key]" :src="avatars[item.key]" alt="" class="size-7 shrink-0 rounded-full object-cover" />
            <span v-else class="grid size-7 shrink-0 place-items-center rounded-full bg-sidebar-accent text-xs font-semibold">{{ accountName(item).charAt(0).toUpperCase() }}</span>
            <span class="min-w-0 flex-1">
              <span class="block truncate text-sm font-medium">{{ accountName(item) }}</span>
              <span class="block truncate text-2xs text-muted-foreground" :title="item.appleId">{{ item.appleId }} · {{ recentTimeLabel(item.lastUsedAt, Date.now(), locale.isEnglish) }}</span>
            </span>
            <span class="shrink-0 text-right">
              <span v-if="item.key === activeAccountKey" class="sr-only">{{ locale.t("sidebar.current") }}</span>
              <Check v-if="item.key === activeAccountKey" :size="13" class="ml-auto text-primary" />
              <span class="mt-0.5 block text-2xs" :class="accountStatusClass(item)">{{ accountStatus(item) }}</span>
            </span>
          </button>

          <p v-if="accounts.length === 0" class="px-2 py-2 text-xs text-muted-foreground">{{ locale.t("account.none") }}</p>

          <button type="button" class="mt-0.5 flex h-8 w-full items-center gap-2 rounded-md px-2 text-sm text-primary transition-colors hover:bg-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none" @click="addAccount">
            <Plus :size="14" />
            {{ locale.t("account.add") }}
          </button>
        </div>

        <div class="border-t border-border p-1">
          <button
            type="button"
            class="flex h-8 w-full items-center gap-2 rounded-md px-2 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
            @click="
              accountsOpen = false;
              emit('logout');
            "
          >
            <LogOut :size="14" />
            {{ locale.t("account.logout") }}
          </button>
        </div>
      </div>

      <button
        type="button"
        class="flex w-full items-center rounded-md py-1 text-left transition-colors hover:bg-sidebar-accent focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
        :class="collapsed ? 'justify-center px-0' : 'gap-2'"
        :aria-expanded="accountsOpen"
        aria-controls="account-switcher"
        :title="collapsed ? `${displayName.text} · ${locale.t('account.switch')}` : locale.t('account.switch')"
        :aria-label="collapsed ? `${displayName.text}，${locale.t('account.switch')}` : locale.t('account.switch')"
        @click="toggleAccounts"
      >
        <img v-if="currentAvatar" :src="currentAvatar" alt="" class="size-7 shrink-0 rounded-full object-cover" />
        <span v-else class="grid size-7 shrink-0 place-items-center rounded-full bg-foreground text-xs font-semibold text-background">{{ currentInitial }}</span>
        <span v-if="!collapsed" class="min-w-0 flex-1">
          <span class="block truncate text-sm font-medium" :title="displayName.title || undefined">{{ displayName.text }}</span>
          <span class="block truncate text-2xs text-muted-foreground" :title="appleId || undefined">{{ appleId || locale.t("account.switch") }}</span>
        </span>
        <ChevronUp v-if="!collapsed" :size="14" class="shrink-0 text-muted-foreground transition-transform" :class="accountsOpen ? 'rotate-180' : ''" />
      </button>
    </div>
  </aside>
</template>
