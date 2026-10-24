<script setup lang="ts">
import { APP_NAME } from "@/constants";

withDefaults(defineProps<{ size?: "sm" | "lg" }>(), { size: "sm" });
</script>

<template>
  <span class="brand-wordmark" :class="`brand-wordmark--${size}`" role="img" :aria-label="APP_NAME">
    <span class="brand-wordmark__name">{{ APP_NAME }}</span>
  </span>
</template>

<style scoped>
.brand-wordmark {
  display: inline-flex;
  align-items: center;
  color: currentColor;
  font-family: Bahnschrift, "Segoe UI Variable Display", "Aptos Display", "SF Pro Display", "Segoe UI", sans-serif;
  font-weight: 600;
  font-stretch: 92%;
  line-height: 1;
  user-select: none;
}

.brand-wordmark--sm {
  font-size: 12px;
  letter-spacing: 0.055em;
}

.brand-wordmark--lg {
  font-size: 28px;
  letter-spacing: 0.035em;
}

.brand-wordmark__name {
  display: inline-block;
  transform: translateY(0.01em);
}
</style>
