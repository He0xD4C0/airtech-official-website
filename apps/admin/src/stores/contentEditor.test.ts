import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { ApiError } from '@airtek/contracts'
import type { ContentDraftV2, ContentRecordV2, ContentTemplateDefinition } from '@airtek/contracts'
import { contentApi } from '@/services/contentApi'
import { AUTOSAVE_DEBOUNCE_MS, useContentEditorStore } from './contentEditor'

async function flushPromises(): Promise<void> {
  await Promise.resolve()
  await Promise.resolve()
  await Promise.resolve()
}

vi.mock('@/services/contentApi', () => ({
  contentApi: {
    listTemplates: vi.fn(),
    listContent: vi.fn(),
    getContentRecord: vi.fn(),
    createContent: vi.fn(),
    saveDraft: vi.fn(),
    createSnapshot: vi.fn(),
    listRevisions: vi.fn(),
    diff: vi.fn(),
    restoreRevision: vi.fn(),
    listMediaAssets: vi.fn(),
    searchProducts: vi.fn(),
  },
}))

const template: ContentTemplateDefinition = {
  key: 'articleDetail',
  contentKind: 'article',
  bodyPolicy: 'required',
  requiredBlocks: ['hero', 'body'],
  allowedBlocks: ['hero', 'body', 'media', 'cta'],
  routable: true,
  singletonPerLocale: false,
}

function draft(version: number, title = 'Article'): ContentDraftV2 {
  return {
    schemaVersion: 2,
    kind: 'article',
    locale: 'en',
    templateKey: 'articleDetail',
    title,
    slug: 'article',
    summary: null,
    isPlaceholder: false,
    typeFields: {
      type: 'article',
      category: null,
      authorDisplayName: null,
      publicationAt: null,
      cover: null,
      featured: false,
    },
    body: { type: 'doc', content: [] },
    composition: {
      blocks: [
        {
          type: 'hero',
          id: '11111111-0000-4000-8000-000000000001',
          eyebrow: null,
          heading: title,
          lead: null,
          media: null,
          actions: [],
          variant: 'standard',
        },
      ],
    },
    seo: { title: null, description: null, indexable: false, socialImage: null },
    relations: [],
    draftVersion: version,
  }
}

function record(version: number, title = 'Article'): ContentRecordV2 {
  return {
    id: '22222222-0000-4000-8000-000000000001',
    status: 'draft',
    draft: draft(version, title),
    latestRevision: null,
    publishedRevision: null,
    createdAt: '2026-09-10T00:00:00Z',
    updatedAt: '2026-09-10T00:00:00Z',
    updatedBy: 'admin',
  }
}

function conflictError(): ApiError {
  return new ApiError({
    type: 'about:blank',
    title: 'Conflict',
    status: 409,
    detail: 'The draft changed since it was loaded.',
    requestId: '33333333-0000-4000-8000-000000000001',
  } as never)
}

function useStore() {
  return useContentEditorStore()
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.useFakeTimers()
  vi.mocked(contentApi.listTemplates).mockResolvedValue([template])
  vi.mocked(contentApi.getContentRecord).mockResolvedValue({ record: record(1), etag: '"draft-1"' })
})

afterEach(() => {
  vi.useRealTimers()
})

