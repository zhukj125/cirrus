<script setup lang="ts">
import { ref, watch } from "vue";
import { FolderOpen } from "@lucide/vue";

import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { StorageKey } from "@/constants";
import { pickDownloadDirectory } from "@/lib/api";
import { readStored, writeStored } from "@/lib/common/storage";
import { toast } from "@/lib/common/toast";
import { useDownloadsStore } from "@/stores/downloads";
import { useLocaleStore } from "@/stores/locale";

const downloads = useDownloadsStore();
const locale = useLocaleStore();
const t = locale.t;

const taskConcurrency = ref(String(downloads.maxConcurrentTasks));
const photoConcurrency = ref(String(downloads.photoConcurrency));
const speedLimitMb = ref(downloads.speedLimitBytesPerSec > 0 ? String(Number((downloads.speedLimitBytesPerSec / (1024 * 1024)).toFixed(2))) : "");
const downloadSubfolder = ref(readStored(StorageKey.downloadSubfolder) === "true");

watch(
  () => downloads.maxConcurrentTasks,
  (value) => {
    taskConcurrency.value = String(value);
  },
);

watch(
  () => downloads.photoConcurrency,
  (value) => {
    photoConcurrency.value = String(value);
  },
);

watch(
  () => downloads.speedLimitBytesPerSec,
  (value) => {
    speedLimitMb.value = value > 0 ? String(Number((value / (1024 * 1024)).toFixed(2))) : "";
  },
);

watch(downloadSubfolder, (value) => writeStored(StorageKey.downloadSubfolder, String(value)));

function saveTaskConcurrency(): void {
  const value = Number(taskConcurrency.value);
  if (!Number.isInteger(value) || value < 1 || value > 10) {
    taskConcurrency.value = String(downloads.maxConcurrentTasks);
    toast.error(t("downloads.taskConcurrencyInvalid"));
    return;
  }
  downloads.setMaxConcurrentTasks(value);
}

function savePhotoConcurrency(): void {
  const value = Number(photoConcurrency.value);
  if (!Number.isInteger(value) || value < 1 || value > 16) {
    photoConcurrency.value = String(downloads.photoConcurrency);
    toast.error(t("downloads.photoConcurrencyInvalid"));
    return;
  }
  downloads.setPhotoConcurrency(value);
}

function saveSpeedLimit(): void {
  const value = speedLimitMb.value.trim() === "" ? 0 : Number(speedLimitMb.value);
  if (!Number.isFinite(value) || value < 0) {
    speedLimitMb.value = downloads.speedLimitBytesPerSec > 0 ? String(Number((downloads.speedLimitBytesPerSec / (1024 * 1024)).toFixed(2))) : "";
    toast.error(t("downloads.speedInvalid"));
    return;
  }
  downloads.setSpeedLimit(Math.round(value * 1024 * 1024));
}

async function chooseDefaultDirectory(): Promise<void> {
  try {
    const picked = await pickDownloadDirectory(t("downloads.chooseSaveFolder"));
    if (picked) downloads.setDefaultDirectory(picked);
  } catch (caught) {
    toast.error(caught instanceof Error ? caught.message : t("downloads.folderPickFailed"));
  }
}
</script>

