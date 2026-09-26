import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { ApiError } from '@airtek/contracts'
import type { CmsPrivateDraft, ContentDraftV2, ContentTemplateDefinition } from '@airtek/contracts'
import { contentApi } from '@/features/content/services/contentApi'
import { EDITOR_HISTORY_LIMIT, useContentEditorStore } from '@/features/content/stores/contentEditor'

vi.mock('@/features/content/services/contentApi', () => ({
  contentApi: {
    listTemplates: vi.fn(), getDraft: vi.fn(), createDraft: vi.fn(),
    saveDraft: vi.fn(), submit: vi.fn(),
  },
}))

const draftId = '22222222-0000-4000-8000-000000000001'
const contentId = '22222222-0000-4000-8000-000000000002'
const template: ContentTemplateDefinition = {
  key: 'articleDetail', contentKind: 'article', bodyPolicy: 'required',
  requiredBlocks: ['hero', 'body'], allowedBlocks: ['hero', 'body', 'media', 'cta'],
  routable: true, routePattern: '/{locale}/resources/articles/{slug}', singletonPerLocale: false,
}

function document(version = 1, title = 'Article'): ContentDraftV2 {
  return {
    schemaVersion: 2, kind: 'article', locale: 'en', templateKey: 'articleDetail',
    title, slug: 'article', summary: null, isPlaceholder: false,
    typeFields: {
      type: 'article', category: null, authorDisplayName: null,
      publicationAt: null, cover: null, featured: false,
    },
    body: { type: 'doc', content: [] }, composition: { blocks: [] },
    seo: { title: null, description: null, indexable: false, socialImage: null },
    relations: [], draftVersion: version,
  }
}

function privateDraft(version = 1, title = 'Article'): CmsPrivateDraft {
  return {
    draftId, contentId, ownerUserId: '11111111-0000-4000-8000-000000000001',
    document: document(version, title), draftVersion: version, basePublicationVersion: 0,
    state: 'editing', rejectionReason: null,
    createdAt: '2026-09-15T00:00:00Z', updatedAt: '2026-09-15T00:00:00Z',
  }
}

beforeEach(() => {
  setActivePinia(createPinia())
  vi.resetAllMocks()
  vi.mocked(contentApi.listTemplates).mockResolvedValue([template])
  vi.mocked(contentApi.getDraft).mockResolvedValue({ draft: privateDraft(), etag: '"draft-1"' })
})

describe('content editor session', () => {
  it('loads a private draft and derives the unchanged canonical path', async () => {
    const store = useContentEditorStore()
    await store.load(draftId)
    expect(store.loadState).toBe('ready')
    expect(store.canonicalPath).toBe('/en/resources/articles/article')
  })

  it('keeps typing, undo, redo and local preview state off the network', async () => {
    const store = useContentEditorStore()
    await store.load(draftId)
    vi.clearAllMocks()
    store.patch((value) => { value.title = 'A' }, { historyKey: 'title' })
    store.patch((value) => { value.title = 'AB' }, { historyKey: 'title' })
    expect(store.draft?.title).toBe('AB')
    store.undo()
    expect(store.draft?.title).toBe('Article')
    store.redo()
    expect(store.draft?.title).toBe('AB')
    expect(contentApi.saveDraft).not.toHaveBeenCalled()
    expect(contentApi.submit).not.toHaveBeenCalled()
  })

  it('writes only after explicit save and blocks dirty submission', async () => {
    const store = useContentEditorStore()
    await store.load(draftId)
    vi.mocked(contentApi.saveDraft).mockImplementation(async (_id, sent) => ({
      draft: { ...privateDraft(2, sent.title), document: { ...sent, draftVersion: 2 } },
      etag: '"draft-2"',
    }))
    vi.mocked(contentApi.submit).mockResolvedValue({
      status: 'pendingReview', draft: { ...privateDraft(2, 'Saved'), state: 'pendingReview' },
      publication: null,
    })
    store.patch((value) => { value.title = 'Saved' }, { historyKey: 'title' })
    await expect(store.submit()).resolves.toBeNull()
    expect(contentApi.submit).not.toHaveBeenCalled()
    expect(contentApi.saveDraft).not.toHaveBeenCalled()
    await expect(store.save()).resolves.toBe(true)
    expect(contentApi.saveDraft).toHaveBeenCalledTimes(1)
    await expect(store.submit()).resolves.toMatchObject({ status: 'pendingReview' })
    expect(contentApi.submit).toHaveBeenCalledTimes(1)
  })

  it('caps independent undo states at 100', async () => {
    const store = useContentEditorStore()
    await store.load(draftId)
    for (let index = 0; index < EDITOR_HISTORY_LIMIT + 20; index += 1) {
      store.patch((value) => { value.title = `${index}` }, { historyKey: `title-${index}` })
    }
    expect(store.undoDepth).toBe(EDITOR_HISTORY_LIMIT)
  })

  it('surfaces an explicit-save conflict and reloads current database state', async () => {
    const store = useContentEditorStore()
    await store.load(draftId)
    store.patch((value) => { value.title = 'Local' })
    vi.mocked(contentApi.saveDraft).mockRejectedValue(new ApiError({
      type: 'about:blank', title: 'Conflict', status: 409,
      detail: 'Draft changed', requestId: '33333333-0000-4000-8000-000000000001',
    } as never))
    await expect(store.save()).resolves.toBe(false)
    expect(store.saveState).toBe('conflict')
    expect(store.saveLabel).toBe('版本冲突')
    store.patch((value) => { value.title = 'Still local' })
    expect(store.saveState).toBe('conflict')
    expect(store.saveError).toContain('Draft changed')
    vi.mocked(contentApi.getDraft).mockResolvedValue({ draft: privateDraft(3, 'Server'), etag: '"draft-3"' })
    await store.reloadServerVersion()
    expect(store.draft?.title).toBe('Server')
    expect(store.saveState).toBe('saved')
  })
})
