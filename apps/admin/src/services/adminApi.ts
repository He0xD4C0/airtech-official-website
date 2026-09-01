import type { ApiProblem, FactState, SessionUser } from '@/types/domain'
import { devtoolsPermissions } from 'virtual:devtools-routes'
import {
  buildCursorPagePath,
  collectCursorPages,
  findInCursorPages,
  type CursorPage,
  type CursorPageRequest,
} from './cursorPagination'
import { shouldUseMockApi } from './runtimeMode'

export type { CursorPage, CursorPageRequest } from './cursorPagination'
export interface BackendContentEntry {
  id: string
  kind: string
  slug: string
  locale: string
  title: string
  summary: string | null
  body: { schemaVersion: number; doc: Record<string, unknown> }
  seo: { title: string | null; description: string | null; canonicalPath: string | null; indexable: boolean }
  status: 'draft' | 'scheduled' | 'published' | 'archived'
  isPlaceholder: boolean
  currentRevision: number
  publishedRevision: number | null
  updatedAt: string
}
export interface ContentDraftPayload {
  kind: string
  slug: string
  locale: 'en'
  title: string
  summary: string | null
  body: { schemaVersion: 1; doc: Record<string, unknown> }
  seo: { title: string | null; description: string | null; canonicalPath: string | null; indexable: boolean }
  isPlaceholder: boolean
}
export interface ContentPreviewLink {
  url: string
  contentId: string
  revision: number
  issuedAt: string
  expiresAt: string
}
export interface BackendProduct {
  id: string
  stableId: string
  model: string | null
  slug: string
  locale: string
  family: 'centrifugal' | 'axial' | 'crossFlow' | 'inlineDuct' | 'motors'
  subtype: string | null
  motorTechnology: string | null
  title: string
  summary: string | null
  status: 'draft' | 'scheduled' | 'published' | 'archived'
  specifications: BackendSpecValue[]
  performanceCurves: BackendPerformanceCurve[]
  sourceSnapshotId: string
  sourceRevision: string
  currentRevision: number
  publishedRevision: number | null
  indexable: boolean
  updatedAt: string
}
export interface BackendSpecValue {
  key: string
  label: string
  value: unknown | null
  unit: string | null
  operatingCondition: string | null
  state: FactState
  sourceReference: string | null
}
export interface BackendPerformancePoint {
  airflow: number
  pressure: number
}
export interface BackendPerformanceCurve {
  airflowUnit: string
  pressureUnit: string
  speedRpm: number | null
  densityKgM3: number | null
  voltage: string | null
  testMethod: string | null
  sourceReference: string
  state: FactState
  points: BackendPerformancePoint[]
}
export interface BackendTemporaryOverride {
  id: string
  productId: string
  fieldPath: string
  value: unknown
  reason: string
  createdAt: string
  expiresAt: string
  expired: boolean
}
export interface BackendSyncRun {
  id: string
  dryRun: boolean
  mappingVersion: string
  status: string
  recordsSeen: number
  recordsValid: number
  conflictCount: number
  startedAt: string
}
export interface BackendOperation {
  id: string
  kind: string
  status: 'queued' | 'running' | 'completed' | 'failed'
  reason: string
  result: unknown
  createdAt: string
  updatedAt: string
}
export interface BackendAuditEvent {
  id: string
  actor: string
  action: string
  entityType: string
  entityId: string | null
  requestId: string
  occurredAt: string
}
export interface TotpEnrollment {
  secret: string
  otpAuthUri: string
  algorithm: 'SHA1'
  digits: 6
  periodSeconds: 30
}
export interface RecoveryCodeSet {
  recoveryCodes: string[]
  generatedAt: string
}
export interface AdminSession {
  id: string
  current: boolean
  createdAt: string
  lastSeenAt: string
  expiresAt: string
}
export interface PlatformSettings {
  rfqRetentionDays: number
  retentionDeletionGraceDays: number
  temporaryOverrideDefaultDays: number
  publicLocale: 'en'
  revision: number
}
export interface UpdatePlatformSettings {
  rfqRetentionDays?: number
  retentionDeletionGraceDays?: number
  temporaryOverrideDefaultDays?: number
  reason: string
}

