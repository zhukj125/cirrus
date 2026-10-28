<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { Building2, Calendar, Copy, FileText, Globe, Mail, MapPin, Phone, RefreshCw, Search, User, Users } from "@lucide/vue";

import { Status } from "@/constants";
import { contactPhoto, contactsList, openExternal } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { isSessionGoneMessage } from "@/lib/session-errors";
import { useLocaleStore } from "@/stores/locale";
import Spinner from "@/components/ui/Spinner.vue";
import type { ContactAddress, ContactEntry } from "@/types";

const ALPHABET = Array.from("ABCDEFGHIJKLMNOPQRSTUVWXYZ");
const SECTION_ORDER = ["#", ...ALPHABET];
const PINYIN_BOUNDARIES: Array<[string, string]> = [
  ["A", "阿"],
  ["B", "八"],
  ["C", "擦"],
  ["D", "搭"],
  ["E", "婀"],
  ["F", "发"],
  ["G", "噶"],
  ["H", "哈"],
  ["J", "击"],
  ["K", "喀"],
  ["L", "拉"],
  ["M", "妈"],
  ["N", "拿"],
  ["O", "哦"],
  ["P", "趴"],
  ["Q", "七"],
  ["R", "然"],
  ["S", "撒"],
  ["T", "塌"],
  ["W", "挖"],
  ["X", "夕"],
  ["Y", "压"],
  ["Z", "匝"],
];
const contactCollator = new Intl.Collator("zh-CN", { sensitivity: "base", numeric: true });

const props = defineProps<{ sessionId: string }>();
const emit = defineEmits<{ logout: [] }>();
const locale = useLocaleStore();
const t = locale.t;

const contacts = ref<ContactEntry[]>([]);
const selectedId = ref<string | null>(null);
const query = ref("");
const loading = ref(false);
const error = ref<string | null>(null);
const photoDataUrls = ref(new Map<string, string>());
const photoRequests = new Map<string, Promise<void>>();

const filteredContacts = computed(() => {
  const needle = query.value.trim().toLocaleLowerCase(locale.locale);
  const matching = !needle ? contacts.value : contacts.value.filter((contact) => contactSearchText(contact).includes(needle));
  return [...matching].sort((left, right) => contactCollator.compare(left.displayName, right.displayName));
});

const sections = computed(() => {
  const grouped = new Map<string, ContactEntry[]>();
  for (const contact of filteredContacts.value) {
    const letter = contactSection(contact.displayName);
    const list = grouped.get(letter) ?? [];
    list.push(contact);
    grouped.set(letter, list);
  }
  return SECTION_ORDER.filter((letter) => grouped.has(letter)).map((letter) => ({ letter, contacts: grouped.get(letter) ?? [] }));
});

const availableLetters = computed(() => new Set(sections.value.map((section) => section.letter)));
const selectedContact = computed(() => filteredContacts.value.find((contact) => contact.id === selectedId.value) ?? null);
const selectedPhotoUrl = computed(() => {
  const contact = selectedContact.value;
  return contact ? (photoDataUrls.value.get(contact.id) ?? null) : null;
});

function contactSearchText(contact: ContactEntry): string {
  return [
    contact.displayName,
    contact.firstName,
    contact.lastName,
    contact.companyName,
    contact.department,
    contact.jobTitle,
    ...contact.phones.map((phone) => phone.value),
    ...contact.emails.map((email) => email.value),
    ...contact.addresses.flatMap((address) => [address.street, address.city, address.region, address.postalCode, address.country]),
  ]
    .join("\n")
    .toLocaleLowerCase(locale.locale);
}

function contactSection(name: string): string {
  const first = Array.from(name.trim())[0] ?? "#";
  if (/[a-z]/i.test(first)) return first.toUpperCase();
  let section = "#";
  for (const [letter, boundary] of PINYIN_BOUNDARIES) {
    if (contactCollator.compare(name, boundary) < 0) break;
    section = letter;
  }
  return section;
}

function contactInitials(contact: ContactEntry): string {
  const name = contact.displayName.trim();
  return name ? Array.from(name).slice(0, 2).join("") : "?";
}

function fieldLabel(label: string): string {
  const normalized = label.trim().toUpperCase();
  if (normalized === "MOBILE" || normalized === "CELL" || normalized === "IPHONE") return t("contacts.labelMobile");
  if (normalized === "HOME") return t("contacts.labelHome");
  if (normalized === "WORK") return t("contacts.labelWork");
  if (normalized === "MAIN" || normalized === "PREF") return t("contacts.labelMain");
  if (normalized === "HOMEPAGE" || normalized === "URL") return t("contacts.labelHomepage");
  if (!normalized || normalized === "OTHER") return t("contacts.labelOther");
  return label;
}

