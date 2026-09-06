import type { ContentRecordV2 } from '@airtek/contracts'
import { describe, expect, it, vi } from 'vitest'
import { adminApi } from './adminApi'
import { response, setupAdminApiTestEnvironment } from './adminApi.testSupport'

setupAdminApiTestEnvironment()

const contentId = '70000000-0000-4000-8000-000000000001'

function newsRecord(draftVersion = 2): ContentRecordV2 {
  return {
    id: contentId,
    status: 'draft',
    latestRevision: 1,
    publishedRevision: null,
    createdAt: '2026-09-02T00:00:00Z',
    updatedAt: '2026-09-02T00:00:00Z',
    updatedBy: 'editor@example.com',
    draft: {
      schemaVersion: 2,
      kind: 'news',
      locale: 'en',
      templateKey: 'newsDetail',
      title: 'Development news',
      slug: 'development-news',
      summary: null,
      isPlaceholder: true,
      typeFields: {
        type: 'news', category: 'Development', authorDisplayName: 'AIRTEKPOWER',
        publicationAt: null, cover: null, featured: false,
      },
      body: { type: 'doc', content: [] },
      composition: { blocks: [] },
      seo: { title: null, description: null, indexable: false, socialImage: null },
      relations: [],
      draftVersion,
    },
  }
}

function generalInformationRecord(draftVersion = 2): ContentRecordV2 {
  return {
    ...newsRecord(draftVersion),
    id: '71000000-0000-4000-8000-000000000001',
    draft: {
      schemaVersion: 2,
      kind: 'generalInformation',
      locale: 'en',
      templateKey: 'generalInformation',
      title: 'General Information',
      slug: null,
      summary: null,
      isPlaceholder: true,
      typeFields: {
        type: 'generalInformation',
        organizationName: 'AIRTEKPOWER',
        brandLine: null,
        homePath: '/en',
        footerStatement: null,
        copyrightTemplate: null,
        contact: { email: null, phone: null, addressLines: [], locality: null, region: null, postalCode: null, countryCode: null },
        socialLinks: [],
        defaultSeo: { title: null, description: null, indexable: false, socialImage: null },
        productCategories: [],
        navigationCta: { label: 'Request a Quote', target: { targetType: 'route', path: '/en/request-a-quote' } },
      },
      body: null,
      composition: { blocks: [] },
      seo: { title: null, description: null, indexable: false, socialImage: null },
      relations: [],
      draftVersion,
    },
  }
}

describe('unified Admin CMS API', () => {
  it('creates content with draft-0 and a canonical V2 document', async () => {
    const created = newsRecord(1)
    const fetchMock = vi.fn(async () => (
      response(created, { ETag: '"draft-1"' })
    ))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.saveNews({
      content: {
        kind: 'news', slug: 'development-news', locale: 'en', title: 'Development news', summary: null,
        body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
        seo: { title: null, description: null, canonicalPath: '/en/resources/news/development-news', indexable: false },
        isPlaceholder: true,
      },
      category: 'Development', authorDisplayName: 'AIRTEKPOWER', featured: false,
      dataClass: 'developmentFixture',
    })

    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/api/admin/v1/content')
    expect(init?.method).toBe('POST')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"draft-0"')
    const body = JSON.parse(String(init?.body))
    expect(body.schemaVersion).toBe(2)
    expect(body.typeFields).toMatchObject({ type: 'news', category: 'Development' })
  })

  it('auto-saves News through the unified draft endpoint with draft concurrency', async () => {
    const current = newsRecord(2)
    const updated = newsRecord(3)
    const fetchMock = vi.fn(async (_input: string | URL | Request, init?: RequestInit) => (
      init?.method === 'PATCH' ? response(updated, { ETag: '"draft-3"' }) : response(current)
    ))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.saveNews({
      content: {
        kind: 'news', slug: 'development-news', locale: 'en', title: 'Development news', summary: null,
        body: { schemaVersion: 1, doc: { type: 'doc', content: [] } },
        seo: { title: null, description: null, canonicalPath: null, indexable: false },
        isPlaceholder: true,
      },
      category: 'Development', authorDisplayName: 'AIRTEKPOWER', featured: false,
      dataClass: 'developmentFixture',
    }, contentId, 2)

    const [url, init] = fetchMock.mock.calls[1] ?? []
    expect(String(url)).toContain(`/content/${contentId}/draft`)
    expect(init?.method).toBe('PATCH')
    expect(new Headers(init?.headers).get('If-Match')).toBe('"draft-2"')
    expect(JSON.parse(String(init?.body)).draftVersion).toBe(2)
  })

  it('publishes and restores through snapshots and immutable revisions', async () => {
    const fetchMock = vi.fn(async () => (
      response(newsRecord(4), { ETag: '"draft-4"' })
    ))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.publishNews(contentId, 4)
    await adminApi.rollbackNews(contentId, 4, 2, 'Restore the reviewed revision')

    const [publishUrl, publishInit] = fetchMock.mock.calls[0] ?? []
    expect(String(publishUrl)).toContain(`/content/${contentId}/snapshots`)
    expect(JSON.parse(String(publishInit?.body))).toEqual({ intent: 'publish', reason: 'Publish the current Admin CMS draft' })
    const [restoreUrl, restoreInit] = fetchMock.mock.calls[1] ?? []
    expect(String(restoreUrl)).toContain(`/content/${contentId}/revisions/2/restore`)
    expect(new Headers(restoreInit?.headers).get('If-Match')).toBe('"draft-4"')
    expect(JSON.parse(String(restoreInit?.body))).toEqual({ reason: 'Restore the reviewed revision' })
  })

  it('round-trips General Information through the shared draft document', async () => {
    const current = generalInformationRecord(2)
    const updated = generalInformationRecord(3)
    const fetchMock = vi.fn(async (_input: string | URL | Request, init?: RequestInit) => (
      init?.method === 'PATCH' ? response(updated, { ETag: '"draft-3"' }) : response(current)
    ))
    vi.stubGlobal('fetch', fetchMock)

    const result = await adminApi.saveGeneralInformation({
      locale: 'en',
      isPlaceholder: true,
      payload: {
        brandName: 'AIRTEKPOWER', brandLine: null, homePath: '/en', footerStatement: null,
        copyrightText: null, defaultSeo: { title: null, description: null },
        organization: { name: 'AIRTEKPOWER', salesEmail: null, address: null, socialLinks: [] },
        navigationCta: { label: 'Request a Quote', href: '/en/request-a-quote' },
      },
    }, current.id, 2)

    expect(result.entry.payload.brandName).toBe('AIRTEKPOWER')
    const body = JSON.parse(String(fetchMock.mock.calls[1]?.[1]?.body))
    expect(body.typeFields).toMatchObject({ type: 'generalInformation', organizationName: 'AIRTEKPOWER' })
  })

  it('loads News and General Information history from one revision route', async () => {
    const fetchMock = vi.fn(async () => (
      response({ items: [], nextCursor: null })
    ))
    vi.stubGlobal('fetch', fetchMock)

    await adminApi.listNewsRevisions(contentId)
    await adminApi.listGeneralInformationRevisions('71000000-0000-4000-8000-000000000001')

    expect(String(fetchMock.mock.calls[0]?.[0])).toContain(`/content/${contentId}/revisions`)
    expect(String(fetchMock.mock.calls[1]?.[0])).toContain('/content/71000000-0000-4000-8000-000000000001/revisions')
  })
})
