import type { AnalyticsApiRange } from '@/features/analytics/services/analyticsDateRange'

import { adminContractClient, cursorQuery } from '@/shared/services/adminApiTransport'
import type { AnalyticsOverview, CursorPage, CursorPageRequest, GuestSourceDaily } from '@/shared/services/adminApiTypes'

export const analyticsApi = {
async analyticsOverview(range: AnalyticsApiRange): Promise<AnalyticsOverview> {
    const result = await adminContractClient.get('/api/admin/v1/analytics/overview', {
      parameters: { query: range },
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