function addressLines(address: ContactAddress): string[] {
  return [address.street, [address.city, address.region, address.postalCode].filter(Boolean).join(" "), address.country].filter(Boolean);
}

async function loadContactPhoto(contact: ContactEntry): Promise<void> {
  if (!contact.photoUrl || photoDataUrls.value.has(contact.id) || photoRequests.has(contact.id)) return;
  const task = contactPhoto({ sessionId: props.sessionId, url: contact.photoUrl })
    .then((result) => {
      if (!result.dataUrl) return;
      const next = new Map(photoDataUrls.value);
      next.set(contact.id, result.dataUrl);
      photoDataUrls.value = next;
    })
    .catch(() => undefined)
    .finally(() => photoRequests.delete(contact.id));
  photoRequests.set(contact.id, task);
  await task;
}
async function copyValue(value: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(value);
    toast.success(t("contacts.copied"));
  } catch {
    toast.error(t("contacts.copyFailed"));
  }
}

function openWebsite(url: string): void {
  const target = /^https?:\/\//i.test(url) ? url : `https://${url}`;
  void openExternal(target).catch(() => toast.error(t("contacts.copyFailed")));
}

function scrollToSection(letter: string): void {
  if (!availableLetters.value.has(letter)) return;
  document.getElementById(`contact-section-${letter}`)?.scrollIntoView({ block: "start", behavior: "smooth" });
}

async function loadContacts(): Promise<void> {
  if (loading.value) return;
  loading.value = true;
  error.value = null;
  try {
    const response = await contactsList({
      sessionId: props.sessionId,
      locale: locale.locale === "zh-CN" ? "zh_CN" : "en_US",
    });
    if (response.status === Status.error) {
      if (isSessionGoneMessage(response.message)) {
        emit("logout");
        return;
      }
      error.value = response.message ?? t("contacts.loadFailed");
      return;
    }
    contacts.value = response.contacts;
    if (!selectedId.value || !contacts.value.some((contact) => contact.id === selectedId.value)) {
      selectedId.value = filteredContacts.value[0]?.id ?? null;
    }
  } catch (caught) {
    error.value = caught instanceof Error ? caught.message : t("contacts.loadFailed");
  } finally {
    loading.value = false;
  }
}

watch(
  selectedContact,
  (contact) => {
    if (contact) void loadContactPhoto(contact);
  },
  { immediate: true },
);
watch(
  filteredContacts,
  (next) => {
    if (!next.length) {
      selectedId.value = null;
      return;
    }
    if (!next.some((contact) => contact.id === selectedId.value)) selectedId.value = next[0]?.id ?? null;
  },
  { flush: "post" },
);

watch(
  () => props.sessionId,
  () => void loadContacts(),
);

onMounted(() => void loadContacts());
</script>

