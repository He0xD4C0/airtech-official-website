import { computed, ref } from 'vue'
import { defineStore } from 'pinia'
import type { MediaAsset } from '@airtek/contracts'
import { uploadMediaAsset } from '@/services/mediaApi'

export const PENDING_MEDIA_PREFIX = 'pending-media:'

interface PendingMedia {
  id: string
  file: File
  objectUrl: string
  uploaded: MediaAsset | null
}

export interface DeferredUploadResult<T> {
  value: T
  complete: boolean
  error?: unknown
}

function pendingIds(value: unknown, ids = new Set<string>()): Set<string> {
  if (Array.isArray(value)) {
    for (const entry of value) pendingIds(entry, ids)
  } else if (value && typeof value === 'object') {
    for (const [key, entry] of Object.entries(value)) {
      if (key === 'assetId' && typeof entry === 'string' && entry.startsWith(PENDING_MEDIA_PREFIX)) {
        ids.add(entry)
      } else {
        pendingIds(entry, ids)
      }
    }
  }
  return ids
}

function replaceIds<T>(value: T, replacements: ReadonlyMap<string, string>): T {
  return JSON.parse(JSON.stringify(value), (key, entry) => (
    key === 'assetId' && typeof entry === 'string'
      ? replacements.get(entry) ?? entry
      : entry
  )) as T
}

export const useDeferredMediaUploads = defineStore('deferredMediaUploads', () => {
  const entries = ref<Record<string, PendingMedia>>({})
  const completed = ref(0)
  const total = ref(0)

  const objectUrls = computed<Record<string, string>>(() => Object.fromEntries(
    Object.values(entries.value).map((entry) => [entry.id, entry.objectUrl]),
  ))
  const uploadLabel = computed(() => total.value > 0
    ? `正在上传并生成预览 ${completed.value}/${total.value}`
    : '')

  function register(file: File): string {
    const id = `${PENDING_MEDIA_PREFIX}${crypto.randomUUID()}`
    entries.value[id] = {
      id,
      file,
      objectUrl: URL.createObjectURL(file),
      uploaded: null,
    }
    return id
  }

  function discard(id: string): void {
    const entry = entries.value[id]
    if (!entry) return
    URL.revokeObjectURL(entry.objectUrl)
    delete entries.value[id]
  }

  async function uploadValue<T>(value: T): Promise<DeferredUploadResult<T>> {
    const ids = [...pendingIds(value)]
    total.value = ids.length
    completed.value = 0
    const replacements = new Map<string, string>()
    let error: unknown
    for (const id of ids) {
      const entry = entries.value[id]
      if (!entry) {
        error = new Error('待上传图片已不在当前浏览器会话中，请重新选择。')
        break
      }
      try {
        entry.uploaded ??= await uploadMediaAsset(entry.file, `editor-${id}`)
        replacements.set(id, entry.uploaded.id)
        completed.value += 1
      } catch (caught) {
        error = caught
        break
      }
    }
    const replaced = replaceIds(value, replacements)
    for (const id of replacements.keys()) discard(id)
    if (!error) {
      completed.value = 0
      total.value = 0
    }
    return { value: replaced, complete: !error, ...(error ? { error } : {}) }
  }

  function clear(): void {
    for (const id of Object.keys(entries.value)) discard(id)
    completed.value = 0
    total.value = 0
  }

  return {
    entries,
    objectUrls,
    uploadLabel,
    register,
    discard,
    uploadValue,
    clear,
  }
})

export { pendingIds, replaceIds }
