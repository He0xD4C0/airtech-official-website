import type { AnalyticsApiRange } from './analyticsDateRange'
import type {
  AdminDashboardSummary,
  BusinessInboxDetail,
  BusinessInboxItem,
  BusinessInboxPage,
  BusinessInboxStatus,
  BusinessPii,
} from '@airtek/contracts'
import { adminContractClient, cursorQuery, randomRequestId, revisionEtag } from './adminApiTransport'
import type {
  AnalyticsOverview,
  CursorPage,
  CursorPageRequest,
  GuestSourceDaily,
} from './adminApiTypes'

export const adminEngagementApi = {
  async dashboardSummary(): Promise<AdminDashboardSummary> {
    return (await adminContractClient.get('/api/admin/v1/dashboard/summary', {})).data
  },
  async listRfqs(request: BusinessInboxListRequest = {}): Promise<BusinessInboxPage> {
    const result = await adminContractClient.get('/api/admin/v1/rfqs', {
      parameters: { query: inboxQuery(request) },
    })
    return result.data
  },

  async listContacts(request: BusinessInboxListRequest = {}): Promise<BusinessInboxPage> {
    const result = await adminContractClient.get('/api/admin/v1/contacts', {
      parameters: { query: inboxQuery(request) },
    })
    return result.data
  },

  async getRfq(id: string): Promise<BusinessInboxDetail> {
    return (await adminContractClient.get('/api/admin/v1/rfqs/{id}', { parameters: { path: { id } } })).data
  },

  async getContact(id: string): Promise<BusinessInboxDetail> {
    return (await adminContractClient.get('/api/admin/v1/contacts/{id}', { parameters: { path: { id } } })).data
  },

  async getRfqPii(id: string): Promise<BusinessPii> {
    return (await adminContractClient.get('/api/admin/v1/rfqs/{id}/pii', { parameters: { path: { id } }, cache: 'no-store' })).data
  },

  async getContactPii(id: string): Promise<BusinessPii> {
    return (await adminContractClient.get('/api/admin/v1/contacts/{id}/pii', { parameters: { path: { id } }, cache: 'no-store' })).data
  },

  async assignRfq(id: string, revision: number, assignedTo: string | null, reason: string): Promise<BusinessInboxItem> {
    return (await adminContractClient.post('/api/admin/v1/rfqs/{id}/assignment', mutation(id, revision, { assignedTo, reason }))).data
  },

  async assignContact(id: string, revision: number, assignedTo: string | null, reason: string): Promise<BusinessInboxItem> {
    return (await adminContractClient.post('/api/admin/v1/contacts/{id}/assignment', mutation(id, revision, { assignedTo, reason }))).data
  },

  async updateRfqStatus(id: string, revision: number, status: BusinessInboxStatus, reason: string): Promise<BusinessInboxItem> {
    return (await adminContractClient.post('/api/admin/v1/rfqs/{id}/status', mutation(id, revision, { status, reason }))).data
  },

  async updateContactStatus(id: string, revision: number, status: BusinessInboxStatus, reason: string): Promise<BusinessInboxItem> {
    return (await adminContractClient.post('/api/admin/v1/contacts/{id}/status', mutation(id, revision, { status, reason }))).data
  },

  async addRfqNote(id: string, revision: number, body: string, reason: string): Promise<BusinessInboxDetail> {
    return (await adminContractClient.post('/api/admin/v1/rfqs/{id}/notes', mutation(id, revision, { body, reason }))).data
  },

  async addContactNote(id: string, revision: number, body: string, reason: string): Promise<BusinessInboxDetail> {
    return (await adminContractClient.post('/api/admin/v1/contacts/{id}/notes', mutation(id, revision, { body, reason }))).data
  },

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

export interface BusinessInboxListRequest extends CursorPageRequest {
  q?: string
  status?: BusinessInboxStatus
  assignedTo?: string
}

function inboxQuery(request: BusinessInboxListRequest) {
  return {
    ...cursorQuery(request),
    ...(request.q?.trim() ? { q: request.q.trim() } : {}),
    ...(request.status ? { status: request.status } : {}),
    ...(request.assignedTo ? { assignedTo: request.assignedTo } : {}),
  }
}

function mutation<T>(id: string, revision: number, body: T) {
  return {
    parameters: {
      path: { id },
      header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) },
    },
    body,
  }
}
