<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from "vue";
import { ArrowRight, MapPin, ShieldCheck } from "@lucide/vue";

import { Status } from "@/constants";
import { photos2faOptions, photosCheckApproval, photosLogin, photosSendCode, photosStatus, photosVerify2fa } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { useLocaleStore } from "@/stores/locale";
import Spinner from "@/components/ui/Spinner.vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import type { KnownAccount, LoginContext } from "@/types";

const props = withDefaults(
  defineProps<{
    accounts?: KnownAccount[];
    activeAccountKey?: string | null;
    avatars?: Record<string, string>;
    loginContext?: LoginContext;
    pendingReloginKey?: string | null;
    accountsLoading?: boolean;
  }>(),
  {
    accounts: () => [],
    activeAccountKey: null,
    avatars: () => ({}),
    loginContext: () => ({ mode: "normal", appleId: null }),
    pendingReloginKey: null,
    accountsLoading: false,
  },
);

const emit = defineEmits<{ success: [sessionId: string]; selectAccount: [key: string]; switchOther: [] }>();
const locale = useLocaleStore();

const POLL_INTERVAL_MS = 2000;
const POLL_LIMIT = 150;

const appleId = ref(props.loginContext.appleId ?? "");
const password = ref("");
const chinaMainland = ref(true);
const loading = ref(false);

const reloginConfirmed = ref(false);

const CODE_LENGTH = 6;
const codeSlots = Array.from({ length: CODE_LENGTH }, (_, index) => index);
const pendingSessionId = ref<string | null>(null);
const codeDigits = ref<string[]>(Array.from({ length: CODE_LENGTH }, () => ""));
const code = computed(() => codeDigits.value.join(""));
const codeInputs = ref<HTMLInputElement[]>([]);
const verifying = ref(false);
const resending = ref(false);
const polling = ref(false);
const statusError = ref<string | null>(null);
const deviceCount = ref<number | null>(null);
const pollCount = ref(0);
let pollTimer: ReturnType<typeof setInterval> | null = null;

const showReloginPrompt = computed(() => props.loginContext.mode === "relogin-expired" && !reloginConfirmed.value && !pendingSessionId.value);
const lockAppleId = computed(() => props.loginContext.mode === "relogin-expired" && reloginConfirmed.value);
const showFirstStep = computed(() => !pendingSessionId.value && !showReloginPrompt.value);
const showKnownAccounts = computed(() => !pendingSessionId.value && !showReloginPrompt.value && props.accounts.length > 0);

const reloginAccount = computed<KnownAccount | null>(() => {
  const byKey = props.accounts.find((account) => account.key === props.pendingReloginKey);
  if (byKey) return byKey;
  const target = props.loginContext.appleId?.trim().toLowerCase();
  return target ? (props.accounts.find((account) => account.appleId.trim().toLowerCase() === target) ?? null) : null;
});

function displayName(account: KnownAccount): string {
  const full = [account.firstName, account.lastName].filter(Boolean).join(" ");
  if (full) return full;
  return account.appleId.split("@")[0] || account.appleId;
}

function statusText(account: KnownAccount): string {
  if (account.lastKnownStatus === "valid") return locale.t("account.valid");
  if (account.lastKnownStatus === "expired") return locale.t("account.expired");
  return locale.t("account.unknown");
}

function statusClass(account: KnownAccount): string {
  if (account.lastKnownStatus === "valid") return "text-success";
  if (account.lastKnownStatus === "expired") return "text-warning";
  return "text-muted-foreground";
}

watch(
  () => props.loginContext,
  (context) => {
    reloginConfirmed.value = false;
    password.value = "";
    if (context.mode === "relogin-expired") {
      appleId.value = context.appleId ?? "";
      return;
    }
    if (context.mode === "switch") appleId.value = "";
  },
  { immediate: true, deep: true },
);

function stopPolling(): void {
  if (pollTimer !== null) {
    clearInterval(pollTimer);
    pollTimer = null;
  }
}

function finish(sessionId: string): void {
  stopPolling();
  polling.value = false;
  emit("success", sessionId);
}

async function pollOnce(sessionId: string): Promise<void> {
  pollCount.value += 1;
  if (pollCount.value > POLL_LIMIT) {
    stopPolling();
    polling.value = false;
    statusError.value = "Two-factor authentication timed out. Resend the code or sign in again.";
    return;
  }

  const state = await photosStatus(sessionId).catch(() => null);
  if (state === null || pendingSessionId.value !== sessionId) return;
  statusError.value = state.error ?? null;
  if (state.state === Status.authenticated && state.activated) {
    finish(sessionId);
    return;
  }

  const approval = await photosCheckApproval(sessionId).catch(() => null);
  if (approval === null || pendingSessionId.value !== sessionId) return;
  if (approval.status === Status.ok) finish(sessionId);
}

