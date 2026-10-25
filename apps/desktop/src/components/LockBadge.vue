<script lang="ts">
import type { Device } from "@/types";

export function hasLockBadge(device: Device): boolean {
  return Boolean(device.lostModeEnabled) || Boolean(device.lostModePending);
}

export function deviceLockLabel(device: Device, english = true): string {
  if (device.lostModeEnabled) return english ? "Lost Mode is on" : "丢失模式已开启";
  if (device.lostModePending) return english ? "Updating Lost Mode…" : "正在更新丢失模式…";
  if (device.activationLocked) return english ? "Activation Lock is on" : "激活锁已开启";
  return english ? "Activation Lock is off" : "激活锁已关闭";
}
</script>

<script setup lang="ts">
import { computed } from "vue";
import { LockKeyhole } from "@lucide/vue";

import { useLocaleStore } from "@/stores/locale";

const locale = useLocaleStore();

const props = withDefaults(defineProps<{ device: Device; size?: number }>(), { size: 20 });

const show = computed(() => hasLockBadge(props.device));
const label = computed(() => deviceLockLabel(props.device, locale.isEnglish));
</script>

<template>
  <span v-if="show" class="grid shrink-0 place-items-center rounded-full shadow-sm" :style="{ width: `${size}px`, height: `${size}px`, background: 'var(--warning-solid)' }" role="img" :aria-label="label" :title="label">
    <LockKeyhole :size="Math.round(size * 0.62)" :stroke-width="2.4" class="text-elevated-foreground" />
  </span>
</template>
