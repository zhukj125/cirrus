<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    level: number | null;
    charging?: boolean;
    showText?: boolean;
  }>(),
  { charging: false, showText: true },
);

const percent = computed<number | null>(() => (props.level != null && props.level > 0 ? Math.min(100, Math.round(props.level * 100)) : null));

const TONE = {
  danger: { fill: "var(--danger-solid)", text: "text-destructive" },
  warning: { fill: "var(--warning-solid)", text: "text-warning" },
  success: { fill: "var(--success-solid)", text: "text-success" },
  unknown: { fill: "var(--muted-foreground)", text: "text-muted-foreground" },
} as const;

const tone = computed(() => (percent.value == null ? TONE.unknown : percent.value <= 20 ? TONE.danger : percent.value <= 50 ? TONE.warning : TONE.success));
const title = computed(() => (percent.value != null ? `Battery ${percent.value}%${props.charging ? " (charging)" : ""}` : "Battery level unavailable"));
</script>

<template>
  <span class="inline-flex shrink-0 items-center gap-1.5" :title="title" :style="{ color: tone.fill }">
    <span class="flex items-center">
      <span class="battery-shell relative flex h-[10px] w-[20px] items-center rounded-[3px] border p-[1.5px]">
        <span class="block h-full rounded-[1.5px] bg-current transition-[width] duration-300" :style="{ width: `${percent ?? 0}%` }" />

        <svg v-if="charging" class="absolute inset-0 m-auto h-[9px] w-[7px] text-elevated-foreground" viewBox="0 0 8 12" fill="currentColor" aria-hidden="true">
          <path d="M4.7.3 1 6.7h2.4l-.6 5 4.3-6.4H4.7Z" />
        </svg>
      </span>
      <span class="battery-cap ml-px h-[4px] w-[1.5px] rounded-r-[1px]" />
    </span>

    <span v-if="showText" class="tabular text-xs" :class="tone.text">{{ percent != null ? `${percent}%` : "--" }}</span>
  </span>
</template>

<style scoped>
.battery-shell {
  border-color: color-mix(in srgb, currentColor 38%, transparent);
}

.battery-cap {
  background-color: color-mix(in srgb, currentColor 42%, transparent);
}
</style>
