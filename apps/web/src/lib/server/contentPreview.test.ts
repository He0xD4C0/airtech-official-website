import { describe, expect, it, vi } from 'vitest'
import { loadContentPreview } from './contentPreview'

const token = `v1.${'a'.repeat(40)}.${'b'.repeat(43)}`
const preview = {
  content: {
    id: '77935cef-4111-4c4c-bdb8-17679a8b42fe', kind: 'article', slug: 'private-preview', locale: 'en',
    title: 'Private draft', summary: 'Draft summary.',
    body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
    seo: { title: null, description: null, canonicalPath: '/en/resources/articles/private-preview', indexable: true },
    status: 'draft', isPlaceholder: false, currentRevision: 3, publishedRevision: 1, scheduledFor: null,
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
  })
})