describe('content editor store', () => {
  it('loads a draft, resolves the template and derives the canonical path', async () => {
    const store = useStore()
    await store.load('22222222-0000-4000-8000-000000000001')
    expect(store.loadState).toBe('ready')
    expect(store.template?.key).toBe('articleDetail')
    expect(store.canonicalPath).toBe('/en/article')
    expect(store.saveState).toBe('idle')
  })

  it('debounces autosave for 1.2s and bumps the draft version', async () => {
    vi.mocked(contentApi.saveDraft).mockImplementation(async (_id, sent) => ({
      record: { ...record(sent.draftVersion + 1, sent.title), draft: { ...sent, draftVersion: sent.draftVersion + 1 } },
      etag: `"draft-${sent.draftVersion + 1}"`,
    }))
    const store = useStore()
    await store.load('22222222-0000-4000-8000-000000000001')

    store.patch((value) => {
      value.title = 'Changed'
    })
    expect(store.saveState).toBe('dirty')
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS - 1)
    expect(contentApi.saveDraft).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(1)
    await flushPromises()

    expect(contentApi.saveDraft).toHaveBeenCalledTimes(1)
    expect(contentApi.saveDraft).toHaveBeenCalledWith(
      '22222222-0000-4000-8000-000000000001',
      expect.objectContaining({ title: 'Changed', draftVersion: 1 }),
    )
    expect(store.draft?.draftVersion).toBe(2)
    expect(store.saveState).toBe('saved')
  })

  it('never runs two saves in parallel and queues a follow-up save', async () => {
    let resolveFirst: ((value: { record: ContentRecordV2; etag: string }) => void) | undefined
    vi.mocked(contentApi.saveDraft).mockImplementationOnce(() => new Promise((resolve) => {
      resolveFirst = resolve
    }))
    vi.mocked(contentApi.saveDraft).mockImplementation(async (_id, sent) => ({
      record: { ...record(sent.draftVersion + 1, sent.title), draft: { ...sent, draftVersion: sent.draftVersion + 1 } },
      etag: `"draft-${sent.draftVersion + 1}"`,
    }))

    const store = useStore()
    await store.load('22222222-0000-4000-8000-000000000001')
    store.patch((value) => {
      value.title = 'First'
    })
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS)
    expect(contentApi.saveDraft).toHaveBeenCalledTimes(1)

    store.patch((value) => {
      value.title = 'Second'
    })
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS)
    expect(contentApi.saveDraft).toHaveBeenCalledTimes(1)

    resolveFirst?.({ record: record(2, 'First'), etag: '"draft-2"' })
    await flushPromises()
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS)
    await flushPromises()
    expect(contentApi.saveDraft).toHaveBeenCalledTimes(2)
    expect(store.draft?.title).toBe('Second')
  })

  it('stops autosave on 409, exposes a diff and reloads the server version', async () => {
    vi.mocked(contentApi.getContentRecord)
      .mockResolvedValueOnce({ record: record(1), etag: '"draft-1"' })
      .mockResolvedValueOnce({ record: record(3, 'Server title'), etag: '"draft-3"' })
      .mockResolvedValueOnce({ record: record(3, 'Server title'), etag: '"draft-3"' })
    vi.mocked(contentApi.saveDraft).mockRejectedValue(conflictError())

    const store = useStore()
    await store.load('22222222-0000-4000-8000-000000000001')
    store.patch((value) => {
      value.title = 'Local title'
    })
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS)
    await flushPromises()

    expect(store.saveState).toBe('conflict')
    expect(store.conflict?.changes.some((change) => change.path === '$.title')).toBe(true)

    store.patch((value) => {
      value.title = 'Another local edit'
    })
    await vi.advanceTimersByTimeAsync(AUTOSAVE_DEBOUNCE_MS * 4)
    expect(contentApi.saveDraft).toHaveBeenCalledTimes(1)

    await store.reloadServerVersion()
    expect(store.draft?.title).toBe('Server title')
    expect(store.draft?.draftVersion).toBe(3)
    expect(store.conflict).toBeNull()
    expect(store.saveState).toBe('idle')
  })

  it('marks forbidden loads without throwing', async () => {
    vi.mocked(contentApi.getContentRecord).mockRejectedValue(new ApiError({
      type: 'about:blank',
      title: 'Forbidden',
      status: 403,
      detail: 'No permission',
      requestId: '44444444-0000-4000-8000-000000000001',
    } as never))
    const store = useStore()
    await store.load('22222222-0000-4000-8000-000000000001')
    expect(store.loadState).toBe('forbidden')
  })
})
