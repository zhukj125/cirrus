<script lang="ts">
import type { Device } from "@/types";

export function hasLockBadge(device: Device): boolean {
  return Boolean(device.lostModeEnabled) || Boolean(device.lostModePending);
}

export function deviceLockLabel(device: Device): string {
  if (device.lostModeEnabled) return "Lost Mode is on";
  if (device.lostModePending) return "Updating Lost Mode…";
  if (device.activationLocked) return "Activation Lock is on";
  return "Activation Lock is off";
}
</script>

<script setup lang="ts">
import { computed } from "vue";
import { LockKeyhole } from "@lucide/vue";

const props = withDefaults(defineProps<{ device: Device; size?: number }>(), { size: 20 });

const show = computed(() => hasLockBadge(props.device));
const label = computed(() => deviceLockLabel(props.device));
</script>

<template>
  <span v-if="show" class="grid shrink-0 place-items-center rounded-full shadow-sm" :style="{ width: `${size}px`, height: `${size}px`, background: 'var(--warning-solid)' }" role="img" :aria-label="label" :title="label">
    <LockKeyhole :size="Math.round(size * 0.62)" :stroke-width="2.4" class="text-elevated-foreground" />
  </span>
</template>
