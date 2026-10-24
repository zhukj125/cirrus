import { ref } from "vue";

export type ToastKind = "success" | "error" | "info" | "warning";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  message: string;
}

const items = ref<ToastItem[]>([]);
let sequence = 0;

function push(kind: ToastKind, message: string, durationMs = 3600): number {
  const id = ++sequence;
  items.value = [...items.value, { id, kind, message }];
  if (durationMs > 0) {
    setTimeout(() => dismiss(id), durationMs);
  }
  return id;
}

export function dismiss(id: number): void {
  items.value = items.value.filter((item) => item.id !== id);
}

export const toast = {
  success: (message: string, durationMs?: number) => push("success", message, durationMs),
  error: (message: string, durationMs?: number) => push("error", message, durationMs),
  info: (message: string, durationMs?: number) => push("info", message, durationMs),
  warning: (message: string, durationMs?: number) => push("warning", message, durationMs),
  dismiss,
};

export function useToasts() {
  return items;
}
