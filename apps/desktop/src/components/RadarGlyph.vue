<script setup lang="ts">
withDefaults(defineProps<{ size?: number; color?: string }>(), { size: 24, color: "currentColor" });
</script>

<template>
  <svg :width="size" :height="size" viewBox="0 0 24 24" fill="none" aria-hidden="true" :style="{ color }">
    <circle cx="12" cy="12" r="8.6" stroke="currentColor" stroke-width="1.4" opacity="0.28" />
    <circle cx="12" cy="12" r="5.2" stroke="currentColor" stroke-width="1.4" opacity="0.5" />

    <path d="M12 12 12 3.4A8.6 8.6 0 0 1 20.47 13.49Z" fill="currentColor" opacity="0.16" />
    <circle cx="12" cy="12" r="1.9" fill="currentColor" />
  </svg>
</template>
