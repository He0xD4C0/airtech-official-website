import type { SessionUser as ContractSessionUser } from '@airtek/contracts'
import { ApiError as ContractApiError } from '@airtek/contracts'
import { devtoolsPermissions } from 'virtual:devtools-routes'
import type { Permission, SessionUser } from '@/shared/types/domain'
import { clearAdminCsrfToken } from '@/shared/services/adminCsrf'
import { adminContractClient } from '@/shared/services/adminApiTransport'
import type { AcceptInvitationRequest, AdminSession, InvitationAcceptance, RecoveryCodeSet, RecoveryKeyState, TotpEnrollment } from '@/shared/services/adminApiTypes'

function sessionEnvironment(value: string): SessionUser['environment'] {
  switch (value) {
    case 'development':
    case 'staging':
    case 'production':
      return value
    default:
      throw new TypeError(`Unsupported API environment: ${value}`)
  }
}

function permission(value: string): Permission | undefined {
  switch (value) {
    case 'dashboard.read':
    case 'content.read':
    case 'content.write':
    case 'content.publish':
    case 'product.read':
    case 'product.write':
    case 'product.publish':
    case 'product.pricing.read':
    case 'integration.run':
    case 'media.write':
    case 'rfq.read':
    case 'rfq.read_pii':
    case 'rfq.assign':
    case 'analytics.read':
    case 'identity.manage':
    case 'identity.roles.manage':
    case 'audit.read':
    case 'settings.manage':
    case 'mail.manage':
    case 'sms.manage':
    case 'captcha.manage':
      return value
    default:
      return devtoolsPermissions.find((candidate) => candidate === value)
  }
}

function sessionUser(session: ContractSessionUser): SessionUser {
  return {
    ...session,
    environment: sessionEnvironment(session.environment),
    permissions: session.permissions.flatMap((value) => {
      const recognizedPermission = permission(value)
      return recognizedPermission ? [recognizedPermission] : []
    }),
  }
}

export const adminAuthApi = {
  async session(): Promise<SessionUser | null> {
    try {
      const result = await adminContractClient.get('/api/admin/v1/auth/session')
      return sessionUser(result.data)
    } catch (error) {
      const status = error instanceof ContractApiError ? error.problem.status : undefined
      if (status === 401) return null
      throw error
    }
  },

  async login(email: string, password: string, otp?: string): Promise<SessionUser> {
    const result = await adminContractClient.post('/api/admin/v1/auth/login', {
      body: { email, password, ...(otp === undefined ? {} : { otp }) },
    })
    return sessionUser(result.data)
  },

  async setup(displayName: string, email: string, password: string, bootstrapToken: string): Promise<SessionUser> {
    const result = await adminContractClient.post('/api/admin/v1/auth/setup', {
      body: { displayName, email, password, bootstrapToken },
    })
    return sessionUser(result.data)
  },

  async acceptInvitation(payload: AcceptInvitationRequest): Promise<InvitationAcceptance> {
    const result = await adminContractClient.post('/api/admin/v1/auth/invitations/accept', { body: payload })
    return result.data
  },

  async logout(): Promise<void> {
    try {
      await adminContractClient.post('/api/admin/v1/auth/logout')
    } finally {
      clearAdminCsrfToken()
    }
  },

  async changePassword(currentPassword: string, newPassword: string): Promise<void> {
    await adminContractClient.post('/api/admin/v1/auth/password', {
      body: { currentPassword, newPassword },
    })
  },

  async recoveryKey(): Promise<RecoveryKeyState> {
    const result = await adminContractClient.get('/api/admin/v1/auth/recovery-key')
    return result.data
  },

  async confirmRecoveryKey(): Promise<void> {
    await adminContractClient.post('/api/admin/v1/auth/recovery-key/confirm')
  },

  async startTotpEnrollment(): Promise<TotpEnrollment> {
    const result = await adminContractClient.post('/api/admin/v1/auth/totp/enrollment')
    return result.data
  },

  async confirmTotpEnrollment(code: string): Promise<RecoveryCodeSet> {
    const result = await adminContractClient.post('/api/admin/v1/auth/totp/confirm', { body: { code } })
    return result.data
  },

  async regenerateRecoveryCodes(code: string): Promise<RecoveryCodeSet> {
    const result = await adminContractClient.post('/api/admin/v1/auth/recovery-codes/regenerate', {
      body: { code },
    })
    return result.data
  },

  async listSessions(): Promise<AdminSession[]> {
    const result = await adminContractClient.get('/api/admin/v1/auth/sessions')
    return result.data
  },

  async revokeSession(id: string): Promise<void> {
    await adminContractClient.delete('/api/admin/v1/auth/sessions/{id}', {
      parameters: { path: { id } },
    })
  },
}
