import { describe, expect, it, vi } from 'vitest'
import { contentApi } from '@/features/content/services/contentApi'
import { product, response, setupAdminApiTestEnvironment } from '@/shared/services/adminApi.testSupport'

setupAdminApiTestEnvironment()

describe('CMS relation product search', () => {
  it('only requests currently published products', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request) => {
      void input
      return response({
        items: [product('10000000-0000-4000-8000-000000000001')],
        nextCursor: null,
        total: 1,
        familyCounts: [],
        statusCounts: [],
        dataStateCounts: [],
      })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(contentApi.searchProducts({ q: 'AX', limit: 6 })).resolves.toMatchObject({
      items: [{ id: '10000000-0000-4000-8000-000000000001' }],
    })
    const url = new URL(String(fetchMock.mock.calls[0]?.[0]))
    expect(url.searchParams.get('status')).toBe('published')
    expect(url.searchParams.get('q')).toBe('AX')
  })
})