<template>
  <div class="flex h-full min-h-0 bg-background">
    <aside class="flex w-[380px] shrink-0 flex-col border-r border-border bg-background xl:w-[430px]">
      <div class="flex h-12 shrink-0 items-center justify-end gap-2 px-4">
        <span v-if="contacts.length" class="mr-auto text-xs text-muted-foreground">{{ t("contacts.count", { count: contacts.length }) }}</span>
        <button type="button" class="grid size-8 place-items-center rounded-full text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50" :disabled="loading" :title="t('contacts.refresh')" :aria-label="t('contacts.refresh')" @click="loadContacts">
          <RefreshCw :size="15" :class="loading ? 'animate-spin' : ''" />
        </button>
        <span class="grid size-8 place-items-center rounded-full text-muted-foreground"><Users :size="16" /></span>
      </div>

      <div class="px-3 pb-3">
        <label class="flex h-11 items-center gap-2 rounded-xl bg-muted px-3.5 text-muted-foreground focus-within:ring-2 focus-within:ring-ring">
          <Search :size="16" class="shrink-0" />
          <input v-model="query" type="search" class="min-w-0 flex-1 bg-transparent text-[15px] text-foreground outline-none placeholder:text-muted-foreground" :placeholder="t('contacts.searchAll')" />
        </label>
      </div>

      <div class="relative min-h-0 flex-1 border-t border-border">
        <div class="h-full overflow-y-auto px-3 pb-8">
          <div v-if="loading && contacts.length === 0" class="grid h-44 place-items-center text-muted-foreground"><Spinner :size="20" /></div>
          <div v-else-if="error" class="px-4 py-10 text-center text-sm text-destructive">{{ error }}</div>
          <p v-else-if="filteredContacts.length === 0" class="px-4 py-10 text-center text-sm text-muted-foreground">{{ query ? t("contacts.noMatches") : t("contacts.noContacts") }}</p>
          <template v-for="section in sections" v-else :key="section.letter">
            <h2 :id="`contact-section-${section.letter}`" class="px-2 pt-5 pb-1.5 text-xs font-medium text-muted-foreground">{{ section.letter }}</h2>
            <button
              v-for="contact in section.contacts"
              :key="contact.id"
              type="button"
              class="mb-0.5 flex h-10 w-full items-center rounded-lg px-3 text-left text-[16px] font-semibold transition-colors"
              :class="contact.id === selectedId ? 'bg-primary text-primary-foreground shadow-sm' : 'text-foreground hover:bg-accent'"
              @click="selectedId = contact.id"
            >
              <span class="truncate">{{ contact.displayName }}</span>
            </button>
          </template>
        </div>

        <div class="pointer-events-none absolute top-1/2 right-1 flex -translate-y-1/2 flex-col items-center gap-[2px]">
          <button
            v-for="letter in SECTION_ORDER"
            :key="letter"
            type="button"
            class="pointer-events-auto h-3.5 min-w-3 text-center text-[10px] leading-none"
            :class="availableLetters.has(letter) ? 'font-semibold text-primary' : 'text-muted-foreground/25'"
            :disabled="!availableLetters.has(letter)"
            :aria-label="letter"
            @click="scrollToSection(letter)"
          >
            {{ availableLetters.has(letter) ? letter : "•" }}
          </button>
        </div>
      </div>
    </aside>

    <section class="min-w-0 flex-1 overflow-y-auto bg-background">
      <div v-if="loading && !selectedContact" class="grid h-full place-items-center text-muted-foreground"><Spinner :size="22" /></div>
      <article v-else-if="selectedContact" class="mx-auto max-w-4xl px-12 py-8">
        <header class="flex items-center gap-8 pb-10">
          <img v-if="selectedPhotoUrl" :src="selectedPhotoUrl" alt="" class="size-36 shrink-0 rounded-full bg-muted object-cover ring-1 ring-border" />
          <span v-else class="grid size-36 shrink-0 place-items-center rounded-full bg-muted text-4xl font-medium text-muted-foreground ring-1 ring-border">{{ contactInitials(selectedContact) }}</span>
          <div class="min-w-0 flex-1">
            <h1 class="break-words text-[28px] font-medium tracking-tight">{{ selectedContact.displayName }}</h1>
            <p v-if="selectedContact.nickname" class="mt-1 text-base text-muted-foreground">{{ selectedContact.nickname }}</p>
            <p v-if="selectedContact.companyName" class="mt-2 text-base text-muted-foreground">{{ selectedContact.companyName }}</p>
          </div>
        </header>

        <div class="space-y-7">
          <section v-if="selectedContact.phones.length" class="flex items-start gap-6">
            <span class="grid size-12 shrink-0 place-items-center rounded-full bg-blue-500 text-white shadow-sm"><Phone :size="23" fill="currentColor" /></span>
            <div class="min-w-0 flex-1 space-y-4">
              <div v-for="phoneValue in selectedContact.phones" :key="`${phoneValue.label}:${phoneValue.value}`" class="group flex items-start gap-3">
                <div class="min-w-0 flex-1">
                  <p class="text-base font-semibold">{{ fieldLabel(phoneValue.label) }}</p>
                  <p class="mt-0.5 break-all text-xl leading-7">{{ phoneValue.value }}</p>
                </div>
                <button
                  type="button"
                  class="mt-1 grid size-8 place-items-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 hover:bg-accent hover:text-foreground focus:opacity-100"
                  :title="t('contacts.copy')"
                  :aria-label="t('contacts.copy')"
                  @click="copyValue(phoneValue.value)"
                >
                  <Copy :size="14" />
                </button>
              </div>
            </div>
          </section>

          <section v-if="selectedContact.emails.length" class="flex items-start gap-6">
            <span class="grid size-12 shrink-0 place-items-center rounded-full bg-emerald-500 text-white shadow-sm"><Mail :size="22" /></span>
            <div class="min-w-0 flex-1 space-y-4">
              <div v-for="email in selectedContact.emails" :key="`${email.label}:${email.value}`" class="group flex items-start gap-3">
                <div class="min-w-0 flex-1">
                  <p class="text-base font-semibold">{{ fieldLabel(email.label) }}</p>
                  <p class="mt-0.5 break-all text-lg leading-7">{{ email.value }}</p>
                </div>
                <button
                  type="button"
                  class="mt-1 grid size-8 place-items-center rounded-md text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 hover:bg-accent hover:text-foreground focus:opacity-100"
                  :title="t('contacts.copy')"
                  :aria-label="t('contacts.copy')"
                  @click="copyValue(email.value)"
                >
                  <Copy :size="14" />
                </button>
              </div>
            </div>
          </section>

          <section v-if="selectedContact.addresses.length" class="flex items-start gap-6">
            <span class="grid size-12 shrink-0 place-items-center rounded-full bg-amber-500 text-white shadow-sm"><MapPin :size="22" /></span>
            <div class="min-w-0 flex-1 space-y-4">
              <div v-for="address in selectedContact.addresses" :key="`${address.label}:${address.street}:${address.city}`">
                <p class="text-base font-semibold">{{ fieldLabel(address.label) }}</p>
                <p v-for="line in addressLines(address)" :key="line" class="mt-0.5 text-lg leading-7">{{ line }}</p>
              </div>
            </div>
          </section>

          <section v-if="selectedContact.companyName || selectedContact.department || selectedContact.jobTitle" class="flex items-start gap-6">
            <span class="grid size-12 shrink-0 place-items-center rounded-full bg-slate-700 text-white shadow-sm"><Building2 :size="22" /></span>
            <div class="min-w-0 flex-1 space-y-2 text-base">
              <p v-if="selectedContact.companyName">
                <span class="mr-2 font-semibold">{{ t("contacts.company") }}</span
                >{{ selectedContact.companyName }}
              </p>
              <p v-if="selectedContact.department">
                <span class="mr-2 font-semibold">{{ t("contacts.department") }}</span
                >{{ selectedContact.department }}
              </p>
              <p v-if="selectedContact.jobTitle">
                <span class="mr-2 font-semibold">{{ t("contacts.jobTitle") }}</span
                >{{ selectedContact.jobTitle }}
              </p>
            </div>
          </section>

          <section v-if="selectedContact.urls.length" class="flex items-start gap-6">
            <span class="grid size-12 shrink-0 place-items-center rounded-full bg-sky-500 text-white shadow-sm"><Globe :size="22" /></span>
            <div class="min-w-0 flex-1 space-y-3">
              <div v-for="website in selectedContact.urls" :key="`${website.label}:${website.value}`" class="flex items-center gap-3">
                <button type="button" class="min-w-0 flex-1 truncate text-left text-lg text-primary hover:underline" @click="openWebsite(website.value)">{{ website.value }}</button>
                <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" :title="t('contacts.copy')" :aria-label="t('contacts.copy')" @click="copyValue(website.value)"><Copy :size="14" /></button>
              </div>
            </div>
          </section>

          <section v-if="selectedContact.relatedNames.length" class="flex items-start gap-6">
            <span class="grid size-12 shrink-0 place-items-center rounded-full bg-violet-500 text-white shadow-sm"><User :size="22" /></span>
            <div class="min-w-0 flex-1 space-y-3 text-base">
              <p v-for="related in selectedContact.relatedNames" :key="`${related.label}:${related.value}`">
                <span class="mr-2 font-semibold">{{ fieldLabel(related.label) }}</span
                >{{ related.value }}
              </p>
            </div>
          </section>

          <section v-if="selectedContact.birthday" class="flex items-start gap-6">
            <span class="grid size-12 shrink-0 place-items-center rounded-full bg-pink-500 text-white shadow-sm"><Calendar :size="22" /></span>
            <div>
              <p class="text-base font-semibold">{{ t("contacts.birthday") }}</p>
              <p class="mt-0.5 text-lg">{{ selectedContact.birthday }}</p>
            </div>
          </section>

          <section v-if="selectedContact.note" class="flex items-start gap-6">
            <span class="grid size-12 shrink-0 place-items-center rounded-full bg-orange-500 text-white shadow-sm"><FileText :size="22" /></span>
            <div class="min-w-0 flex-1">
              <p class="text-base font-semibold">{{ t("contacts.note") }}</p>
              <p class="mt-1 whitespace-pre-wrap text-base leading-7">{{ selectedContact.note }}</p>
            </div>
          </section>
        </div>
      </article>
      <div v-else class="grid h-full place-items-center px-6 text-center text-muted-foreground">
        <div>
          <Users :size="30" class="mx-auto mb-3 opacity-35" />
          <p class="text-sm">{{ error || t("contacts.selectContact") }}</p>
        </div>
      </div>
    </section>
  </div>
</template>
