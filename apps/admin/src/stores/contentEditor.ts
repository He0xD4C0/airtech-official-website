import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import { ApiError } from '@airtek/contracts'
import type {
  ContentDiffV2,
  ContentDraftV2,
  ContentRecordV2,
  ContentRevisionV2,
  ContentTemplateDefinition,
} from '@airtek/contracts'
import { contentApi } from '@/services/contentApi'
import { canonicalPathForDraft } from '@/services/canonicalPath'
import { diffJsonRevisions, type JsonRevisionDifference } from '@/services/jsonRevisionDiff'

export const AUTOSAVE_DEBOUNCE_MS = 1200

export type EditorLoadState = 'loading' | 'ready' | 'empty' | 'error' | 'forbidden'
export type EditorSaveState = 'idle' | 'dirty' | 'saving' | 'saved' | 'conflict' | 'error'

export interface EditorConflict {
  localDraft: ContentDraftV2
  serverRecord: ContentRecordV2 | null
  changes: JsonRevisionDifference[]
  detectedAt: string
}

let templateCache: ContentTemplateDefinition[] | null = null

function cloneDraft(draft: ContentDraftV2): ContentDraftV2 {
  // Drafts are pure JSON documents by contract; a JSON round trip also avoids
  // structuredClone failing on Pinia's reactive proxies.
  return JSON.parse(JSON.stringify(draft)) as ContentDraftV2
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
  if (error instanceof Error && error.message) return error.message
  return fallback
}