watch(pendingSessionId, (sessionId) => {
  if (!sessionId) return;
  void photos2faOptions(sessionId)
    .then((options) => {
      if (pendingSessionId.value !== sessionId) return;
      if (options.status === Status.ok) deviceCount.value = options.trustedDeviceCount;
    })
    .catch(() => undefined);
});

watch(pendingSessionId, (sessionId) => {
  stopPolling();
  if (!sessionId) return;
  pollCount.value = 0;
  polling.value = true;
  pollTimer = setInterval(() => {
    void pollOnce(sessionId);
  }, POLL_INTERVAL_MS);
});

onUnmounted(() => {
  stopPolling();
});

function onSwitchRowClick(event: MouseEvent): void {
  if (event.target instanceof HTMLElement && event.target.closest("button")) return;
  chinaMainland.value = !chinaMainland.value;
}

async function handleLogin(): Promise<void> {
  const id = lockAppleId.value ? (props.loginContext.appleId ?? "").trim() : appleId.value.trim();
  if (!id) {
    toast.error("Enter your Apple ID email");
    return;
  }
  if (!password.value) {
    toast.error("Enter your password");
    return;
  }
  loading.value = true;
  try {
    const result = await photosLogin({ appleId: id, password: password.value, chinaMainland: chinaMainland.value });
    if (result.status === Status.error) {
      toast.error(result.message ?? "Sign-in failed");
      return;
    }
    if (result.status === Status.need2fa && result.sessionId) {
      pendingSessionId.value = result.sessionId;
      return;
    }
    if (result.sessionId) emit("success", result.sessionId);
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "Request failed. Try again later.");
  } finally {
    loading.value = false;
  }
}

function sanitizeCode(raw: string): string {
  return raw.replace(/\D/g, "").slice(0, CODE_LENGTH);
}

function focusCodeInput(index: number): void {
  const target = Math.min(Math.max(index, 0), CODE_LENGTH - 1);
  void nextTick(() => {
    const input = codeInputs.value[target];
    if (!input) return;
    input.focus();
    input.select();
  });
}

function maybeAutoVerify(): void {
  if (verifying.value || !codeDigits.value.every(Boolean)) return;
  void nextTick(() => void handleVerify());
}

function applyCodeDigits(startIndex: number, raw: string): void {
  const digits = sanitizeCode(raw);
  if (!digits) return;

  const start = digits.length >= CODE_LENGTH ? 0 : Math.min(Math.max(startIndex, 0), CODE_LENGTH - 1);
  const next = [...codeDigits.value];
  for (let offset = 0; offset < digits.length && start + offset < CODE_LENGTH; offset += 1) {
    next[start + offset] = digits[offset]!;
  }
  codeDigits.value = next;
  focusCodeInput(Math.min(start + digits.length, CODE_LENGTH - 1));
  maybeAutoVerify();
}

function onCodeDigitInput(index: number, event: Event): void {
  const input = event.target as HTMLInputElement;
  const raw = sanitizeCode(input.value);
  input.value = "";
  if (!raw) {
    const next = [...codeDigits.value];
    next[index] = "";
    codeDigits.value = next;
    return;
  }
  applyCodeDigits(index, raw);
}

function onCodePaste(index: number, event: ClipboardEvent): void {
  event.preventDefault();
  applyCodeDigits(index, event.clipboardData?.getData("text") ?? "");
}

function onCodeKeydown(index: number, event: KeyboardEvent): void {
  if (event.key === "Backspace") {
    event.preventDefault();
    const next = [...codeDigits.value];
    if (next[index]) {
      next[index] = "";
    } else if (index > 0) {
      next[index - 1] = "";
      focusCodeInput(index - 1);
    }
    codeDigits.value = next;
    return;
  }
  if (event.key === "Delete") {
    event.preventDefault();
    const next = [...codeDigits.value];
    next[index] = "";
    codeDigits.value = next;
    return;
  }
  if (event.key === "ArrowLeft") {
    event.preventDefault();
    focusCodeInput(index - 1);
    return;
  }
  if (event.key === "ArrowRight") {
    event.preventDefault();
    focusCodeInput(index + 1);
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    void handleVerify();
  }
}

function onCodeFocus(index: number): void {
  codeInputs.value[index]?.select();
}

async function handleVerify(): Promise<void> {
  const sessionId = pendingSessionId.value;
  if (!sessionId || verifying.value) return;
  const trimmed = code.value;
  if (!/^\d{6}$/.test(trimmed)) {
    toast.error("Enter the full 6-digit code");
    return;
  }
  verifying.value = true;
  try {
    const result = await photosVerify2fa({ sessionId, code: trimmed });
    if (result.status === Status.error) {
      toast.error(result.message ?? "Verification failed");
      return;
    }
    toast.success("Verification successful");
    finish(sessionId);
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "Request failed");
  } finally {
    verifying.value = false;
  }
}

