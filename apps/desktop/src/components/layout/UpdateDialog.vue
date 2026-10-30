<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { AlertTriangle, CheckCircle2, ChevronDown, Download as DownloadIcon, ExternalLink, Loader2, RefreshCw, RotateCcw, X } from "@lucide/vue";

import { Button } from "@/components/ui/button";
import { openExternal } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { fetchAppReleases, isOlderVersion, type AppRelease } from "@/lib/updates/releases";
import { useLocaleStore } from "@/stores/locale";
import { useUpdaterStore } from "@/stores/updater";

type UpdateTab = "app" | "changelog";

const props = defineProps<{
  currentVersion?: string | null;
}>();

const emit = defineEmits<{ close: [] }>();
const locale = useLocaleStore();
const updater = useUpdaterStore();
const selectedTab = ref<UpdateTab>("app");
const historyExpanded = ref(false);
const releases = ref<AppRelease[]>([]);
const historyLoading = ref(false);
const historyError = ref<string | null>(null);

const currentVersion = computed(() => updater.info?.currentVersion || props.currentVersion || "");
const olderReleases = computed(() => releases.value.filter((release) => currentVersion.value && isOlderVersion(release.version, currentVersion.value)));
const latestRelease = computed(() => releases.value[0] ?? null);
const releaseUrl = computed(() => {
  const version = updater.info?.version;
  return version ? `https://github.com/zhukj125/cirrus/releases/tag/${version.replace(/^v/, "")}` : "https://github.com/zhukj125/cirrus/releases";
});
const closeBlocked = computed(() => updater.installing);
const canCheck = computed(() => !updater.checking && !updater.downloading && !updater.installing);

function close(): void {
  if (!closeBlocked.value) emit("close");
}

function openRelease(url = releaseUrl.value): void {
  void openExternal(url).catch(() => toast.error(locale.t("updates.releaseLinkFailed")));
}

function check(): void {
  if (canCheck.value) void updater.check();
}

function download(): void {
  void updater.download();
}

function install(): void {
  void updater.install();
}

function restart(): void {
  void updater.restart();
}

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "—";
  return new Intl.DateTimeFormat(locale.locale, { dateStyle: "medium" }).format(date);
}

async function loadReleases(): Promise<void> {
  if (historyLoading.value) return;
  historyLoading.value = true;
  historyError.value = null;
  try {
    releases.value = await fetchAppReleases();
  } catch (error) {
    historyError.value = error instanceof Error ? error.message : String(error);
  } finally {
    historyLoading.value = false;
  }
}

function toggleHistory(): void {
  historyExpanded.value = !historyExpanded.value;
}

function onKeydown(event: KeyboardEvent): void {
  if (event.key === "Escape") close();
}

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
  void loadReleases();
});
onUnmounted(() => window.removeEventListener("keydown", onKeydown));
</script>

