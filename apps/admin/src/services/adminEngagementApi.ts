import type { AnalyticsApiRange } from './analyticsDateRange'
import { adminContractClient, cursorQuery } from './adminApiTransport'
import type {
  AnalyticsOverview,
  CursorPage,
  CursorPageRequest,
  GuestSourceDaily,
  GuestVisitAggregate,
} from './adminApiTypes'

export const adminEngagementApi = {
  async listRfqs(pagination?: CursorPageRequest): Promise<CursorPage<Record<string, unknown>>> {
    const result = await adminContractClient.get('/api/admin/v1/rfqs', {
      parameters: { query: cursorQuery(pagination) },
    })
    return {
      items: result.data.items.map((item) => ({ ...item })),
      nextCursor: result.data.nextCursor,
    }
  },

  async listContacts(pagination?: CursorPageRequest): Promise<CursorPage<Record<string, unknown>>> {
    const result = await adminContractClient.get('/api/admin/v1/contacts', {
      parameters: { query: cursorQuery(pagination) },
    })
    return {
      items: result.data.items.map((item) => ({ ...item })),
      nextCursor: result.data.nextCursor,
    }
  },

  async analyticsOverview(range: AnalyticsApiRange): Promise<AnalyticsOverview> {
    const result = await adminContractClient.get('/api/admin/v1/analytics/overview', {
      parameters: { query: range },
    })
    return result.data
  },

  async listGuestVisits(pagination?: CursorPageRequest): Promise<CursorPage<GuestVisitAggregate>> {
    const result = await adminContractClient.get('/api/admin/v1/analytics/visits', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },

  async listGuestSources(pagination?: CursorPageRequest): Promise<CursorPage<GuestSourceDaily>> {
    const result = await adminContractClient.get('/api/admin/v1/analytics/sources', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
}
