<script lang="ts">
import { Airplay, Headphones, Laptop, Smartphone, Tablet, Watch } from "@lucide/vue";
import type { Component } from "vue";

function iconFor(deviceClass: string | null): Component | null {
  switch (deviceClass) {
    case "iPhone":
      return Smartphone;
    case "iPad":
      return Tablet;
    case "Mac":
      return Laptop;
    case "Watch":
      return Watch;
    case "AirPods":
      return Headphones;
    case "Accessory":
      return Airplay;
    default:
      return null;
  }
}

export function deviceEmoji(deviceClass: string | null): string {
  switch (deviceClass) {
    case "iPhone":
      return "📱";
    case "iPad":
      return "📱";
    case "Mac":
      return "💻";
    case "Watch":
      return "⌚";
    case "AirPods":
      return "🎧";
    default:
      return "📦";
  }
}
</script>

<script setup lang="ts">
import { computed } from "vue";

const props = withDefaults(defineProps<{ deviceClass: string | null; size?: number }>(), { size: 26 });

const icon = computed(() => iconFor(props.deviceClass));
</script>

<template>
  <span class="inline-grid shrink-0 place-items-center text-muted-foreground" :style="{ width: `${size}px`, height: `${size}px` }">
    <component :is="icon" v-if="icon" :size="size" :stroke-width="1.6" />
    <span v-else :style="{ fontSize: `${Math.round(size * 0.9)}px`, lineHeight: 1 }">{{ deviceEmoji(props.deviceClass) }}</span>
  </span>
</template>
