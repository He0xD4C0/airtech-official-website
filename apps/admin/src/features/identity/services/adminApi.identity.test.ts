import { adminAuthApi } from '@/shared/services/adminAuthApi'
import { adminIdentityApi } from '@/features/identity/services/adminIdentityApi'
import { describe, expect, it, vi } from 'vitest'

import { response, setupAdminApiTestEnvironment } from '@/shared/services/adminApi.testSupport'

setupAdminApiTestEnvironment()

describe('admin identity API', () => {
  it('accepts an invitation through the unauthenticated contract endpoint', async () => {
    const acceptance = {
      userId: '91000000-0000-4000-8000-000000000001', email: 'invitee@example.test', displayName: 'Invitee',
      locale: 'zh-CN', roleKeys: ['content-editor'], status: 'active', acceptedAt: '2026-09-02T00:00:00Z',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return new Response(JSON.stringify(acceptance), { status: 201, headers: { 'Content-Type': 'application/json' } })
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminAuthApi.acceptInvitation({ token: 'a'.repeat(43), password: 'secure-password-123' })).resolves.toEqual(acceptance)
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/api/admin/v1/auth/invitations/accept')
    expect(JSON.parse(String(init?.body))).toEqual({ token: 'a'.repeat(43), password: 'secure-password-123' })
  })

  it('uses the invitation resource and preserves its one-time token response', async () => {
    const invitation = {
      id: '80000000-0000-4000-8000-000000000001', email: 'editor@example.test', displayName: 'Editor',
      locale: 'zh-CN', roleKeys: ['content-editor'], status: 'pending', invitedAt: '2026-09-02T00:00:00Z',
      expiresAt: '2026-09-09T00:00:00Z', invitationToken: 'one-time-token',
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(invitation)
    })
    vi.stubGlobal('fetch', fetchMock)

    await expect(adminIdentityApi.inviteUser({
      email: invitation.email,
      displayName: invitation.displayName,
      roleKeys: invitation.roleKeys,
    })).resolves.toMatchObject({ invitationToken: 'one-time-token' })
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/user-invitations')
    expect(String(url)).not.toContain('/users')
    expect(init?.method).toBe('POST')
  })

  it('revokes invitations through the invitation resource with an audit reason', async () => {
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return new Response(null, { status: 204 })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminIdentityApi.revokeInvitation(
      '80000000-0000-4000-8000-000000000001',
      'Invitation was sent to the wrong address',
    )
    const [url, init] = fetchMock.mock.calls[0] ?? []
    expect(String(url)).toContain('/user-invitations/80000000-0000-4000-8000-000000000001/revoke')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(String(init?.body))).toEqual({ reason: 'Invitation was sent to the wrong address' })
  })

  it('updates users with the exact revision ETag', async () => {
    const user = {
      id: '90000000-0000-4000-8000-000000000001', email: 'editor@example.test', displayName: 'Editor',
      locale: 'zh-CN', status: 'active', managerUserId: null, roles: ['content-editor'], totpEnabled: true, invitedAt: null,
      lastLoginAt: null, createdAt: '2026-09-02T00:00:00Z', updatedAt: '2026-09-02T00:00:00Z', revision: 3,
    }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response(user)
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminIdentityApi.updateUser(user.id, 3, { displayName: 'Updated editor', reason: 'Correct the display name' })
    const [, init] = fetchMock.mock.calls[0] ?? []
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-3"')
    expect(new Headers(init?.headers).get('Idempotency-Key')).toBeTruthy()
  })

  it('updates roles with the exact revision ETag and audit reason', async () => {
    const role = { id: '92000000-0000-4000-8000-000000000001', key: 'publisher', displayName: 'Publisher', systemRole: true, isPreset: true, permissions: ['content.publish'], revision: 5 }
    const fetchMock = vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      void input
      void init
      return response({ ...role, revision: 6 }, { ETag: '"revision-6"' })
    })
    vi.stubGlobal('fetch', fetchMock)

    await adminIdentityApi.updateRole(role.id, 5, { displayName: 'Publisher', permissions: role.permissions, reason: 'Align approved permission matrix' })
    const [, init] = fetchMock.mock.calls[0] ?? []
    expect(new Headers(init?.headers).get('If-Match')).toBe('"revision-5"')
    expect(new Headers(init?.headers).get('Idempotency-Key')).toBeTruthy()
  })
})
