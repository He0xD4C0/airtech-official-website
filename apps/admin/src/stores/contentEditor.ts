import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { ApiError } from '@airtek/contracts'
import type {
  CmsPrivateDraft,
  CmsSubmitResult,
  ContentDraftV2,
  ContentTemplateDefinition,
} from '@airtek/contracts'
import { contentApi } from '@/services/contentApi'
import { canonicalPathForDraft } from '@/services/canonicalPath'

export const EDITOR_HISTORY_LIMIT = 100

export type EditorLoadState = 'loading' | 'ready' | 'empty' | 'error' | 'forbidden'
export type EditorSaveState = 'idle' | 'dirty' | 'saving' | 'saved' | 'conflict' | 'error'

let templateCache: ContentTemplateDefinition[] | null = null

function cloneDraft(draft: ContentDraftV2): ContentDraftV2 {
  return JSON.parse(JSON.stringify(draft))
}

function fingerprint(draft: ContentDraftV2 | null): string {
  return draft ? JSON.stringify(draft) : ''
}

function errorStatus(error: unknown): number | undefined {
  return error instanceof ApiError ? error.status : undefined
}

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof ApiError) {
    const fields = error.errors
      ? Object.entries(error.errors)
          .map(([path, messages]) => `${path}: ${messages.join('; ')}`)
          .join(' | ')
      : ''
    return [error.detail || error.title || fallback, fields].filter(Boolean).join(' — ')
  }
  return error instanceof Error && error.message ? error.message : fallback
}