<template>
  <div class="fixed inset-0 z-[4200] grid place-items-center bg-black/45 p-6 backdrop-blur-[2px]" @click.self="close">
    <div class="flex max-h-[min(720px,calc(100vh-48px))] w-full max-w-[760px] flex-col overflow-hidden rounded-xl border border-border bg-card text-card-foreground shadow-2xl" role="dialog" aria-modal="true" aria-labelledby="update-dialog-title">
      <div class="flex items-center gap-3 border-b border-border px-5 py-4">
        <h2 id="update-dialog-title" class="flex min-w-0 items-center gap-2 text-lg font-semibold">
          <span>{{ locale.t("updates.centerTitle") }}</span>
          <span v-if="updater.hasUpdate" class="rounded-full bg-primary/10 px-2 py-0.5 text-xs font-medium text-primary">{{ locale.t("updates.updateCount", { count: 1 }) }}</span>
        </h2>
        <button v-if="!closeBlocked" type="button" class="ml-auto grid size-8 shrink-0 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" :aria-label="locale.t('common.close')" @click="close">
          <X :size="17" />
        </button>
      </div>

      <div class="flex shrink-0 items-center gap-1 border-b border-border px-5 pt-1.5" role="tablist" :aria-label="locale.t('updates.centerTitle')">
        <button
          type="button"
          role="tab"
          :aria-selected="selectedTab === 'app'"
          class="relative h-9 px-2.5 text-sm font-medium transition-colors"
          :class="selectedTab === 'app' ? 'text-foreground after:absolute after:inset-x-2.5 after:bottom-0 after:h-0.5 after:rounded-full after:bg-foreground' : 'text-muted-foreground hover:text-foreground'"
          @click="selectedTab = 'app'"
        >
          {{ locale.t("updates.appTab") }}
        </button>
        <button
          type="button"
          role="tab"
          :aria-selected="selectedTab === 'changelog'"
          class="relative h-9 px-2.5 text-sm font-medium transition-colors"
          :class="selectedTab === 'changelog' ? 'text-foreground after:absolute after:inset-x-2.5 after:bottom-0 after:h-0.5 after:rounded-full after:bg-foreground' : 'text-muted-foreground hover:text-foreground'"
          @click="selectedTab = 'changelog'"
        >
          {{ locale.t("updates.changelogTab") }}
        </button>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto px-5 py-4 text-sm">
        <template v-if="selectedTab === 'app'">
          <p v-if="updater.info">{{ locale.t("updates.availableMessage", { current: updater.info.currentVersion, latest: updater.info.version }) }}</p>
          <p v-else-if="updater.checking" class="flex items-center gap-2 text-muted-foreground"><Loader2 :size="15" class="animate-spin" />{{ locale.t("updates.checking") }}</p>
          <p v-else-if="updater.error" class="flex items-start gap-2 text-destructive">
            <AlertTriangle :size="15" class="mt-0.5 shrink-0" /><span>{{ locale.t("updates.error", { error: updater.error }) }}</span>
          </p>
          <p v-else-if="updater.checked" class="flex items-center gap-2 text-muted-foreground"><CheckCircle2 :size="15" class="text-success" />{{ locale.t("updates.dialogUpToDate", { version: currentVersion || "—" }) }}</p>

          <div v-if="updater.info" class="mt-3 max-h-64 overflow-y-auto rounded-md border border-border bg-muted/30 p-3">
            <p class="mb-1.5 text-xs font-medium">{{ locale.t("updates.notes") }}</p>
            <div class="text-xs leading-5 whitespace-pre-line text-muted-foreground">{{ updater.info.body || locale.t("updates.noNotes") }}</div>
          </div>

          <div v-if="updater.downloading" class="mt-3 space-y-2 rounded-md border border-border bg-muted/30 p-3">
            <div class="flex items-center justify-between text-xs">
              <span class="text-muted-foreground">{{ updater.progress === null ? locale.t("updates.progressUnknown") : locale.t("updates.downloading") }}</span>
              <span class="tabular font-medium">{{ updater.progress === null ? "—" : `${updater.progress}%` }}</span>
            </div>
            <div class="h-1.5 overflow-hidden rounded-full bg-secondary">
              <div class="h-full rounded-full bg-primary transition-[width] duration-200" :class="updater.progress === null ? 'w-1/3 animate-pulse' : ''" :style="updater.progress === null ? undefined : { width: `${updater.progress}%` }" />
            </div>
          </div>

          <p v-if="updater.downloaded" class="mt-3 flex items-center gap-2 text-success"><CheckCircle2 :size="15" />{{ locale.t("updates.downloaded") }}</p>
          <p v-if="updater.installed" class="mt-3 flex items-center gap-2 text-success"><CheckCircle2 :size="15" />{{ locale.t("updates.installed") }}</p>

          <div class="mt-4 border-t border-border pt-3">
            <button type="button" class="flex w-full items-center justify-between gap-2 text-xs font-medium text-muted-foreground transition-colors hover:text-foreground" @click="toggleHistory">
              <span>{{ locale.t("updates.historyTab") }}</span>
              <ChevronDown :size="14" class="transition-transform" :class="historyExpanded ? 'rotate-180' : ''" />
            </button>
            <div v-if="historyExpanded" class="mt-3 space-y-2">
              <p class="text-xs text-muted-foreground">{{ locale.t("updates.historyHint") }}</p>
              <div v-if="historyLoading" class="flex items-center justify-center gap-2 py-6 text-muted-foreground"><Loader2 :size="15" class="animate-spin" />{{ locale.t("updates.loadingHistory") }}</div>
              <div v-else-if="historyError" class="rounded-md border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive">
                <p>{{ locale.t("updates.historyLoadFailed") }}</p>
                <Button variant="outline" size="sm" class="mt-2" @click="loadReleases">{{ locale.t("updates.retry") }}</Button>
              </div>
              <div v-else-if="olderReleases.length === 0" class="py-4 text-center text-xs text-muted-foreground">{{ locale.t("updates.noHistory") }}</div>
              <div v-else class="divide-y divide-border overflow-hidden rounded-md border border-border">
                <div v-for="release in olderReleases" :key="release.tag" class="flex items-center justify-between gap-4 px-3 py-2">
                  <span class="min-w-0 truncate text-sm font-medium">{{ release.tag }}</span>
                  <span class="flex shrink-0 items-center gap-3">
                    <span class="text-xs text-muted-foreground">{{ formatDate(release.date) }}</span>
                    <Button variant="outline" size="sm" @click="openRelease(release.url)">{{ locale.t("updates.openRelease") }}</Button>
                  </span>
                </div>
              </div>
            </div>
          </div>
        </template>

        <template v-else>
          <div v-if="historyLoading" class="flex items-center justify-center gap-2 py-12 text-muted-foreground"><Loader2 :size="16" class="animate-spin" />{{ locale.t("updates.loadingHistory") }}</div>
          <div v-else-if="historyError" class="rounded-md border border-destructive/30 bg-destructive/5 p-3 text-xs text-destructive">
            <p>{{ locale.t("updates.historyLoadFailed") }}</p>
            <Button variant="outline" size="sm" class="mt-2" @click="loadReleases">{{ locale.t("updates.retry") }}</Button>
          </div>
          <div v-else-if="latestRelease">
            <div class="flex flex-wrap items-center justify-between gap-3">
              <div>
                <h3 class="text-base font-semibold">{{ latestRelease.name }}</h3>
                <p class="mt-1 text-xs text-muted-foreground">{{ formatDate(latestRelease.date) }}</p>
              </div>
              <Button variant="outline" size="sm" @click="openRelease(latestRelease.url)"><ExternalLink :size="13" />{{ locale.t("updates.openRelease") }}</Button>
            </div>
            <div class="mt-4 whitespace-pre-line text-xs leading-5 text-muted-foreground">{{ latestRelease.body || locale.t("updates.noNotes") }}</div>
          </div>
          <div v-else class="py-12 text-center text-muted-foreground">{{ locale.t("updates.noNotes") }}</div>
        </template>
      </div>

      <div class="flex flex-wrap items-center gap-2 border-t border-border bg-muted/20 px-4 py-3">
        <span class="mr-auto text-xs text-muted-foreground">{{ locale.t("settings.version") }} v{{ currentVersion || "—" }}</span>
        <Button v-if="!updater.hasUpdate && canCheck" variant="ghost" size="sm" @click="check"><RefreshCw :size="13" />{{ locale.t("updates.checkAgain") }}</Button>
        <Button v-if="updater.installed" size="sm" @click="restart"><RotateCcw :size="13" />{{ locale.t("updates.restart") }}</Button>
        <template v-else-if="updater.info">
          <Button v-if="updater.downloading" variant="outline" size="sm" @click="updater.cancelDownload()">{{ locale.t("updates.cancelDownload") }}</Button>
          <Button v-else-if="updater.downloaded" size="sm" :disabled="updater.installing" @click="install">
            <Loader2 v-if="updater.installing" :size="13" class="animate-spin" />
            <DownloadIcon v-else :size="13" />
            {{ updater.installing ? locale.t("updates.installing") : locale.t("updates.exitAndUpdate") }}
          </Button>
          <Button v-else-if="!updater.downloading" size="sm" @click="download"><DownloadIcon :size="13" />{{ locale.t("updates.downloadInBackground") }}</Button>
        </template>
        <Button v-if="!updater.hasUpdate" size="sm" @click="openRelease"><ExternalLink :size="13" />{{ locale.t("updates.openRelease") }}</Button>
        <Button v-else variant="outline" size="sm" @click="openRelease"><ExternalLink :size="13" />{{ locale.t("updates.openRelease") }}</Button>
      </div>
    </div>
  </div>
</template>
