<script setup lang="ts">
import type { HTMLAttributes } from "vue";

import { cn } from "@/lib/common/utils";
import { badgeVariants, type BadgeVariants } from "./variants";

const props = withDefaults(defineProps<{ variant?: BadgeVariants["variant"]; class?: HTMLAttributes["class"] }>(), { variant: "default" });
</script>

<template>
  <span :class="cn(badgeVariants({ variant }), props.class)">
    <slot />
  </span>
</template>