const baseUrl = (import.meta.env.VITE_ADMIN_API_BASE_URL ?? 'http://localhost:8080/api/admin/v1').replace(/\/$/, '')
export const mockApiEnabled = shouldUseMockApi(import.meta.env.DEV, import.meta.env.VITE_USE_MOCK_API)
let csrfTokenFromApi: string | undefined
let demoSettings: PlatformSettings = {
  rfqRetentionDays: 365,
  retentionDeletionGraceDays: 30,
  temporaryOverrideDefaultDays: 30,
  publicLocale: 'en',
  revision: 1,
}

function cookieValue(name: string): string | undefined {
  const prefix = `${encodeURIComponent(name)}=`
  return document.cookie
    .split(';')
    .map((part) => part.trim())
    .find((part) => part.startsWith(prefix))
    ?.slice(prefix.length)
}

const demoUser: SessionUser = {
  id: 'demo-admin-user',
  displayName: '开发演示管理员',
  email: 'demo@localhost.invalid',
  role: 'Super Admin · Demo',
  permissions: [
    'dashboard.read',
    'content.read',
    'content.write',
    'content.publish',
    'product.read',
    'product.write',
    'product.publish',
    'integration.run',
    'media.write',
    'rfq.read',
    'rfq.read_pii',
    'rfq.assign',
    'analytics.read',
    'identity.manage',
    'audit.read',
    'settings.manage',
    'operations.run',
    ...devtoolsPermissions,
  ],
  environment: 'development',
  totpEnabled: false,
}

function requestHeaders(init: RequestInit): Headers {
  const headers = new Headers(init.headers)
  if (init.body && !headers.has('Content-Type')) headers.set('Content-Type', 'application/json')
  headers.set('Accept', 'application/json')
  const method = (init.method ?? 'GET').toUpperCase()
  if (!['GET', 'HEAD', 'OPTIONS'].includes(method)) {
    const csrfToken = csrfTokenFromApi ?? cookieValue('airtek_admin_csrf')
    if (csrfToken) headers.set('X-CSRF-Token', decodeURIComponent(csrfToken))
  }
  return headers
}

async function responseFor(path: string, init: RequestInit = {}): Promise<Response> {
  const headers = requestHeaders(init)
  const response = await fetch(`${baseUrl}${path}`, {
    ...init,
    credentials: 'include',
    headers,
  })
  const rotatedCsrfToken = response.headers.get('X-CSRF-Token')
  if (rotatedCsrfToken) csrfTokenFromApi = rotatedCsrfToken

  if (!response.ok) {
    const problem = (await response.json().catch(() => ({
      type: 'about:blank',
      title: '请求失败',
      status: response.status,
    }))) as ApiProblem
    throw problem
  }

  return response
}

async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await responseFor(path, init)
  if (response.status === 204) return undefined as T
  return response.json() as Promise<T>
}

export async function createDevtoolsTerminalToken(): Promise<{ token: string; expiresInSeconds: number }> {
  if (!__AIRTEK_DEVTOOLS__) throw new Error('DevTools are not available in this build.')
  const devtoolsBase = baseUrl.replace(/\/api\/admin\/v1$/, '/api/devtools/v1')
  const headers = new Headers({ Accept: 'application/json' })
  const csrfToken = csrfTokenFromApi ?? cookieValue('airtek_admin_csrf')
  if (csrfToken) headers.set('X-CSRF-Token', decodeURIComponent(csrfToken))
  const response = await fetch(`${devtoolsBase}/sessions/token`, {
    method: 'POST',
    credentials: 'include',
    headers,
  })
  if (!response.ok) {
    const problem = await response.json().catch(() => null) as ApiProblem | null
    throw new Error(problem?.detail ?? problem?.title ?? 'Unable to authorize the development terminal.')
  }
  return response.json() as Promise<{ token: string; expiresInSeconds: number }>
}

export function devtoolsTerminalUrl(token: string): string {
  const devtoolsBase = baseUrl.replace(/\/api\/admin\/v1$/, '/api/devtools/v1')
  const url = new URL(`${devtoolsBase}/terminal`)
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:'
  url.searchParams.set('token', token)
  return url.toString()
}

