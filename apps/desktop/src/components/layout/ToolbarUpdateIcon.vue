<script setup lang="ts">
import { computed } from "vue";
import { Cloud, CloudDownload } from "@lucide/vue";

const props = defineProps<{
  loading: boolean;
  downloading?: boolean;
  progress?: number | null;
}>();

const progressRingStyle = computed(() => {
  const percent = props.progress == null ? 25 : Math.round(Math.min(100, Math.max(0, props.progress)));
  return { background: `conic-gradient(var(--primary) ${percent}%, transparent ${percent}%)` };
});
</script>

<template>
  <span class="relative block size-4" aria-hidden="true">
    <span v-if="props.downloading" class="update-progress absolute inset-0 rounded-full" :class="{ 'update-progress--indeterminate': props.progress == null }" :style="progressRingStyle" />
    <CloudDownload v-else-if="!props.loading" :size="16" />
    <template v-else>
      <Cloud :size="16" class="absolute inset-0 opacity-40" />
      <Cloud :size="16" class="update-outline absolute inset-0" />
    </template>
  </span>
</template>

<style scoped>
.update-progress {
  mask: radial-gradient(farthest-side, transparent calc(100% - 1.75px), #000 calc(100% - 1.75px));
  -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 1.75px), #000 calc(100% - 1.75px));
}

.update-progress--indeterminate,
.update-outline {
  animation: update-progress-spin 1100ms linear infinite;
}

.update-outline {
  stroke-dasharray: 7 36;
  stroke-dashoffset: 0;
  animation-name: update-outline-scan;
}

@keyframes update-outline-scan {
  to {
    stroke-dashoffset: -43;
  }
}

@keyframes update-progress-spin {
  to {
    transform: rotate(360deg);
  }
}

@media (prefers-reduced-motion: reduce) {
  .update-progress--indeterminate,
  .update-outline {
    animation: none;
  }
}
</style>
