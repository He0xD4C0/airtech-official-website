import { describe, expect, it, vi } from 'vitest'
import { adminApi } from './adminApi'
import { response, setupAdminApiTestEnvironment } from './adminApi.testSupport'

setupAdminApiTestEnvironment()

describe('admin content API', () => {
  it('issues an exact-revision short-lived preview with the concurrency precondition', async () => {
    const contentId = '30000000-0000-4000-8000-000000000001'
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return new Response(JSON.stringify({
        url: 'http://www.localhost:8088/en/preview?token=signed-token',
        contentId,
        revision: 7,
        issuedAt: '2026-09-01T00:00:00Z',
        expiresAt: '2026-09-01T00:10:00Z',
      }), { status: 201, headers: { 'Content-Type': 'application/json' } })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminApi.createContentPreview(contentId, 7)).resolves.toMatchObject({
      contentId,
      revision: 7,
    })
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain(`/content/${contentId}/preview`)
    expect(init?.method).toBe('POST')
    const headers = new Headers(init?.headers)
    expect(headers.get('If-Match')).toBe('"revision-7"')
    expect(JSON.parse(String(init?.body))).toEqual({ revision: 7, expiresInSeconds: 600 })
  })

  it('saves General Information with an exact revision precondition', async () => {
    const entry = {
      id: '60000000-0000-4000-8000-000000000001',
      locale: 'en' as const,
      payload: {
        brandName: 'AIRTEKPOWER', brandLine: null, homePath: '/en', footerStatement: null,
        copyrightText: null, defaultSeo: { title: null, description: null },
        organization: {
          name: 'AIRTEKPOWER', url: null, logoUrl: null, legalName: null, salesEmail: null,
          marketingEmail: null, address: null, socialLinks: [],
        },
        navigationCta: { label: 'Request a Quote', href: '/en/request-a-quote' },
      },
      status: 'draft' as const,
      currentRevision: 3,
      publishedRevision: null,
      isPlaceholder: true,
      updatedAt: '2026-09-02T00:00:00Z',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(entry, { ETag: '"revision-3"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.saveGeneralInformation({ locale: 'en', payload: entry.payload, isPlaceholder: true }, entry.id, 2)
    const [, init] = fetchMock.mock.calls[0] ?? []
    expect(init?.method).toBe('PATCH')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-2"')
  })

  it('rolls General Information back by publishing a selected historical revision with a reason', async () => {
    const entry = {
      id: '60000000-0000-4000-8000-000000000001', locale: 'en', payload: {}, status: 'published',
      currentRevision: 5, publishedRevision: 5, isPlaceholder: true, updatedAt: '2026-09-02T00:00:00Z',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(entry, { ETag: '"revision-5"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.rollbackGeneralInformation(entry.id, 4, 2, 'Restore the reviewed site identity')
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain(`/general-information/${entry.id}/rollback`)
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-4"')
    expect(JSON.parse(String(init?.body))).toEqual({ revision: 2, reason: 'Restore the reviewed site identity' })
  })

  it('keeps News metadata separate from the versioned content document', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({
        content: {
          id: '70000000-0000-4000-8000-000000000001', kind: 'news', slug: 'development-news', locale: 'en',
          title: 'Development news', summary: null, body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
          seo: { title: null, description: null, canonicalPath: '/en/resources/news/development-news', indexable: false },
          status: 'draft', isPlaceholder: true, currentRevision: 1, publishedRevision: null, updatedAt: '2026-09-02T00:00:00Z',
        },
        category: 'Development', authorDisplayName: 'AIRTEKPOWER', coverMediaId: null, publishedAt: null,
        featured: false, dataClass: 'developmentFixture',
      })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.saveNews({
      content: {
        kind: 'news', slug: 'development-news', locale: 'en', title: 'Development news', summary: null,
        body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
        seo: { title: null, description: null, canonicalPath: '/en/resources/news/development-news', indexable: false },
        isPlaceholder: true,
      },
      category: 'Development', authorDisplayName: 'AIRTEKPOWER', featured: false, dataClass: 'developmentFixture',
    })
    const [, init] = fetchMock.mock.calls[0] ?? []
    const body = JSON.parse(String(init?.body))
    expect(body.content.kind).toBe('news')
    expect(body.category).toBe('Development')
    expect(body.content.body.doc).not.toHaveProperty('category')
  })

  it('sends the target revision and an auditable reason for News rollback', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({
        content: {
          id: '70000000-0000-4000-8000-000000000001', kind: 'news', slug: 'news', locale: 'en',
          title: 'News', summary: null, body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
          seo: { title: null, description: null, canonicalPath: null, indexable: false }, status: 'published',
          isPlaceholder: true, currentRevision: 5, publishedRevision: 5, updatedAt: '2026-09-02T00:00:00Z',
        },
        category: 'Development', authorDisplayName: 'AIRTEKPOWER', coverMediaId: null,
        publishedAt: null, featured: false, dataClass: 'developmentFixture',
      })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.rollbackNews(
      '70000000-0000-4000-8000-000000000001',
      4,
      2,
      'Restore the reviewed revision',
    )
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/news/70000000-0000-4000-8000-000000000001/rollback')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-4"')
    expect(JSON.parse(String(init?.body))).toEqual({ revision: 2, reason: 'Restore the reviewed revision' })
  })

  it('loads immutable News and General Information revision lists from their contract routes', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      const url = String(input)
      return response({ items: [], nextCursor: null }, url.includes('/revisions') ? {} : {})
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.listNewsRevisions('70000000-0000-4000-8000-000000000001')
    await adminApi.listGeneralInformationRevisions('71000000-0000-4000-8000-000000000001')

    expect(String(fetchMock.mock.calls[0]?.[0])).toContain('/api/admin/v1/news/70000000-0000-4000-8000-000000000001/revisions')
    expect(String(fetchMock.mock.calls[1]?.[0])).toContain('/api/admin/v1/general-information/71000000-0000-4000-8000-000000000001/revisions')
  })
})
