<script setup lang="ts">
import { computed, nextTick, onMounted, ref, watch } from "vue";
import { Clock3, FileText, Folder, NotebookPen, Pencil, Plus, RefreshCw, Save, Search, StickyNote, Trash2, X } from "@lucide/vue";

import { Status } from "@/constants";
import { noteCreate, noteDelete, noteDetail, noteUpdate, notesList, openExternal } from "@/lib/api";
import { toast } from "@/lib/common/toast";
import { isSessionGoneMessage } from "@/lib/session-errors";
import { useLocaleStore } from "@/stores/locale";
import Spinner from "@/components/ui/Spinner.vue";
import type { NoteAttachment, NoteDetail, NoteFolder, NoteSummary } from "@/types";

const props = defineProps<{ sessionId: string }>();
const emit = defineEmits<{ logout: [] }>();
const locale = useLocaleStore();
const t = locale.t;

const folders = ref<NoteFolder[]>([]);
const notes = ref<NoteSummary[]>([]);
const nextCursor = ref<string | null>(null);
const selectedFolderId = ref<string | null>(null);
const selectedNoteId = ref<string | null>(null);
const detail = ref<NoteDetail | null>(null);
const query = ref("");
const loading = ref(false);
const loadingMore = ref(false);
const detailLoading = ref(false);
const error = ref<string | null>(null);
const detailError = ref<string | null>(null);
const activeAttachment = ref<NoteAttachment | null>(null);
const editing = ref(false);
const creating = ref(false);
const saving = ref(false);
const deleting = ref(false);
const draftTitle = ref("");
const draftSegments = ref<string[]>([""]);
let loadEpoch = 0;

const visibleNotes = computed(() => {
  const search = query.value.trim().toLocaleLowerCase(locale.locale);
  return notes.value.filter((note) => {
    if (selectedFolderId.value && note.folderId !== selectedFolderId.value) return false;
    if (!search) return true;
    return `${note.title}\n${note.snippet}`.toLocaleLowerCase(locale.locale).includes(search);
  });
});

const noteBody = computed(() => detail.value?.body.replaceAll("\uFFFC", "").trim() ?? "");
const draftBody = computed(() => draftSegments.value.join("\uFFFC"));
const draftInlineAttachments = computed<(NoteAttachment | null)[]>(() => {
  const markerCount = Math.max(0, draftSegments.value.length - 1);
  return Array.from({ length: markerCount }, (_, index) => detail.value?.attachments[index] ?? null);
});
const selectedFolderName = computed(() => {
  if (!selectedFolderId.value) return t("notes.allNotes");
  return folders.value.find((folder) => folder.id === selectedFolderId.value)?.name ?? t("notes.allNotes");
});

const folderCounts = computed(() => {
  const counts = new Map<string, number>();
  for (const note of notes.value) {
    if (!note.folderId) continue;
    counts.set(note.folderId, (counts.get(note.folderId) ?? 0) + 1);
  }
  return counts;
});

