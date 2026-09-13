import { describe, expect, it, vi } from 'vitest'
import { loadContentPreview } from './contentPreview'

const token = `v1.${'a'.repeat(40)}.${'b'.repeat(43)}`
const preview = {
  content: {
    schemaVersion: 2,
    id: '77935cef-4111-4c4c-bdb8-17679a8b42fe',
    kind: 'article',
    templateKey: 'articleDetail',
    slug: 'private-preview',
    locale: 'en',
    title: 'Private V2 draft',
    summary: 'Draft summary.',
    body: { type: 'doc', content: [] },
    composition: { blocks: [
      { type: 'hero', id: 'hero', eyebrow: 'Preview', heading: 'Private V2 draft', lead: null, media: null, actions: [], variant: 'standard' },
      { type: 'body', id: 'body', width: 'standard' },
    ] },
    typeFields: { type: 'article', category: null, authorDisplayName: null, publicationAt: null, cover: null, featured: false },
    seo: { title: null, description: null, indexable: false, socialImage: null },
    isPlaceholder: false,
    publishedRevision: 3,
    updatedAt: '2026-09-01T08:00:00Z',
    resolvedRelations: [],
    resolvedLinks: [],
    resolvedMedia: [],
  },
  previewExpiresAt: '2026-09-01T08:10:00Z',
}

const legacyPreview = {
  content: {
    id: '77935cef-4111-4c4c-bdb8-17679a8b42fe', kind: 'article', slug: 'private-preview', locale: 'en',
    title: 'Legacy draft', summary: null,
    body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
    seo: { title: null, description: null, canonicalPath: null, indexable: false },
    status: 'draft', isPlaceholder: false, currentRevision: 3, publishedRevision: null, scheduledFor: null,
    updatedAt: '2026-09-01T08:00:00Z',
  },
  previewExpiresAt: '2026-09-01T08:10:00Z',
}

describe('server-only content preview loader', () => {
  it('moves the URL token to an internal bearer header without API query leakage', async () => {
    const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      void input
      void init
      return new Response(JSON.stringify(preview), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      })
    })
    const value = await loadContentPreview(token, {
      baseUrl: 'http://api:8080/api/public/v1/',
      fetchImpl: fetchMock as unknown as typeof fetch,
    })
    expect(value).toEqual(preview)
    expect(fetchMock).toHaveBeenCalledWith(
      'http://api:8080/api/public/v1/content-preview',
      expect.objectContaining({
        cache: 'no-store',
        headers: { Accept: 'application/json', Authorization: `Bearer ${token}` },
      }),
    )
    expect(String(fetchMock.mock.calls[0]?.[0])).not.toContain(token)
  })

  it('maps expiry to 410 and every other failure to a safe 404 without fallback', async () => {
    for (const [status, expected] of [[410, 410], [401, 404], [500, 404]] as const) {
      const promise = loadContentPreview(token, {
        fetchImpl: vi.fn(async () => new Response(null, { status })) as unknown as typeof fetch,
      })
      await expect(promise).rejects.toMatchObject({ statusCode: expected })
    }
    await expect(loadContentPreview('too-short', { fetchImpl: vi.fn() as unknown as typeof fetch }))
      .rejects.toMatchObject({ statusCode: 404 })
    await expect(loadContentPreview(token, {
      fetchImpl: vi.fn(async () => new Response(JSON.stringify({ content: { status: 'published' } }))) as unknown as typeof fetch,
    })).rejects.toMatchObject({ statusCode: 404 })
    await expect(loadContentPreview(token, {
      fetchImpl: vi.fn(async () => new Response(JSON.stringify(legacyPreview))) as unknown as typeof fetch,
    })).rejects.toMatchObject({ statusCode: 404 })
  })
})
