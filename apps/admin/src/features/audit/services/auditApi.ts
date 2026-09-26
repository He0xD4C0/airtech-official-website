import { adminContractClient, cursorQuery } from '@/shared/services/adminApiTransport'
import type { CursorPageRequest } from '@/shared/services/adminApiTypes'
import type { AuditEventPage } from '@airtek/contracts'

export const auditApi = {
async listAudit(request: AuditListRequest = {}): Promise<AuditEventPage> {
    const result = await adminContractClient.get('/api/admin/v1/audit', {
      parameters: { query: auditQuery(request) },
    })
    return result.data
  },

async exportAudit(request: AuditListRequest = {}): Promise<Blob> {
    const response = await adminContractClient.raw('get', '/api/admin/v1/audit/export.csv', {
      parameters: { query: auditQuery(request) },
    })
    return response.blob()
  },
}
export interface AuditListRequest extends CursorPageRequest {
  actor?: string
  action?: string
  resourceType?: string
  resourceId?: string
  from?: string
  to?: string
  q?: string
}

function auditQuery(request: AuditListRequest) {
  return {
    ...cursorQuery(request),
    ...(request.actor?.trim() ? { actor: request.actor.trim() } : {}),
    ...(request.action?.trim() ? { action: request.action.trim() } : {}),
    ...(request.resourceType?.trim() ? { resourceType: request.resourceType.trim() } : {}),
    ...(request.resourceId ? { resourceId: request.resourceId } : {}),
    ...(request.from ? { from: request.from } : {}),
    ...(request.to ? { to: request.to } : {}),
    ...(request.q?.trim() ? { q: request.q.trim() } : {}),
  }
}