async function handleResend(): Promise<void> {
  const sessionId = pendingSessionId.value;
  if (!sessionId) return;
  resending.value = true;
  try {
    const result = await photosSendCode(sessionId);
    if (result.status === Status.error) {
      toast.error(result.message ?? "Could not send the verification code");
      return;
    }
    toast.success("A new code was sent to your trusted device");
  } catch (error) {
    toast.error(error instanceof Error ? error.message : "Request failed");
  } finally {
    resending.value = false;
  }
}

function resetToLogin(): void {
  pendingSessionId.value = null;
  codeDigits.value = Array.from({ length: CODE_LENGTH }, () => "");
  statusError.value = null;
  deviceCount.value = null;
}

function confirmRelogin(): void {
  reloginConfirmed.value = true;
  appleId.value = props.loginContext.appleId ?? appleId.value;
}

function switchToOtherAccount(): void {
  reloginConfirmed.value = false;
  password.value = "";
  appleId.value = "";
  emit("switchOther");
}
</script>

<template>
  <div class="flex h-full min-h-0 items-center justify-center overflow-y-auto bg-background px-5 py-8 sm:px-8">
    <div class="w-full max-w-[380px]">
      <div class="mb-6">
        <h1 class="text-2xl font-semibold tracking-tight">{{ locale.t("login.title") }}</h1>
        <p class="mt-2 text-sm leading-5 text-muted-foreground">{{ locale.t("login.subtitle") }}</p>
      </div>

      <div class="relative">
        <div class="rounded-xl border border-border bg-card p-5 shadow-sm sm:p-6">
          <div v-if="showReloginPrompt" class="flex flex-col gap-4" data-testid="relogin-prompt">
            <div class="flex items-start gap-3 rounded-lg border border-warning/25 bg-warning/5 p-3.5">
              <span class="grid size-9 shrink-0 place-items-center rounded-full bg-warning/10 text-warning"><ShieldCheck :size="17" /></span>
              <div class="min-w-0">
                <p class="text-sm font-medium">{{ locale.t("login.reloginTitle") }}</p>
                <p class="mt-1 truncate text-xs text-muted-foreground" :title="loginContext.appleId ?? undefined">{{ loginContext.appleId }}</p>
                <p v-if="reloginAccount" class="mt-1 text-xs text-muted-foreground">{{ displayName(reloginAccount) }}</p>
              </div>
            </div>
            <Button type="button" class="h-10 w-full" @click="confirmRelogin">{{ locale.t("login.relogin") }}</Button>
            <Button type="button" variant="outline" class="h-10 w-full" @click="switchToOtherAccount">{{ locale.t("login.switchOther") }}</Button>
          </div>

          <form v-else-if="showFirstStep" novalidate class="flex flex-col gap-4" @submit.prevent="handleLogin">
            <div>
              <label for="apple-id" class="mb-2 block text-sm font-medium">Apple ID</label>
              <div>
                <Input id="apple-id" v-model="appleId" class="h-10 rounded-lg" placeholder="you@example.com" autocomplete="username" :readonly="lockAppleId" :aria-readonly="lockAppleId" />
              </div>
            </div>

            <div>
              <label for="apple-password" class="mb-2 block text-sm font-medium">{{ locale.t("login.password") }}</label>
              <Input id="apple-password" v-model="password" class="h-10 rounded-lg" type="password" :placeholder="locale.t('login.passwordPlaceholder')" autocomplete="current-password" />
            </div>

            <div class="flex cursor-pointer items-center justify-between rounded-lg border border-border bg-secondary/45 px-3.5 py-2.5 text-sm select-none" @click="onSwitchRowClick">
              <span class="flex items-center gap-2 text-muted-foreground"><MapPin :size="14" />{{ locale.t("login.china") }}</span>
              <Switch id="china-mainland" v-model="chinaMainland" aria-label="Mainland China account" />
            </div>

            <Button type="submit" class="mt-1 h-10 w-full rounded-lg" :disabled="loading">
              <Spinner v-if="loading" :size="13" />
              {{ loading ? locale.t("login.authenticating") : locale.t("login.submit") }}
              <ArrowRight v-if="!loading" :size="14" class="ml-auto" />
            </Button>

            <button v-if="loginContext.mode === 'relogin-expired'" type="button" class="rounded-md py-1 text-sm text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="switchToOtherAccount">{{ locale.t("login.switchOther") }}</button>
          </form>

          <div v-else class="flex flex-col gap-4">
            <div class="flex items-center gap-3">
              <span class="grid size-10 shrink-0 place-items-center rounded-full bg-primary/10 text-primary"><ShieldCheck :size="18" /></span>
              <div>
                <p class="text-sm font-medium">{{ locale.t("login.twoFactor") }}</p>
                <p class="mt-0.5 text-xs text-muted-foreground">{{ locale.t("login.codeSent") }}</p>
              </div>
            </div>

            <p class="text-sm leading-5 text-muted-foreground">{{ locale.t("login.codeHelp") }}</p>

            <div class="flex flex-col gap-1 text-xs text-muted-foreground">
              <p v-if="polling" class="flex items-center gap-1.5"><Spinner :size="11" />{{ locale.t("login.waiting") }}</p>
              <p v-if="deviceCount !== null && deviceCount >= 0">{{ locale.t("login.trustedDevices", { count: deviceCount }) }}</p>
              <p v-if="statusError" class="text-warning">{{ statusError }}</p>
            </div>

            <div class="grid grid-cols-6 gap-2" role="group" aria-label="6-digit two-factor authentication code">
              <input
                v-for="index in codeSlots"
                :key="index"
                ref="codeInputs"
                :value="codeDigits[index]"
                type="text"
                class="tabular h-12 min-w-0 rounded-lg border border-input bg-card px-0 text-center text-xl font-semibold outline-none transition-[border-color,background-color,box-shadow] placeholder:text-muted-foreground focus-visible:border-primary focus-visible:ring-[3px] focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                :class="codeDigits[index] ? 'border-primary/45 bg-primary/5' : ''"
                inputmode="numeric"
                maxlength="1"
                autocomplete="one-time-code"
                placeholder="·"
                :autofocus="index === 0"
                :aria-label="`Two-factor authentication code digit ${index + 1}`"
                @input="onCodeDigitInput(index, $event)"
                @keydown="onCodeKeydown(index, $event)"
                @paste="onCodePaste(index, $event)"
                @focus="onCodeFocus(index)"
              />
            </div>

            <Button class="h-10 w-full rounded-lg" :disabled="verifying" @click="handleVerify">
              <Spinner v-if="verifying" :size="13" />
              {{ locale.t("login.verify") }}
            </Button>

            <div class="flex items-center justify-between text-sm">
              <button type="button" class="rounded-md px-1.5 py-1 text-primary transition-colors hover:bg-accent disabled:opacity-50" :disabled="resending" @click="handleResend">{{ resending ? locale.t("login.resending") : locale.t("login.resend") }}</button>
              <button type="button" class="rounded-md px-1.5 py-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground" @click="resetToLogin">{{ locale.t("login.reloginButton") }}</button>
            </div>
          </div>
        </div>
      </div>

      <div v-if="showKnownAccounts" class="mt-4 rounded-xl border border-border bg-card/70 p-2" data-testid="known-accounts">
        <div class="flex items-center justify-between px-2 pt-1 pb-2">
          <span class="text-xs font-medium text-muted-foreground">{{ accountsLoading ? locale.t("login.checkingAccounts") : locale.t("login.existingAccounts") }}</span>
          <button type="button" class="rounded-md px-1.5 py-0.5 text-xs text-primary transition-colors hover:bg-accent" @click="switchToOtherAccount">{{ locale.t("login.addOther") }}</button>
        </div>

        <button v-for="item in accounts" :key="item.key" type="button" class="flex w-full items-center gap-3 rounded-lg px-2.5 py-2.5 text-left transition-colors hover:bg-accent" :aria-label="`Switch to ${item.appleId}`" @click="emit('selectAccount', item.key)">
          <img v-if="avatars[item.key]" :src="avatars[item.key]" alt="" class="size-9 shrink-0 rounded-full object-cover" />
          <span v-else class="grid size-9 shrink-0 place-items-center rounded-full bg-secondary text-sm font-semibold text-muted-foreground">{{ displayName(item).charAt(0).toUpperCase() }}</span>
          <span class="min-w-0 flex-1">
            <span class="flex items-center gap-1.5">
              <span class="truncate text-sm font-medium">{{ displayName(item) }}</span>
              <span v-if="item.key === activeAccountKey" class="shrink-0 rounded bg-sidebar-accent px-1 py-px text-2xs text-muted-foreground">{{ locale.t("account.current") }}</span>
            </span>
            <span class="block truncate text-xs text-muted-foreground">{{ item.appleId }}</span>
          </span>
          <span class="shrink-0 text-xs" :class="statusClass(item)">{{ statusText(item) }}</span>
        </button>
      </div>
    </div>
  </div>
</template>
