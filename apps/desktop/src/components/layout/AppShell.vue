<script setup lang="ts">
import type { ShellView } from "@/constants";
import type { AccountInfo, KnownAccount } from "@/types";
import { Images, MapPin, NotebookPen } from "@lucide/vue";

import AppSidebar from "./AppSidebar.vue";

withDefaults(
  defineProps<{
    view: ShellView;
    title: string;
    subtitle?: string | null;
    account?: AccountInfo | null;
    deviceCount?: number | null;
    accounts?: KnownAccount[];
    activeAccountKey?: string | null;
    accountsLoading?: boolean;
    accountAvatars?: Record<string, string>;
  }>(),
  {
    subtitle: null,
    account: null,
    deviceCount: null,
    accounts: () => [],
    activeAccountKey: null,
    accountsLoading: false,
    accountAvatars: () => ({}),
  },
);

const emit = defineEmits<{
  navigate: [view: ShellView];
  logout: [];
  switchAccount: [key: string];
  addAccount: [];
  validateAccounts: [];
}>();
</script>

<template>
  <div class="flex h-full overflow-hidden">
    <AppSidebar
      :view="view"
      :account="account"
      :device-count="deviceCount"
      :accounts="accounts"
      :active-account-key="activeAccountKey"
      :accounts-loading="accountsLoading"
      :avatars="accountAvatars"
      @navigate="emit('navigate', $event)"
      @logout="emit('logout')"
      @switch-account="emit('switchAccount', $event)"
      @add-account="emit('addAccount')"
      @validate-accounts="emit('validateAccounts')"
    />

    <main class="flex min-w-0 flex-1 flex-col bg-background">
      <header class="flex h-11 shrink-0 items-center gap-3 border-b border-border bg-card/70 px-4 select-none">
        <span class="grid size-7 shrink-0 place-items-center rounded-md border border-border/70 bg-background text-muted-foreground">
          <Images v-if="view === 'photos-gallery'" :size="14" />
          <NotebookPen v-else-if="view === 'notes'" :size="14" />
          <MapPin v-else :size="14" />
        </span>
        <div class="flex min-w-0 items-baseline gap-2">
          <h1 class="shrink-0 text-md font-semibold tracking-tight">{{ title }}</h1>
          <span v-if="subtitle" class="truncate text-xs text-muted-foreground">{{ subtitle }}</span>
        </div>
        <div class="ml-auto flex shrink-0 items-center gap-1.5">
          <slot name="actions" />
        </div>
      </header>

      <div class="min-h-0 flex-1">
        <slot />
      </div>
    </main>
  </div>
</template>
