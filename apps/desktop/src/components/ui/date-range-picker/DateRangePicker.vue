<script setup lang="ts">
import { computed, ref } from "vue";
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from "reka-ui";
import { CalendarRange } from "@lucide/vue";

import { addDays, formatDateInput, type DateBounds } from "@/lib/photos/dates";
import { useLocaleStore } from "@/stores/locale";

import DateRangeCalendar from "./DateRangeCalendar.vue";

const props = withDefaults(
  defineProps<{
    modelValue: DateBounds;
    placeholder?: string;
    showClear?: boolean;
  }>(),
  { showClear: true },
);

const emit = defineEmits<{ "update:modelValue": [value: DateBounds] }>();
const locale = useLocaleStore();
const t = locale.t;

const open = ref(false);
const placeholderLabel = computed(() => props.placeholder || t("photos.selectDateRange"));
const triggerLabel = computed(() => {
  const { after, before } = props.modelValue;
  if (after === null || before === null) return placeholderLabel.value;
  return `${formatDateInput(after)}${locale.isEnglish ? " to " : " 至 "}${formatDateInput(addDays(before, -1))}`;
});

function onOpenChange(next: boolean): void {
  open.value = next;
}

function onUpdate(value: DateBounds): void {
  emit("update:modelValue", value);
  open.value = false;
}
</script>

<template>
  <PopoverRoot :open="open" @update:open="onOpenChange">
    <PopoverTrigger as-child>
      <button
        type="button"
        class="flex h-8 w-[230px] items-center gap-2 rounded-md border border-input bg-card px-2.5 text-left text-xs text-foreground transition-colors hover:bg-accent focus-visible:border-primary focus-visible:ring-[2px] focus-visible:ring-ring/40"
        :title="placeholderLabel"
        :aria-label="placeholderLabel"
      >
        <CalendarRange :size="13" class="shrink-0 text-muted-foreground" />
        <span class="tabular truncate" :class="props.modelValue.after === null && !open ? 'text-muted-foreground' : ''">{{ triggerLabel }}</span>
      </button>
    </PopoverTrigger>

    <PopoverPortal>
      <PopoverContent align="start" :side-offset="6" :collision-padding="8" class="z-[3200] w-[310px] rounded-xl border border-border bg-popover p-3 text-popover-foreground shadow-2xl outline-none">
        <DateRangeCalendar :model-value="modelValue" :show-clear="showClear" @update:model-value="onUpdate" @cancel="open = false" />
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>