function noteDate(value: number | null): string {
  if (!value) return "";
  try {
    return new Date(value).toLocaleString(locale.locale, { year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" });
  } catch {
    return "";
  }
}

function attachmentUrl(attachment: NoteAttachment): string | null {
  if (attachment.renderUrl) return attachment.renderUrl;
  if (attachment.uti === "com.apple.paper") {
    return attachment.fallbackUrl || attachment.previewUrl || attachment.darkPreviewUrl || attachment.downloadUrl;
  }
  return attachment.downloadUrl || attachment.fallbackUrl || attachment.previewUrl || attachment.darkPreviewUrl;
}

function openAttachment(attachment: NoteAttachment): void {
  const url = attachmentUrl(attachment);
  if (!url) return;
  if (attachment.uti === "com.apple.paper") {
    activeAttachment.value = attachment;
    return;
  }
  void openExternal(url);
}

function isInlineImageAttachment(attachment: NoteAttachment): boolean {
  const uti = attachment.uti?.toLocaleLowerCase() ?? "";
  return uti === "com.apple.paper" || uti.startsWith("public.image") || [".png", ".jpeg", ".jpg", ".heic", ".gif", ".webp", ".tiff"].some((suffix) => uti.endsWith(suffix));
}

function draftAttachment(index: number): NoteAttachment | null {
  return draftInlineAttachments.value[index] ?? null;
}

function draftAttachmentName(index: number): string {
  return draftAttachment(index)?.name ?? "";
}

function draftAttachmentImageUrl(index: number): string | null {
  const attachment = draftAttachment(index);
  return attachment && isInlineImageAttachment(attachment) ? attachmentUrl(attachment) : null;
}

function openDraftAttachment(index: number): void {
  const attachment = draftAttachment(index);
  if (attachment) openAttachment(attachment);
}

function handleNoteContentClick(event: MouseEvent): void {
  const target = (event.target as HTMLElement | null)?.closest<HTMLElement>("[data-note-attachment-id]");
  const attachmentId = target?.dataset.noteAttachmentId;
  if (!attachmentId || !detail.value) return;
  const attachment = detail.value.attachments.find((candidate) => candidate.id === attachmentId);
  if (attachment) openAttachment(attachment);
}
function noteTitle(note: NoteSummary | NoteDetail): string {
  return note.title.trim() || t("notes.untitled");
}

function editableBody(note: NoteDetail): string {
  const lines = note.body.replaceAll("\r\n", "\n").replaceAll("\r", "\n").split("\n");
  while (lines.length > 0 && !lines[0]?.trim()) lines.shift();
  if (lines[0]?.trim() === note.title.trim()) lines.shift();
  return lines.join("\n").replace(/^\n+|\n+$/g, "");
}

function resizeDraftTextarea(element: HTMLTextAreaElement): void {
  element.style.height = "auto";
  element.style.height = `${Math.max(element.scrollHeight, 48)}px`;
}

function resizeDraftEditors(): void {
  document.querySelectorAll<HTMLTextAreaElement>("[data-note-draft-segment]").forEach(resizeDraftTextarea);
}

function draftRows(segment: string): number {
  return Math.min(12, Math.max(1, segment.split("\n").length));
}

function updateDraftSegment(index: number, event: Event): void {
  const target = event.currentTarget;
  if (!(target instanceof HTMLTextAreaElement)) return;
  draftSegments.value[index] = target.value;
  resizeDraftTextarea(target);
}

function startCreate(): void {
  editing.value = true;
  creating.value = true;
  draftTitle.value = "";
  draftSegments.value = [""];
  selectedNoteId.value = null;
  detail.value = null;
  detailError.value = null;
  activeAttachment.value = null;
  void nextTick(resizeDraftEditors);
}

function startEdit(): void {
  if (!detail.value) return;
  editing.value = true;
  creating.value = false;
  draftTitle.value = detail.value.title.trim();
  draftSegments.value = editableBody(detail.value).split("\uFFFC");
  void nextTick(resizeDraftEditors);
}

function cancelEdit(): void {
  editing.value = false;
  creating.value = false;
  draftTitle.value = "";
  draftSegments.value = [""];
}

async function saveDraft(): Promise<void> {
  if (saving.value) return;
  if (!draftTitle.value.trim() && !draftBody.value.trim()) {
    toast.error(t("notes.emptyDraft"));
    return;
  }
  saving.value = true;
  try {
    const title = draftTitle.value.trim() || t("notes.untitled");
    const result = creating.value ? await noteCreate({ sessionId: props.sessionId, folderId: selectedFolderId.value, title, body: draftBody.value }) : await noteUpdate({ sessionId: props.sessionId, noteId: selectedNoteId.value!, title, body: draftBody.value });
    if (result.status === Status.error) {
      if (isSessionGoneMessage(result.message)) {
        emit("logout");
        return;
      }
      throw new Error(result.message ?? (creating.value ? t("notes.createFailed") : t("notes.updateFailed")));
    }
    const noteId = result.noteId;
    editing.value = false;
    creating.value = false;
    await loadNotes();
    if (noteId) await loadNoteDetail(noteId);
    toast.success(t("notes.saved"));
  } catch (caught) {
    toast.error(caught instanceof Error ? caught.message : t("notes.updateFailed"));
  } finally {
    saving.value = false;
  }
}

async function deleteSelected(): Promise<void> {
  if (!detail.value || deleting.value) return;
  if (!window.confirm(t("notes.deleteConfirm", { title: noteTitle(detail.value) }))) return;
  deleting.value = true;
  try {
    const result = await noteDelete({ sessionId: props.sessionId, noteId: detail.value.id });
    if (result.status === Status.error) {
      if (isSessionGoneMessage(result.message)) {
        emit("logout");
        return;
      }
      throw new Error(result.message ?? t("notes.deleteFailed"));
    }
    cancelEdit();
    detail.value = null;
    selectedNoteId.value = null;
    await loadNotes();
    toast.success(t("notes.deleted"));
  } catch (caught) {
    toast.error(caught instanceof Error ? caught.message : t("notes.deleteFailed"));
  } finally {
    deleting.value = false;
  }
}

async function loadNotes(reset = true): Promise<void> {
  if (loading.value || loadingMore.value) return;
  if (reset) {
    loadEpoch += 1;
    loading.value = true;
    error.value = null;
    notes.value = [];
    folders.value = [];
    nextCursor.value = null;
    detail.value = null;
    selectedNoteId.value = null;
    detailError.value = null;
  } else {
    loadingMore.value = true;
  }
  const epoch = loadEpoch;
  try {
    const result = await notesList({ sessionId: props.sessionId, cursor: reset ? null : nextCursor.value });
    if (epoch !== loadEpoch) return;
    if (result.status === Status.error) {
      if (isSessionGoneMessage(result.message)) {
        emit("logout");
        return;
      }
      error.value = result.message ?? t("notes.loadFailed");
      return;
    }
    if (reset) folders.value = result.folders;
    const seen = new Set(notes.value.map((note) => note.id));
    notes.value.push(...result.notes.filter((note) => !seen.has(note.id)));
    nextCursor.value = result.nextCursor;
  } catch (caught) {
    if (epoch !== loadEpoch) return;
    error.value = caught instanceof Error ? caught.message : t("notes.loadFailed");
  } finally {
    if (epoch === loadEpoch) {
      loading.value = false;
      loadingMore.value = false;
    }
  }
}

async function selectNote(note: NoteSummary): Promise<void> {
  cancelEdit();
  await loadNoteDetail(note.id);
}

async function loadNoteDetail(noteId: string): Promise<void> {
  selectedNoteId.value = noteId;
  activeAttachment.value = null;
  detail.value = null;
  detailError.value = null;
  detailLoading.value = true;
  try {
    const result = await noteDetail({ sessionId: props.sessionId, noteId });
    if (selectedNoteId.value !== noteId) return;
    if (result.status === Status.error || !result.note) {
      if (isSessionGoneMessage(result.message)) {
        emit("logout");
        return;
      }
      detailError.value = result.message ?? t("notes.detailFailed");
      return;
    }
    detail.value = result.note;
  } catch (caught) {
    if (selectedNoteId.value === noteId) detailError.value = caught instanceof Error ? caught.message : t("notes.detailFailed");
  } finally {
    if (selectedNoteId.value === noteId) detailLoading.value = false;
  }
}
function selectFolder(id: string | null): void {
  selectedFolderId.value = id;
}

onMounted(() => void loadNotes());

watch(
  () => props.sessionId,
  () => {
    cancelEdit();
    void loadNotes();
  },
);
</script>

<template>
  <div class="flex h-full min-h-0 bg-background">
    <aside class="flex w-48 shrink-0 flex-col border-r border-border bg-card/35">
      <div class="flex h-11 shrink-0 items-center gap-2 border-b border-border px-3">
        <NotebookPen :size="15" class="text-primary" />
        <span class="text-sm font-semibold">{{ t("nav.notes") }}</span>
        <button type="button" class="ml-auto grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50" :disabled="loading || saving" :title="t('notes.new')" :aria-label="t('notes.new')" @click="startCreate">
          <Plus :size="14" />
        </button>
        <button type="button" class="grid size-7 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50" :disabled="loading || saving" :title="t('notes.refresh')" :aria-label="t('notes.refresh')" @click="loadNotes()">
          <RefreshCw :size="13" :class="loading ? 'animate-spin' : ''" />
        </button>
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto p-2">
        <button type="button" class="flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm transition-colors" :class="selectedFolderId === null ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70 hover:text-foreground'" @click="selectFolder(null)">
          <StickyNote :size="14" />
          <span class="min-w-0 flex-1 truncate">{{ t("notes.allNotes") }}</span>
          <span class="tabular text-2xs text-muted-foreground">{{ notes.length }}</span>
        </button>
        <button
          v-for="folder in folders"
          :key="folder.id"
          type="button"
          class="mt-0.5 flex h-8 w-full items-center gap-2 rounded-md px-2 text-left text-sm transition-colors"
          :class="selectedFolderId === folder.id ? 'bg-accent text-foreground' : 'text-muted-foreground hover:bg-accent/70 hover:text-foreground'"
          :title="folder.name"
          @click="selectFolder(folder.id)"
        >
          <Folder :size="14" class="shrink-0" />
          <span class="min-w-0 flex-1 truncate">{{ folder.name }}</span>
          <span v-if="folderCounts.get(folder.id)" class="tabular text-2xs text-muted-foreground">{{ folderCounts.get(folder.id) }}</span>
        </button>
      </div>
    </aside>

    <section class="flex w-[310px] shrink-0 flex-col border-r border-border">
      <div class="flex h-11 shrink-0 items-center border-b border-border px-3">
        <div class="relative min-w-0 flex-1">
          <Search :size="13" class="pointer-events-none absolute top-1/2 left-2 -translate-y-1/2 text-muted-foreground" />
          <input v-model="query" class="h-7 w-full rounded-md border border-border/70 bg-background pr-2 pl-7 text-xs outline-none transition-colors focus:border-primary" :placeholder="t('notes.searchPlaceholder')" :aria-label="t('notes.searchPlaceholder')" />
        </div>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto p-2">
        <div v-if="loading && notes.length === 0" class="grid h-32 place-items-center text-muted-foreground">
          <Spinner :size="18" />
        </div>
        <div v-else-if="error" class="m-2 rounded-lg border border-destructive/25 bg-destructive/5 p-3 text-xs text-destructive">
          <p>{{ error }}</p>
          <button type="button" class="mt-2 rounded border border-border px-2 py-1 text-muted-foreground hover:bg-accent" @click="loadNotes()">{{ t("common.retry") }}</button>
        </div>
        <div v-else-if="visibleNotes.length === 0" class="grid h-32 place-items-center px-4 text-center text-xs text-muted-foreground">
          {{ query ? t("notes.noMatches") : t("notes.noNotes") }}
        </div>
        <template v-else>
          <button
            v-for="note in visibleNotes"
            :key="note.id"
            type="button"
            class="mb-1 w-full rounded-lg border px-3 py-2.5 text-left transition-colors"
            :class="selectedNoteId === note.id ? 'border-primary/25 bg-primary/8' : 'border-transparent hover:border-border hover:bg-accent/55'"
            @click="selectNote(note)"
          >
            <div class="flex items-start gap-2">
              <FileText :size="14" class="mt-0.5 shrink-0 text-muted-foreground" />
              <div class="min-w-0 flex-1">
                <div class="flex items-center gap-1.5">
                  <p class="min-w-0 flex-1 truncate text-sm font-medium">{{ noteTitle(note) }}</p>
                  <span v-if="note.pinned" class="shrink-0 text-2xs text-primary">{{ t("notes.pinned") }}</span>
                </div>
                <p v-if="note.snippet" class="mt-1 line-clamp-2 text-xs leading-4 text-muted-foreground">{{ note.snippet }}</p>
                <p class="tabular mt-1.5 text-2xs text-muted-foreground">{{ noteDate(note.modifiedAt) }}</p>
              </div>
            </div>
          </button>
          <button v-if="nextCursor" type="button" class="mt-2 flex h-8 w-full items-center justify-center gap-2 rounded-md border border-border text-xs text-muted-foreground hover:bg-accent disabled:opacity-50" :disabled="loadingMore" @click="loadNotes(false)">
            <Spinner v-if="loadingMore" :size="12" />
            {{ loadingMore ? t("notes.loading") : t("notes.loadMore") }}
          </button>
        </template>
      </div>
    </section>

    <section class="min-w-0 flex-1 overflow-y-auto">
      <form v-if="editing" class="mx-auto flex h-full max-w-4xl flex-col px-10 py-8" @submit.prevent="saveDraft">
        <div class="flex items-center justify-between gap-3 border-b border-border pb-3">
          <p class="text-xs text-muted-foreground">{{ creating ? t("notes.new") : selectedFolderName }}</p>
          <span v-if="draftInlineAttachments.length === 0" class="text-xs text-muted-foreground">{{ t("notes.plainTextOnly") }}</span>
        </div>
        <input v-model="draftTitle" data-testid="note-title-input" class="mt-5 w-full border-0 bg-transparent text-2xl font-semibold tracking-tight outline-none placeholder:text-muted-foreground/55" :placeholder="t('notes.titlePlaceholder')" autofocus />
        <p v-if="draftInlineAttachments.length > 0" class="mt-3 rounded-md border border-border bg-muted/40 px-3 py-2 text-xs leading-5 text-muted-foreground">
          {{ t("notes.attachmentsReadOnly") }}
        </p>
        <div class="note-draft-body mt-4 flex min-h-[50vh] flex-1 flex-col gap-3 overflow-y-auto rounded-lg border border-border bg-card/35 p-4 transition-colors focus-within:border-primary">
          <template v-for="(segment, index) in draftSegments" :key="index">
            <textarea
              :value="segment"
              :rows="draftRows(segment)"
              data-testid="note-body-input"
              data-note-draft-segment
              :class="draftSegments.length === 1 ? 'min-h-[calc(50vh-3.5rem)] flex-1' : 'min-h-12'"
              class="w-full resize-none overflow-hidden border-0 bg-transparent p-0 text-[15px] leading-7 outline-none placeholder:text-muted-foreground/55"
              :placeholder="index === 0 ? t('notes.bodyPlaceholder') : ''"
              @input="updateDraftSegment(index, $event)"
            ></textarea>
            <div v-if="index < draftInlineAttachments.length" class="shrink-0">
              <button v-if="draftAttachment(index)" type="button" class="group relative block w-full overflow-hidden rounded-lg border border-border bg-background text-left transition-colors hover:border-primary/60" :title="draftAttachmentName(index)" @click="openDraftAttachment(index)">
                <img v-if="draftAttachmentImageUrl(index)" :src="draftAttachmentImageUrl(index) ?? undefined" :alt="draftAttachmentName(index)" class="max-h-80 w-full bg-white object-contain" loading="lazy" decoding="async" />
                <span v-else class="flex min-h-20 items-center justify-center gap-2 px-4 text-sm text-muted-foreground">
                  <FileText :size="18" />
                  {{ draftAttachmentName(index) }}
                </span>
              </button>
              <div v-else class="flex min-h-16 items-center justify-center rounded-lg border border-dashed border-border bg-background/60 px-4 text-center text-xs text-muted-foreground">
                {{ t("notes.attachmentsReadOnly") }}
              </div>
            </div>
          </template>
        </div>
        <div class="mt-4 flex items-center gap-2">
          <button type="submit" class="inline-flex h-8 items-center gap-1.5 rounded-md bg-primary px-3 text-xs font-medium text-primary-foreground transition-opacity hover:opacity-90 disabled:opacity-50" :disabled="saving">
            <Spinner v-if="saving" :size="12" />
            <Save v-else :size="13" />
            {{ saving ? t("notes.saving") : t("notes.save") }}
          </button>
          <button type="button" class="h-8 rounded-md border border-border px-3 text-xs text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50" :disabled="saving" @click="cancelEdit">
            {{ t("notes.cancel") }}
          </button>
        </div>
      </form>
      <div v-else-if="detailLoading" class="grid h-full place-items-center text-muted-foreground"><Spinner :size="22" /></div>
      <div v-else-if="detailError" class="grid h-full place-items-center px-6 text-center text-sm text-destructive">{{ detailError }}</div>
      <article v-else-if="detail" class="mx-auto max-w-4xl px-10 py-8">
        <header class="border-b border-border pb-5">
          <div class="flex items-start justify-between gap-4">
            <div class="min-w-0">
              <p class="text-xs text-muted-foreground">{{ selectedFolderName }}</p>
              <h1 class="mt-1.5 text-2xl font-semibold tracking-tight">{{ noteTitle(detail) }}</h1>
            </div>
            <div class="flex shrink-0 items-center gap-1">
              <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-accent hover:text-foreground disabled:opacity-50" :disabled="deleting" :title="t('notes.edit')" :aria-label="t('notes.edit')" @click="startEdit">
                <Pencil :size="15" />
              </button>
              <button type="button" class="grid size-8 place-items-center rounded-md text-muted-foreground transition-colors hover:bg-destructive/10 hover:text-destructive disabled:opacity-50" :disabled="deleting" :title="t('notes.delete')" :aria-label="t('notes.delete')" @click="deleteSelected">
                <Spinner v-if="deleting" :size="14" />
                <Trash2 v-else :size="15" />
              </button>
            </div>
          </div>
          <div class="mt-3 flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
            <span class="flex items-center gap-1.5"><Clock3 :size="12" />{{ t("notes.modifiedAt") }} {{ noteDate(detail.modifiedAt) }}</span>
          </div>
        </header>
        <div v-if="detail.bodyHtml" class="note-content mt-6" v-html="detail.bodyHtml" @click="handleNoteContentClick"></div>
        <pre v-else-if="noteBody" class="mt-6 whitespace-pre-wrap break-words font-sans text-[15px] leading-7 text-foreground">{{ noteBody }}</pre>
        <p v-else-if="detail.attachments.length === 0" class="mt-6 text-sm text-muted-foreground">{{ t("notes.emptyBody") }}</p>
      </article>
      <div v-else class="grid h-full place-items-center px-6 text-center text-muted-foreground">
        <div>
          <NotebookPen :size="28" class="mx-auto mb-3 opacity-45" />
          <p class="text-sm">{{ t("notes.selectNote") }}</p>
        </div>
      </div>
    </section>
    <div v-if="activeAttachment" class="fixed inset-0 z-50 grid place-items-center bg-black/80 p-6" role="dialog" aria-modal="true" @click.self="activeAttachment = null">
      <div class="flex max-h-full max-w-[min(96vw,1200px)] flex-col overflow-hidden rounded-xl bg-card shadow-2xl">
        <div class="flex items-center gap-2.5 border-b border-border px-4 py-3">
          <span class="min-w-0 flex-1 truncate text-sm font-medium">{{ activeAttachment.name }}</span>
          <span v-if="activeAttachment.width && activeAttachment.height" class="tabular shrink-0 text-2xs text-muted-foreground">{{ Math.round(activeAttachment.width) }} × {{ Math.round(activeAttachment.height) }}</span>
          <button type="button" class="grid size-7 place-items-center rounded-md text-muted-foreground hover:bg-accent hover:text-foreground" title="Close" aria-label="Close" @click="activeAttachment = null">
            <X :size="15" />
          </button>
        </div>
        <img :src="attachmentUrl(activeAttachment) ?? undefined" :alt="activeAttachment.name" class="max-h-[82vh] w-auto max-w-full bg-white object-contain" decoding="async" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.note-content {
  color: var(--foreground);
  font-size: 15px;
  line-height: 1.75;
  word-break: break-word;
}

.note-content :deep(.note-content-inner) {
  min-width: 0;
}

.note-content :deep(.note-title) {
  margin: 1.1rem 0 1rem;
  font-size: 2rem;
  font-weight: 700;
  line-height: 1.15;
  letter-spacing: -0.025em;
}

.note-content :deep(.note-heading) {
  margin: 1.1rem 0 0.65rem;
  font-size: 1.5rem;
  font-weight: 650;
  line-height: 1.25;
}

.note-content :deep(.note-subheading) {
  margin: 1rem 0 0.55rem;
  font-size: 1.2rem;
  font-weight: 600;
  line-height: 1.35;
}

.note-content :deep(.note-paragraph) {
  margin: 0.55rem 0;
  white-space: pre-wrap;
}

.note-content :deep(.note-monospaced) {
  margin: 0.75rem 0;
  overflow-x: auto;
  border-radius: 0.5rem;
  background: var(--muted);
  padding: 0.75rem 0.9rem;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.9rem;
}

.note-content :deep(.note-empty) {
  height: 0.85rem;
}

.note-content :deep(.note-list-item) {
  display: flex;
  align-items: flex-start;
  gap: 0.55rem;
  margin: 0.4rem 0;
  padding-left: calc(var(--note-indent, 0) * 1.35rem);
}

.note-content :deep(.note-list-marker) {
  flex: 0 0 1.1rem;
  text-align: center;
  line-height: 1.75;
}

.note-content :deep(.note-list-number) {
  min-width: 1.2rem;
  text-align: right;
}

.note-content :deep(.note-list-text) {
  min-width: 0;
  flex: 1;
  white-space: pre-wrap;
}

.note-content :deep(.note-checkbox) {
  display: inline-block;
  flex: 0 0 1rem;
  width: 1rem;
  height: 1rem;
  margin-top: 0.28rem;
  border: 1.5px solid color-mix(in srgb, var(--muted-foreground) 62%, transparent);
  border-radius: 999px;
}

.note-content :deep(.note-checkbox.checked) {
  border-color: var(--primary);
  background: var(--primary);
  box-shadow: inset 0 0 0 3px var(--background);
}

.note-content :deep(.note-quote) {
  margin: 0.75rem 0;
  border-left: 3px solid var(--border);
  padding-left: 0.9rem;
  color: var(--muted-foreground);
}

.note-content :deep(.note-embedded-image) {
  margin: 1.15rem 0;
  cursor: zoom-in;
}

.note-content :deep(.note-embedded-image img) {
  display: block;
  max-width: 100%;
  height: auto;
  border-radius: 4px;
}

.note-content :deep(.note-embedded-paper img) {
  background: #fff;
}

.note-content :deep(.note-file-embed) {
  display: flex;
  align-items: center;
  gap: 0.65rem;
  margin: 0.8rem 0;
  border: 1px solid var(--border);
  border-radius: 0.65rem;
  padding: 0.7rem 0.85rem;
}

.note-content :deep(.note-file-embed-name) {
  min-width: 0;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.note-content :deep(.note-file-embed-type) {
  flex: 0 0 auto;
  color: var(--muted-foreground);
  font-size: 0.75rem;
}
.note-content :deep(.note-table-wrap) {
  margin: 0.8rem 0 1.4rem;
  overflow-x: auto;
}

.note-content :deep(.note-table) {
  border-collapse: collapse;
  table-layout: fixed;
  color: var(--foreground);
  font-size: 0.98rem;
}

.note-content :deep(.note-table td) {
  min-width: 13rem;
  border: 1px solid color-mix(in srgb, var(--border) 88%, transparent);
  padding: 0.45rem 0.6rem;
  text-align: left;
  vertical-align: top;
}

.note-content :deep(a) {
  color: var(--primary);
  text-decoration: underline;
  text-underline-offset: 2px;
}
</style>
