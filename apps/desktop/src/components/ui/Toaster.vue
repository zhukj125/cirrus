<script setup lang="ts">
import { CircleCheck, CircleX, Info, TriangleAlert, X } from "@lucide/vue";

import { useToasts, toast, type ToastKind } from "@/lib/common/toast";

const items = useToasts();

const ICONS = {
  success: CircleCheck,
  error: CircleX,
  warning: TriangleAlert,
  info: Info,
} as const satisfies Record<ToastKind, unknown>;

const TONES: Record<ToastKind, string> = {
  success: "text-success",
  error: "text-destructive",
  warning: "text-warning",
  info: "text-primary",
};
</script>

<template>
  <div class="pointer-events-none fixed right-4 bottom-4 z-[3000] flex w-[320px] flex-col gap-2">
    <TransitionGroup enter-active-class="transition duration-200 ease-out" enter-from-class="translate-y-2 opacity-0" leave-active-class="transition duration-150 ease-in" leave-to-class="translate-y-2 opacity-0">
      <div v-for="item in items" :key="item.id" class="pointer-events-auto flex items-start gap-2.5 rounded-lg border border-border bg-elevated px-3 py-2.5 shadow-lg">
        <component :is="ICONS[item.kind]" :class="['mt-px size-3.5 shrink-0', TONES[item.kind]]" />
        <p class="flex-1 text-base leading-5 break-words text-elevated-foreground">{{ item.message }}</p>
        <button type="button" class="-mt-0.5 shrink-0 text-muted-foreground transition-colors hover:text-foreground" aria-label="Close" @click="toast.dismiss(item.id)">
          <X class="size-3.5" />
        </button>
      </div>
    </TransitionGroup>
  </div>
</template>
