<script setup lang="ts">
import type { HTMLAttributes } from "vue";
import { SwitchRoot, SwitchThumb } from "reka-ui";

import { cn } from "@/lib/common/utils";

const model = defineModel<boolean>({ default: false });

const props = withDefaults(defineProps<{ class?: HTMLAttributes["class"]; disabled?: boolean; id?: string }>(), { disabled: false });
</script>

<template>
  <SwitchRoot
    :id="props.id"
    v-model="model"
    :disabled="disabled"
    :class="
      cn(
        'peer inline-flex h-[1.15rem] w-8 shrink-0 items-center rounded-full border border-transparent shadow-xs transition-all outline-none',
        'focus-visible:border-ring focus-visible:ring-[3px] focus-visible:ring-ring/40',
        'data-[state=checked]:bg-primary data-[state=unchecked]:bg-input',
        'disabled:cursor-not-allowed disabled:opacity-50',
        props.class,
      )
    "
  >
    <SwitchThumb :class="cn('pointer-events-none block size-4 rounded-full bg-background ring-0 transition-transform', 'data-[state=checked]:translate-x-[calc(100%-2px)] data-[state=unchecked]:translate-x-0')" />
  </SwitchRoot>
</template>