export const useContentEditorStore = defineStore('contentEditor', () => {
  const record = ref<ContentRecordV2 | null>(null)
  const draft = ref<ContentDraftV2 | null>(null)
  const template = ref<ContentTemplateDefinition | null>(null)
  const revisions = ref<ContentRevisionV2[]>([])
  const loadState = ref<EditorLoadState>('loading')
  const saveState = ref<EditorSaveState>('idle')
  const loadError = ref('')
  const saveError = ref('')
  const lastSavedAt = ref<string | null>(null)
  const conflict = ref<EditorConflict | null>(null)
  const conflictDismissed = ref(false)

  let saveTimer: ReturnType<typeof setTimeout> | undefined
  let saving = false
  let saveCompletion: Promise<void> | null = null
  let resolveSaveCompletion: (() => void) | null = null
  let queuedSave = false
  let changeSequence = 0
  let savedSequence = 0

  const isDirty = computed(() => changeSequence !== savedSequence)
  const canonicalPath = computed(() => (
    draft.value ? canonicalPathForDraft(draft.value, template.value) : null
  ))
  const saveLabel = computed(() => {
    if (saveState.value === 'saving') return '正在保存…'
    if (saveState.value === 'dirty') return '有未保存更改'
    if (saveState.value === 'conflict') return '保存已停止（版本冲突）'
    if (saveState.value === 'error') return '保存失败'
    if (lastSavedAt.value) return `已保存 ${lastSavedAt.value}`
    return '尚未保存'
  })

  function clearTimer(): void {
    if (saveTimer !== undefined) {
      clearTimeout(saveTimer)
      saveTimer = undefined
    }
  }

  async function templates(): Promise<ContentTemplateDefinition[]> {
    templateCache ??= await contentApi.listTemplates()
    return templateCache
  }

  async function resolveTemplate(key: ContentDraftV2['templateKey']): Promise<void> {
    const registry = await templates()
    template.value = registry.find((entry) => entry.key === key) ?? null
  }

  function applyRecord(next: ContentRecordV2, options: { keepLocalEdits: boolean } = { keepLocalEdits: false }): void {
    record.value = next
    if (options.keepLocalEdits && draft.value && changeSequence !== savedSequence) {
      draft.value = { ...draft.value, draftVersion: next.draft.draftVersion }
    } else {
      draft.value = next.draft
    }
  }

  async function load(id: string): Promise<void> {
    clearTimer()
    loadState.value = 'loading'
    loadError.value = ''
    conflict.value = null
    conflictDismissed.value = false
    try {
      const { record: loaded } = await contentApi.getContentRecord(id)
      record.value = loaded
      draft.value = loaded.draft
      await resolveTemplate(loaded.draft.templateKey)
      changeSequence = 0
      savedSequence = 0
      saveState.value = 'idle'
      loadState.value = 'ready'
    } catch (error) {
      const status = errorStatus(error)
      loadState.value = status === 403 ? 'forbidden' : status === 404 ? 'empty' : 'error'
      loadError.value = errorMessage(error, '内容草稿加载失败。')
    }
  }

  async function create(next: ContentDraftV2): Promise<string> {
    const created = await contentApi.createContent(next)
    record.value = created.record
    draft.value = created.record.draft
    await resolveTemplate(created.record.draft.templateKey)
    changeSequence = 0
    savedSequence = 0
    saveState.value = 'saved'
    loadState.value = 'ready'
    lastSavedAt.value = new Date().toLocaleTimeString('zh-CN')
    return created.record.id
  }

  function patch(mutator: (value: ContentDraftV2) => ContentDraftV2 | void, options: { autosave?: boolean } = {}): void {
    if (!draft.value) return
    const next = cloneDraft(draft.value)
    const result = mutator(next)
    draft.value = result ?? next
    changeSequence += 1
    saveState.value = saveState.value === 'conflict' ? 'conflict' : 'dirty'
    if (saveState.value === 'conflict') return
    if (options.autosave === false) return
    scheduleAutosave()
  }

  function scheduleAutosave(): void {
    clearTimer()
    saveTimer = setTimeout(() => {
      saveTimer = undefined
      void save()
    }, AUTOSAVE_DEBOUNCE_MS)
  }

  async function save(): Promise<boolean> {
    if (saveState.value === 'conflict') return false
    if (!draft.value || !record.value) return false
    if (saving) {
      queuedSave = true
      return false
    }

    clearTimer()
    saving = true
    saveCompletion = new Promise((resolve) => {
      resolveSaveCompletion = resolve
    })
    queuedSave = false
    saveError.value = ''
    saveState.value = 'saving'
    const sentSequence = changeSequence
    const sentDraft = cloneDraft(draft.value)

    try {
      const saved = await contentApi.saveDraft(record.value.id, sentDraft)
      savedSequence = sentSequence
      applyRecord(saved.record, { keepLocalEdits: true })
      lastSavedAt.value = new Date().toLocaleTimeString('zh-CN')
      if (changeSequence !== sentSequence) {
        saveState.value = 'dirty'
        queuedSave = true
      } else {
        saveState.value = 'saved'
      }
      return true
    } catch (error) {
      if (errorStatus(error) === 409) {
        await enterConflict()
        return false
      }
      saveState.value = 'error'
      saveError.value = errorMessage(error, '草稿保存失败，请检查字段与网络后重试。')
      return false
    } finally {
      saving = false
      resolveSaveCompletion?.()
      resolveSaveCompletion = null
      saveCompletion = null
      if (queuedSave && (saveState.value === 'dirty' || saveState.value === 'saved')) {
        scheduleAutosave()
      }
      queuedSave = false
    }
  }

  async function enterConflict(): Promise<void> {
    clearTimer()
    queuedSave = false
    saveState.value = 'conflict'
    conflictDismissed.value = false
    const localDraft = draft.value
    if (!localDraft || !record.value) return
    let serverRecord: ContentRecordV2 | null = null
    try {
      serverRecord = (await contentApi.getContentRecord(record.value.id)).record
    } catch {
      serverRecord = null
    }
    conflict.value = {
      localDraft,
      serverRecord,
      changes: serverRecord
        ? diffJsonRevisions(serverRecord.draft, localDraft)
        : [],
      detectedAt: new Date().toLocaleString('zh-CN'),
    }
  }

  function dismissConflict(): void {
    conflictDismissed.value = true
  }

  function reopenConflict(): void {
    conflictDismissed.value = false
  }

  async function reloadServerVersion(): Promise<boolean> {
    if (!record.value) return false
    const { record: serverRecord } = await contentApi.getContentRecord(record.value.id)
    record.value = serverRecord
    draft.value = serverRecord.draft
    await resolveTemplate(serverRecord.draft.templateKey)
    changeSequence = 0
    savedSequence = 0
    conflict.value = null
    conflictDismissed.value = false
    saveState.value = 'idle'
    return true
  }

  function conflictJson(): string {
    return JSON.stringify(conflict.value?.localDraft ?? draft.value, null, 2)
  }

  async function flush(): Promise<boolean> {
    clearTimer()
    for (let attempt = 0; attempt < 10; attempt += 1) {
      if (saving) {
        await saveCompletion
        continue
      }
      if (!isDirty.value) return saveState.value !== 'conflict' && saveState.value !== 'error'
      if (!(await save())) return false
    }
    saveState.value = 'error'
    saveError.value = '等待串行保存完成时超时，请重试。'
    return false
  }

  async function snapshot(intent: 'manual', reason: string): Promise<ContentRecordV2 | null> {
    if (!record.value || !draft.value) return null
    if (!(await flush())) return null
    const result = await contentApi.createSnapshot(
      record.value.id,
      draft.value.draftVersion,
      intent,
      reason,
    )
    applyRecord(result.record)
    saveState.value = 'saved'
    return result.record
  }

  async function publish(reason: string): Promise<ContentRecordV2 | null> {
    if (!record.value || !draft.value) return null
    if (!(await flush())) return null
    const result = await contentApi.publishContent(
      record.value.id,
      draft.value.draftVersion,
      reason,
    )
    applyRecord(result.record)
    saveState.value = 'saved'
    return result.record
  }

  async function loadRevisions(): Promise<void> {
    if (!record.value) return
    revisions.value = await contentApi.listRevisions(record.value.id)
  }

  async function compare(baseRevision: number, targetRevision?: number): Promise<ContentDiffV2> {
    if (!record.value) throw new Error('No content record is loaded.')
    return contentApi.diff(record.value.id, baseRevision, targetRevision)
  }

  async function restoreRevision(revision: number, reason: string): Promise<boolean> {
    if (!record.value || !draft.value) return false
    const result = await contentApi.restoreRevision(
      record.value.id,
      draft.value.draftVersion,
      revision,
      reason,
    )
    applyRecord(result.record)
    changeSequence = 0
    savedSequence = 0
    saveState.value = 'saved'
    lastSavedAt.value = new Date().toLocaleTimeString('zh-CN')
    return true
  }

  function reset(): void {
    clearTimer()
    record.value = null
    draft.value = null
    template.value = null
    revisions.value = []
    conflict.value = null
    conflictDismissed.value = false
    loadState.value = 'loading'
    saveState.value = 'idle'
    loadError.value = ''
    saveError.value = ''
    lastSavedAt.value = null
    changeSequence = 0
    savedSequence = 0
  }

  return {
    record,
    draft,
    template,
    revisions,
    loadState,
    saveState,
    loadError,
    saveError,
    lastSavedAt,
    conflict,
    conflictDismissed,
    isDirty,
    canonicalPath,
    saveLabel,
    load,
    create,
    patch,
    save,
    flush,
    scheduleAutosave,
    enterConflict,
    dismissConflict,
    reopenConflict,
    reloadServerVersion,
    conflictJson,
    snapshot,
    publish,
    loadRevisions,
    compare,
    restoreRevision,
    reset,
  }
})
