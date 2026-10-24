<script setup lang="ts">
import type { HTMLAttributes } from "vue";

import { cn } from "@/lib/common/utils";

const model = defineModel<string | number | undefined>();

const props = withDefaults(
  defineProps<{
    class?: HTMLAttributes["class"];
    type?: string;
    placeholder?: string;
    autocomplete?: string;
    inputmode?: "text" | "numeric" | "decimal" | "email" | "search" | "tel" | "url";
    maxlength?: number;
    disabled?: boolean;
    autofocus?: boolean;
  }>(),
  { type: "text", disabled: false, autofocus: false },
);
</script>

<template>
  <input
    v-model="model"
    :type="type"
    :placeholder="placeholder"
    :autocomplete="autocomplete"
    :inputmode="inputmode"
    :maxlength="maxlength"
    :disabled="disabled"
    :autofocus="autofocus"
    :class="
      cn(
        'flex h-8 w-full min-w-0 rounded-md border border-input bg-card px-2.5 py-1 text-base outline-none transition-[color,box-shadow]',
        'placeholder:text-muted-foreground',
        'focus-visible:border-primary focus-visible:ring-[3px] focus-visible:ring-ring',
        'disabled:cursor-not-allowed disabled:opacity-50',
        props.class,
      )
    "
  />
</template>
