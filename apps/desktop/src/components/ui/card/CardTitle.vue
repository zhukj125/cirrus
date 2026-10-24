<script setup lang="ts">
import type { HTMLAttributes } from "vue";

import { cn } from "@/lib/common/utils";

const props = defineProps<{ class?: HTMLAttributes["class"] }>();
</script>

<template>
  <h3 :class="cn('text-base leading-none font-semibold tracking-tight', props.class)">
    <slot />
  </h3>
</template>
