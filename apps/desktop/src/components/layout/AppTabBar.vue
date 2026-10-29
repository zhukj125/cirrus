<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { CalendarDays, ChevronDown, Images, MapPin, NotebookPen, Plus, Settings2, Users, X } from "@lucide/vue";

import { SHELL_VIEWS, type ShellView, type WorkspaceTab } from "@/constants";
import { useLocaleStore } from "@/stores/locale";

const props = defineProps<{
  tabs: WorkspaceTab[];
  active: WorkspaceTab | null;
}>();

const emit = defineEmits<{
  select: [tab: WorkspaceTab];
  close: [tab: WorkspaceTab];
  add: [view: ShellView];
}>();

const locale = useLocaleStore();
const addOpen = ref(false);
const addRoot = ref<HTMLElement | null>(null);
const tabList = ref<HTMLElement | null>(null);

const shellTabCount = computed(() => props.tabs.filter((tab) => tab !== "settings").length);
const availableTabs = computed(() => SHELL_VIEWS.filter((view) => !props.tabs.includes(view)));

function tabLabel(tab: WorkspaceTab): string {
  if (tab === "settings") return locale.t("settings.title");
  if (tab === "findmy") return locale.t("nav.devices");
  if (tab === "photos-gallery") return locale.t("nav.photos");
  if (tab === "notes") return locale.t("nav.notes");
  if (tab === "contacts") return locale.t("nav.contacts");
  return locale.t("nav.calendar");
}

function tabIcon(tab: WorkspaceTab) {
  if (tab === "settings") return Settings2;
  if (tab === "photos-gallery") return Images;
  if (tab === "notes") return NotebookPen;
  if (tab === "contacts") return Users;
  if (tab === "calendar") return CalendarDays;
  return MapPin;
}

function canClose(tab: WorkspaceTab): boolean {
  return tab === "settings" || shellTabCount.value > 1;
}

function closeTab(tab: WorkspaceTab): void {
  if (!canClose(tab)) return;
  emit("close", tab);
}

function addTab(view: ShellView): void {
  addOpen.value = false;
  emit("add", view);
}

function onDocumentPointerDown(event: PointerEvent): void {
  if (!addOpen.value || !addRoot.value) return;
  if (!addRoot.value.contains(event.target as Node)) addOpen.value = false;
}

function focusAdjacentTab(index: number, direction: -1 | 1): void {
  const nodes = tabList.value?.querySelectorAll<HTMLButtonElement>("[data-workspace-tab]");
  if (!nodes?.length) return;
  const next = (index + direction + nodes.length) % nodes.length;
  nodes[next]?.focus();
}

onMounted(() => document.addEventListener("pointerdown", onDocumentPointerDown));
onUnmounted(() => document.removeEventListener("pointerdown", onDocumentPointerDown));
</script>

<template>
  <div class="flex h-9 shrink-0 items-stretch border-b border-border bg-card/55" data-testid="workspace-tabs">
    <div ref="tabList" role="tablist" :aria-label="locale.t('tabs.aria')" class="flex min-w-0 flex-1 items-stretch gap-1 overflow-x-auto px-2 py-1">
      <div
        v-for="(tab, index) in tabs"
        :key="tab"
        class="group relative flex min-w-[116px] max-w-[210px] items-stretch rounded-md border transition-colors"
        :class="active === tab ? 'border-border bg-background text-foreground shadow-sm' : 'border-transparent text-muted-foreground hover:bg-accent/70 hover:text-foreground'"
      >
        <button
          type="button"
          role="tab"
          data-workspace-tab
          :aria-selected="active === tab"
          :tabindex="active === tab ? 0 : -1"
          class="flex min-w-0 flex-1 items-center gap-1.5 rounded-md py-0 pr-1 pl-2.5 text-left text-xs font-medium focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          @click="emit('select', tab)"
          @keydown.left.prevent="focusAdjacentTab(index, -1)"
          @keydown.right.prevent="focusAdjacentTab(index, 1)"
          @keydown.delete.prevent="closeTab(tab)"
        >
          <component :is="tabIcon(tab)" :size="13" class="shrink-0" />
          <span class="min-w-0 truncate">{{ tabLabel(tab) }}</span>
        </button>
        <button
          v-if="canClose(tab)"
          type="button"
          class="my-1 mr-1 grid size-5 shrink-0 place-items-center rounded text-muted-foreground opacity-60 transition-[background-color,color,opacity] hover:bg-secondary hover:text-foreground hover:opacity-100 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none group-hover:opacity-100"
          :aria-label="locale.t('tabs.close', { title: tabLabel(tab) })"
          :title="locale.t('tabs.close', { title: tabLabel(tab) })"
          @click.stop="closeTab(tab)"
        >
          <X :size="12" />
        </button>
        <span v-if="active === tab" class="pointer-events-none absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-primary" />
      </div>
    </div>

    <div ref="addRoot" class="relative flex shrink-0 items-center border-l border-border px-1.5">
      <button
        type="button"
        class="grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none disabled:cursor-default disabled:opacity-35"
        :disabled="availableTabs.length === 0"
        :aria-label="locale.t('tabs.open')"
        :title="locale.t('tabs.open')"
        :aria-expanded="addOpen"
        @click="addOpen = !addOpen"
      >
        <Plus :size="14" />
      </button>
      <ChevronDown :size="10" class="pointer-events-none -ml-1.5 text-muted-foreground" />

      <div v-if="addOpen" class="absolute top-full right-0 z-50 mt-1 w-44 overflow-hidden rounded-lg border border-border bg-card p-1 text-card-foreground shadow-xl">
        <button
          v-for="view in availableTabs"
          :key="view"
          type="button"
          class="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
          @click="addTab(view)"
        >
          <component :is="tabIcon(view)" :size="14" class="shrink-0" />
          <span class="truncate">{{ tabLabel(view) }}</span>
        </button>
        <p v-if="availableTabs.length === 0" class="px-2 py-2 text-xs text-muted-foreground">{{ locale.t("tabs.allOpen") }}</p>
      </div>
    </div>
  </div>
</template>
