import { adminContractClient, cursorQuery, randomRequestId, revisionEtag } from './adminApiTransport'
import type { AdminRoleRecordPage, AdminUserRecordPage } from '@airtek/contracts'
import type {
  AdminRoleRecord,
  AdminUserRecord,
  CursorPage,
  CursorPageRequest,
  UpdateAdminRole,
  UpdateAdminUser,
  UserInvitation,
} from './adminApiTypes'

export interface IdentityListRequest extends CursorPageRequest {
  q?: string
  status?: 'invited' | 'active' | 'disabled'
}

export const adminIdentityApi = {
  async listUsers(request: IdentityListRequest = {}): Promise<AdminUserRecordPage> {
    const result = await adminContractClient.get('/api/admin/v1/users', {
      parameters: { query: {
        ...cursorQuery(request),
        ...(request.q?.trim() ? { q: request.q.trim() } : {}),
        ...(request.status ? { status: request.status } : {}),
      } },
    })
    return result.data
  },

  async getUser(id: string): Promise<{ user: AdminUserRecord; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/users/{id}', {
      parameters: { path: { id } },
    })
    return { user: result.data, etag: result.etag ?? `"revision-${result.data.revision}"` }
  },

  async listUserInvitations(): Promise<CursorPage<UserInvitation>> {
    const result = await adminContractClient.get('/api/admin/v1/user-invitations')
    return result.data
  },

  async inviteUser(payload: { email: string; displayName: string; roleKeys: string[] }): Promise<UserInvitation> {
    const result = await adminContractClient.post('/api/admin/v1/user-invitations', {
      parameters: { header: { 'Idempotency-Key': randomRequestId() } },
      body: payload,
    })
    return result.data
  },

  async revokeInvitation(id: string, reason: string): Promise<void> {
    await adminContractClient.post('/api/admin/v1/user-invitations/{id}/revoke', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId() } },
      body: { reason },
    })
  },

  async updateUser(id: string, revision: number, payload: UpdateAdminUser): Promise<{ user: AdminUserRecord; etag: string }> {
    const result = await adminContractClient.patch('/api/admin/v1/users/{id}', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) },
      },
      body: payload,
    })
    return { user: result.data, etag: result.etag ?? `"revision-${result.data.revision}"` }
  },

  async revokeUserSessions(id: string): Promise<void> {
    await adminContractClient.delete('/api/admin/v1/users/{id}/sessions', {
      parameters: { path: { id } },
    })
  },

  async listRoles(request: Pick<IdentityListRequest, 'cursor' | 'limit' | 'q'> = {}): Promise<AdminRoleRecordPage> {
    const result = await adminContractClient.get('/api/admin/v1/roles', {
      parameters: { query: {
        ...cursorQuery(request),
        ...(request.q?.trim() ? { q: request.q.trim() } : {}),
      } },
    })
    return result.data
  },

  async getRole(id: string): Promise<{ role: AdminRoleRecord; etag: string }> {
    const result = await adminContractClient.get('/api/admin/v1/roles/{id}', {
      parameters: { path: { id } },
    })
    return { role: result.data, etag: result.etag ?? `"revision-${result.data.revision}"` }
  },

  async updateRole(id: string, revision: number, payload: UpdateAdminRole): Promise<{ role: AdminRoleRecord; etag: string }> {
    const result = await adminContractClient.patch('/api/admin/v1/roles/{id}', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) },
      },
      body: payload,
    })
    return { role: result.data, etag: result.etag ?? `"revision-${result.data.revision}"` }
  },
}