<template>
  <div>
    <div class="flex items-start justify-between gap-4">
      <h3 class="text-base font-semibold tracking-tight">{{ t("downloads.downloadOptionsTitle") }}</h3>
      <span class="text-2xs text-muted-foreground">{{ t("downloads.autoSave") }}</span>
    </div>

    <div class="mt-4 divide-y divide-border overflow-hidden rounded-xl border border-border bg-card">
      <section class="p-4">
        <p class="text-sm font-medium">{{ t("downloads.taskConcurrencyTitle") }}</p>
        <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ t("downloads.taskConcurrencyDesc") }}</p>
        <div class="mt-3 flex items-center gap-3">
          <input
            v-model="taskConcurrency"
            type="number"
            min="1"
            max="10"
            step="1"
            inputmode="numeric"
            class="h-9 w-32 rounded-md border border-input bg-background px-3 text-sm outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring/25"
            :aria-label="t('downloads.taskConcurrencyTitle')"
            @change="saveTaskConcurrency"
          />
          <span class="text-sm text-muted-foreground">{{ t("downloads.taskUnit") }}</span>
          <span class="text-xs text-muted-foreground">{{ t("downloads.taskConcurrencyRange") }}</span>
        </div>
      </section>

      <section class="p-4">
        <p class="text-sm font-medium">{{ t("downloads.photoConcurrencyTitle") }}</p>
        <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ t("downloads.photoConcurrencyDesc") }}</p>
        <div class="mt-3 flex items-center gap-3">
          <input
            v-model="photoConcurrency"
            type="number"
            min="1"
            max="16"
            step="1"
            inputmode="numeric"
            class="h-9 w-32 rounded-md border border-input bg-background px-3 text-sm outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring/25"
            :aria-label="t('downloads.photoConcurrencyTitle')"
            @change="savePhotoConcurrency"
          />
          <span class="text-sm text-muted-foreground">{{ t("downloads.photoUnit") }}</span>
          <span class="text-xs text-muted-foreground">{{ t("downloads.photoConcurrencyRange") }}</span>
        </div>
      </section>

      <section class="p-4">
        <div class="flex items-start justify-between gap-4">
          <div class="min-w-0">
            <p class="text-sm font-medium">{{ t("downloads.defaultFolderTitle") }}</p>
            <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ t("downloads.defaultFolderDesc") }}</p>
          </div>
          <Button variant="outline" size="sm" class="h-8 shrink-0" @click="chooseDefaultDirectory">{{ t(downloads.defaultDirectory ? "downloads.changeFolder" : "downloads.selectFolder") }}</Button>
        </div>
        <div class="mt-3 flex min-w-0 items-center gap-2 rounded-lg bg-secondary/55 px-3 py-2.5">
          <FolderOpen :size="14" class="shrink-0 text-muted-foreground" />
          <span class="min-w-0 flex-1 truncate text-xs" :class="downloads.defaultDirectory ? 'text-foreground' : 'text-muted-foreground'" :title="downloads.defaultDirectory || undefined">{{ downloads.defaultDirectory || t("downloads.folderNotSet") }}</span>
          <button v-if="downloads.defaultDirectory" type="button" class="shrink-0 text-xs text-muted-foreground transition-colors hover:text-foreground" @click="downloads.setDefaultDirectory('')">{{ t("common.clear") }}</button>
        </div>
      </section>

      <section class="p-4">
        <p class="text-sm font-medium">{{ t("downloads.speedSettingsTitle") }}</p>
        <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ t("downloads.speedSettingsDesc") }}</p>
        <div class="mt-3 flex items-center gap-2">
          <input
            v-model="speedLimitMb"
            type="number"
            min="0"
            step="0.5"
            inputmode="decimal"
            :placeholder="t('downloads.unlimitedPlaceholder')"
            class="h-9 w-52 rounded-md border border-input bg-background px-3 text-sm outline-none transition-colors focus:border-primary focus:ring-2 focus:ring-ring/25"
            :aria-label="t('downloads.speedSettingsTitle')"
            @change="saveSpeedLimit"
          />
          <span class="text-sm text-muted-foreground">MB/s</span>
        </div>
      </section>

      <section class="flex items-start justify-between gap-4 p-4">
        <div class="min-w-0">
          <p class="text-sm font-medium">{{ t("settings.downloadSubfolder") }}</p>
          <p class="mt-1 text-xs leading-5 text-muted-foreground">{{ t("settings.downloadSubfolderTip") }}</p>
        </div>
        <Switch id="download-subfolder-setting" v-model="downloadSubfolder" class="mt-0.5 shrink-0" :aria-label="t('settings.separateTaskFolders')" />
      </section>
    </div>
  </div>
</template>