export const adminApi = {
  async session(): Promise<SessionUser | null> {
    try {
      return await request<SessionUser>('/auth/session')
    } catch (error) {
      const status = (error as ApiProblem).status
      if (status === 401) return null
      if (mockApiEnabled) return null
      throw error
    }
  },

  async login(email: string, password: string, otp?: string): Promise<SessionUser> {
    if (mockApiEnabled) {
      await new Promise((resolve) => window.setTimeout(resolve, 420))
      if (!email || !password) {
        throw { type: 'validation', title: '请输入邮箱与密码', status: 422 } satisfies ApiProblem
      }
      return demoUser
    }

    return request<SessionUser>('/auth/login', {
      method: 'POST',
      body: JSON.stringify({ email, password, otp }),
    })
  },

  async setup(displayName: string, email: string, password: string, bootstrapToken: string): Promise<SessionUser> {
    if (mockApiEnabled) {
      await new Promise((resolve) => window.setTimeout(resolve, 420))
      return { ...demoUser, displayName, email }
    }

    return request<SessionUser>('/auth/setup', {
      method: 'POST',
      body: JSON.stringify({ displayName, email, password, bootstrapToken }),
    })
  },

  async logout(): Promise<void> {
    if (mockApiEnabled) return
    try {
      await request<void>('/auth/logout', { method: 'POST' })
    } finally {
      csrfTokenFromApi = undefined
    }
  },

  startTotpEnrollment: () => request<TotpEnrollment>('/auth/totp/enrollment', { method: 'POST' }),
  confirmTotpEnrollment: (code: string) => request<RecoveryCodeSet>('/auth/totp/confirm', {
    method: 'POST',
    body: JSON.stringify({ code }),
  }),
  regenerateRecoveryCodes: (code: string) => request<RecoveryCodeSet>('/auth/recovery-codes/regenerate', {
    method: 'POST',
    body: JSON.stringify({ code }),
  }),
  listSessions: () => request<AdminSession[]>('/auth/sessions'),
  revokeSession: (id: string) => request<void>(`/auth/sessions/${encodeURIComponent(id)}`, { method: 'DELETE' }),

  listContent: (pagination?: CursorPageRequest) => request<CursorPage<BackendContentEntry>>(buildCursorPagePath('/content', pagination)),
  findContent: (idOrSlug: string) => findInCursorPages(
    (pagination) => request<CursorPage<BackendContentEntry>>(buildCursorPagePath('/content', pagination)),
    (entry) => entry.id === idOrSlug || entry.slug === idOrSlug,
  ),
  async saveContent(payload: ContentDraftPayload, id?: string, revision?: number): Promise<{ entry: BackendContentEntry; etag: string }> {
    const response = await responseFor(id ? `/content/${id}` : '/content', {
      method: id ? 'PATCH' : 'POST',
      headers: {
        'Idempotency-Key': crypto.randomUUID(),
        ...(id && revision ? { 'If-Match': `"revision-${revision}"` } : {}),
      },
      body: JSON.stringify(payload),
    })
    return { entry: await response.json() as BackendContentEntry, etag: response.headers.get('etag') ?? '' }
  },
  async publishContent(id: string, revision: number): Promise<{ entry: BackendContentEntry; etag: string }> {
    const response = await responseFor(`/content/${id}/publish`, {
      method: 'POST',
      headers: { 'Idempotency-Key': crypto.randomUUID(), 'If-Match': `"revision-${revision}"` },
    })
    return { entry: await response.json() as BackendContentEntry, etag: response.headers.get('etag') ?? '' }
  },
  createContentPreview: (id: string, revision: number, expiresInSeconds = 600) => request<ContentPreviewLink>(
    `/content/${encodeURIComponent(id)}/preview`,
    {
      method: 'POST',
      headers: { 'If-Match': `"revision-${revision}"` },
      body: JSON.stringify({ revision, expiresInSeconds }),
    },
  ),
  listProducts: (pagination?: CursorPageRequest) => request<CursorPage<BackendProduct>>(buildCursorPagePath('/products', pagination)),
  findProduct: (id: string) => findInCursorPages(
    (pagination) => request<CursorPage<BackendProduct>>(buildCursorPagePath('/products', pagination)),
    (product) => product.id === id,
  ),
  listTemporaryOverrides: (productId: string, pagination?: CursorPageRequest) => request<CursorPage<BackendTemporaryOverride>>(
    buildCursorPagePath(`/products/${encodeURIComponent(productId)}/temporary-overrides`, pagination),
  ),
  listAllTemporaryOverrides: (productId: string) => collectCursorPages(
    (pagination) => request<CursorPage<BackendTemporaryOverride>>(
      buildCursorPagePath(`/products/${encodeURIComponent(productId)}/temporary-overrides`, pagination),
    ),
  ),
  async publishProduct(id: string, revision: number, idempotencyKey: string = crypto.randomUUID()): Promise<{ product: BackendProduct; etag: string }> {
    const response = await responseFor(`/products/${encodeURIComponent(id)}/publish`, {
      method: 'POST',
      headers: { 'Idempotency-Key': idempotencyKey, 'If-Match': `"revision-${revision}"` },
    })
    return { product: await response.json() as BackendProduct, etag: response.headers.get('etag') ?? '' }
  },
  listSyncRuns: (pagination?: CursorPageRequest) => request<CursorPage<BackendSyncRun>>(buildCursorPagePath('/feishu/sync-runs', pagination)),
  listConflicts: (pagination?: CursorPageRequest) => request<CursorPage<Record<string, unknown>>>(buildCursorPagePath('/feishu/conflicts', pagination)),
  startSync: (dryRun: boolean, mappingVersion = 'v1') => request<BackendSyncRun>('/feishu/sync-runs', {
    method: 'POST',
    headers: { 'Idempotency-Key': crypto.randomUUID() },
    body: JSON.stringify({ dryRun, mappingVersion }),
  }),
  listRfqs: (pagination?: CursorPageRequest) => request<CursorPage<Record<string, unknown>>>(buildCursorPagePath('/rfqs', pagination)),
  listContacts: (pagination?: CursorPageRequest) => request<CursorPage<Record<string, unknown>>>(buildCursorPagePath('/contacts', pagination)),
  analyticsSummary: () => request<{ acceptedEventCount: number; rfqCount: number; contactCount: number; containsPii: false; source: 'firstParty' }>('/analytics/summary'),
  async getSettings(): Promise<{ settings: PlatformSettings; etag: string }> {
    if (mockApiEnabled) {
      return { settings: { ...demoSettings }, etag: `"revision-${demoSettings.revision}"` }
    }
    const response = await responseFor('/settings')
    return {
      settings: await response.json() as PlatformSettings,
      etag: response.headers.get('etag') ?? '',
    }
  },
  async updateSettings(payload: UpdatePlatformSettings, revision: number): Promise<{ settings: PlatformSettings; etag: string }> {
    if (mockApiEnabled) {
      if (revision !== demoSettings.revision) {
        throw { type: 'conflict', title: '设置已被其他用户更新', status: 409 } satisfies ApiProblem
      }
      demoSettings = {
        ...demoSettings,
        ...(payload.rfqRetentionDays === undefined ? {} : { rfqRetentionDays: payload.rfqRetentionDays }),
        ...(payload.retentionDeletionGraceDays === undefined ? {} : { retentionDeletionGraceDays: payload.retentionDeletionGraceDays }),
        ...(payload.temporaryOverrideDefaultDays === undefined ? {} : { temporaryOverrideDefaultDays: payload.temporaryOverrideDefaultDays }),
        publicLocale: 'en',
        revision: demoSettings.revision + 1,
      }
      return { settings: { ...demoSettings }, etag: `"revision-${demoSettings.revision}"` }
    }
    const response = await responseFor('/settings', {
      method: 'PATCH',
      headers: { 'If-Match': `"revision-${revision}"` },
      body: JSON.stringify(payload),
    })
    return {
      settings: await response.json() as PlatformSettings,
      etag: response.headers.get('etag') ?? '',
    }
  },
  listOperations: (pagination?: CursorPageRequest) => request<CursorPage<BackendOperation>>(buildCursorPagePath('/operations', pagination)),
  createOperation: (kind: string, reason: string, confirmation: string, otp: string) => request<BackendOperation>('/operations', {
    method: 'POST',
    headers: { 'Idempotency-Key': crypto.randomUUID(), 'X-TOTP-Code': otp },
    body: JSON.stringify({ kind, reason, confirmation }),
  }),
  listAudit: (pagination?: CursorPageRequest) => request<CursorPage<BackendAuditEvent>>(buildCursorPagePath('/audit', pagination)),
}
