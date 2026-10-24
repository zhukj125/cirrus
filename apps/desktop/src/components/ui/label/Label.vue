<script setup lang="ts">
import type { HTMLAttributes } from "vue";

import { cn } from "@/lib/common/utils";

const props = defineProps<{ class?: HTMLAttributes["class"]; for?: string }>();
</script>

<template>
  <label :for="props.for" :class="cn('flex items-center gap-2 text-sm leading-none font-medium select-none', props.class)">
    <slot />
  </label>
</template>