export const useContentEditorStore = defineStore('contentEditor', () => {
  const record = ref<CmsPrivateDraft | null>(null)
  const draft = ref<ContentDraftV2 | null>(null)
  const template = ref<ContentTemplateDefinition | null>(null)
  const loadState = ref<EditorLoadState>('loading')
  const saveState = ref<EditorSaveState>('idle')
  const loadError = ref('')
  const saveError = ref('')
  const lastSavedAt = ref<string | null>(null)
  const undoDepth = ref(0)
  const redoDepth = ref(0)

  let savedFingerprint = ''
  let undoStack: ContentDraftV2[] = []
  let redoStack: ContentDraftV2[] = []
  let lastHistoryKey: string | null = null

  const isDirty = computed(() => fingerprint(draft.value) !== savedFingerprint)
  const canUndo = computed(() => undoDepth.value > 0)
  const canRedo = computed(() => redoDepth.value > 0)
  const canonicalPath = computed(() => (
    draft.value ? canonicalPathForDraft(draft.value, template.value) : null
  ))
  const saveLabel = computed(() => {
    if (saveState.value === 'saving') return '正在保存…'
    if (saveState.value === 'conflict') return '版本冲突'
    if (saveState.value === 'error') return '保存失败'
    if (isDirty.value) return '有未保存更改'
    if (lastSavedAt.value) return `已保存 ${lastSavedAt.value}`
    return '已与数据库同步'
  })

  async function templates(): Promise<ContentTemplateDefinition[]> {
    templateCache ??= await contentApi.listTemplates()
    return templateCache
  }

  async function resolveTemplate(key: ContentDraftV2['templateKey']): Promise<void> {
    template.value = (await templates()).find((entry) => entry.key === key) ?? null
  }

  function resetHistory(): void {
    undoStack = []
    redoStack = []
    undoDepth.value = 0
    redoDepth.value = 0
    lastHistoryKey = null
  }

  function acceptRecord(next: CmsPrivateDraft): void {
    record.value = next
    draft.value = cloneDraft(next.document)
    savedFingerprint = fingerprint(draft.value)
    saveState.value = 'saved'
    saveError.value = ''
  }

  async function load(draftId: string): Promise<void> {
    loadState.value = 'loading'
    loadError.value = ''
    try {
      const loaded = (await contentApi.getDraft(draftId)).draft
      acceptRecord(loaded)
      await resolveTemplate(loaded.document.templateKey)
      resetHistory()
      loadState.value = 'ready'
    } catch (error) {
      const status = errorStatus(error)
      loadState.value = status === 403 ? 'forbidden' : status === 404 ? 'empty' : 'error'
      loadError.value = errorMessage(error, '私人草稿加载失败。')
    }
  }

  async function create(document: ContentDraftV2): Promise<string> {
    const created = (await contentApi.createDraft(document)).draft
    acceptRecord(created)
    await resolveTemplate(created.document.templateKey)
    resetHistory()
    loadState.value = 'ready'
    lastSavedAt.value = new Date().toLocaleTimeString('zh-CN')
    return created.draftId
  }

  function patch(
    mutator: (value: ContentDraftV2) => ContentDraftV2 | void,
    options: { historyKey?: string } = {},
  ): void {
    if (!draft.value || record.value?.state !== 'editing') return
    const before = cloneDraft(draft.value)
    const next = cloneDraft(draft.value)
    draft.value = mutator(next) ?? next
    if (fingerprint(before) === fingerprint(draft.value)) return
    const historyKey = options.historyKey ?? 'document'
    if (lastHistoryKey !== historyKey) {
      undoStack.push(before)
      if (undoStack.length > EDITOR_HISTORY_LIMIT) undoStack.shift()
    }
    lastHistoryKey = historyKey
    redoStack = []
    undoDepth.value = undoStack.length
    redoDepth.value = 0
    if (saveState.value !== 'conflict') {
      saveState.value = 'dirty'
      saveError.value = ''
    }
  }

  function undo(): void {
    if (!draft.value) return
    const previous = undoStack.pop()
    if (!previous) return
    redoStack.push(cloneDraft(draft.value))
    draft.value = previous
    lastHistoryKey = null
    undoDepth.value = undoStack.length
    redoDepth.value = redoStack.length
    if (saveState.value !== 'conflict') saveState.value = isDirty.value ? 'dirty' : 'saved'
  }

  function redo(): void {
    if (!draft.value) return
    const next = redoStack.pop()
    if (!next) return
    undoStack.push(cloneDraft(draft.value))
    draft.value = next
    lastHistoryKey = null
    undoDepth.value = undoStack.length
    redoDepth.value = redoStack.length
    if (saveState.value !== 'conflict') saveState.value = isDirty.value ? 'dirty' : 'saved'
  }

  async function save(): Promise<boolean> {
    if (!draft.value || !record.value || record.value.state !== 'editing') return false
    if (!isDirty.value) return true
    saveState.value = 'saving'
    saveError.value = ''
    const sent = cloneDraft(draft.value)
    try {
      const saved = await contentApi.saveDraft(record.value.draftId, sent)
      record.value = saved.draft
      draft.value = cloneDraft(saved.draft.document)
      savedFingerprint = fingerprint(draft.value)
      lastHistoryKey = null
      lastSavedAt.value = new Date().toLocaleTimeString('zh-CN')
      saveState.value = 'saved'
      return true
    } catch (error) {
      saveState.value = errorStatus(error) === 409 ? 'conflict' : 'error'
      saveError.value = errorMessage(error, '草稿保存失败。')
      return false
    }
  }

  async function submit(): Promise<CmsSubmitResult | null> {
    if (!record.value || !draft.value || isDirty.value) return null
    const result = await contentApi.submit(record.value.draftId, record.value.draftVersion)
    if (result.draft) acceptRecord(result.draft)
    if (result.publication) {
      record.value = null
      draft.value = null
      savedFingerprint = ''
      resetHistory()
    }
    return result
  }

  async function reloadServerVersion(): Promise<boolean> {
    if (!record.value) return false
    await load(record.value.draftId)
    return loadState.value === 'ready'
  }

  function reset(): void {
    record.value = null
    draft.value = null
    template.value = null
    loadState.value = 'loading'
    saveState.value = 'idle'
    loadError.value = ''
    saveError.value = ''
    lastSavedAt.value = null
    savedFingerprint = ''
    resetHistory()
  }

  return {
    record,
    draft,
    template,
    loadState,
    saveState,
    loadError,
    saveError,
    lastSavedAt,
    undoDepth,
    redoDepth,
    isDirty,
    canUndo,
    canRedo,
    canonicalPath,
    saveLabel,
    load,
    create,
    patch,
    undo,
    redo,
    save,
    submit,
    reloadServerVersion,
    reset,
  }
})
