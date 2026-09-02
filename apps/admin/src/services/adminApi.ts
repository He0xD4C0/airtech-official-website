import type {
  AcceptInvitationRequest as ContractAcceptInvitationRequest,
  AdminRoleRecord as ContractAdminRoleRecord,
  AdminProductDetail as ContractAdminProductDetail,
  AdminSession as ContractAdminSession,
  AdminUserRecord as ContractAdminUserRecord,
  AuditEvent as ContractAuditEvent,
  BackgroundOperation as ContractBackgroundOperation,
  ContentDraftInput as ContractContentDraftInput,
  ContentEntry as ContractContentEntry,
  ContentPreviewLink as ContractContentPreviewLink,
  GeneralInformation as ContractGeneralInformation,
  GeneralInformationRevision as ContractGeneralInformationRevision,
  GeneralInformationPayload as ContractGeneralInformationPayload,
  GuestSourceDaily as ContractGuestSourceDaily,
  GuestVisitAggregate as ContractGuestVisitAggregate,
  InvitationAcceptance as ContractInvitationAcceptance,
  MissingAssetReference as ContractMissingAssetReference,
  NewsDraftInput as ContractNewsDraftInput,
  NewsEntry as ContractNewsEntry,
  NewsRevision as ContractNewsRevision,
  OperationKind as ContractOperationKind,
  PerformanceCurve as ContractPerformanceCurve,
  PerformancePoint as ContractPerformancePoint,
  PlatformSettings as ContractPlatformSettings,
  Product as ContractProduct,
  ProductImportAccepted as ContractProductImportAccepted,
  ProductImportError as ContractProductImportError,
  ProductImportResult as ContractProductImportResult,
  ProductPrivatePricing as ContractProductPrivatePricing,
  RecoveryCodeSet as ContractRecoveryCodeSet,
  SessionUser as ContractSessionUser,
  SpecValue as ContractSpecValue,
  SyncConflict as ContractSyncConflict,
  SyncRun as ContractSyncRun,
  TemporaryOverride as ContractTemporaryOverride,
  TotpEnrollment as ContractTotpEnrollment,
  UpdateAdminRole as ContractUpdateAdminRole,
  UpdateAdminUser as ContractUpdateAdminUser,
  UpdatePlatformSettings as ContractUpdatePlatformSettings,
  UpdateProductPresentation as ContractUpdateProductPresentation,
  UserInvitation as ContractUserInvitation,
} from '@airtek/contracts'
import { ApiError as ContractApiError, createContractClient } from '@airtek/contracts'
import { devtoolsPermissions } from 'virtual:devtools-routes'
import type { Permission, SessionUser } from '@/types/domain'
import {
  collectCursorPages,
  findInCursorPages,
  type CursorPage,
  type CursorPageRequest,
} from './cursorPagination'
import {
  captureAdminCsrfToken,
  clearAdminCsrfToken,
  getAdminCsrfToken,
} from './adminCsrf'

export type { CursorPage, CursorPageRequest } from './cursorPagination'
export type BackendContentEntry = ContractContentEntry
export type AcceptInvitationRequest = ContractAcceptInvitationRequest
export type InvitationAcceptance = ContractInvitationAcceptance
export type BackendNewsEntry = ContractNewsEntry
export type NewsDraftPayload = ContractNewsDraftInput
export interface SiteNavigationLink {
  label: string
  href: string
}
export type GeneralInformationPayload = ContractGeneralInformationPayload
export type BackendGeneralInformation = ContractGeneralInformation
export type ProductImportError = ContractProductImportError
export type MissingProductAsset = ContractMissingAssetReference
export type ProductImportResult = ContractProductImportResult
export type ProductImportAccepted = ContractProductImportAccepted
export type GuestVisitAggregate = ContractGuestVisitAggregate
export type GuestSourceDaily = ContractGuestSourceDaily
export type NewsRevision = ContractNewsRevision
export type GeneralInformationRevision = ContractGeneralInformationRevision
export type AdminUserRecord = ContractAdminUserRecord
export type AdminRoleRecord = ContractAdminRoleRecord
export type ProductPrivatePricing = ContractProductPrivatePricing
export type UpdateAdminUser = ContractUpdateAdminUser
export type UpdateAdminRole = ContractUpdateAdminRole
export type UserInvitation = ContractUserInvitation
export type ContentDraftPayload = ContractContentDraftInput
export type ContentPreviewLink = ContractContentPreviewLink
export type BackendProduct = ContractProduct & {
  sourceKind?: ContractAdminProductDetail['sourceKind']
  missingAssets?: ContractAdminProductDetail['missingAssets']
  presentation?: ContractAdminProductDetail['presentation']
}
export type ProductPresentationPayload = ContractUpdateProductPresentation
export type BackendSpecValue = ContractSpecValue
export type BackendPerformancePoint = ContractPerformancePoint
export type BackendPerformanceCurve = ContractPerformanceCurve
export type BackendTemporaryOverride = ContractTemporaryOverride
export type BackendSyncRun = ContractSyncRun
export type BackendSyncConflict = ContractSyncConflict
export type BackendOperation = ContractBackgroundOperation
export type BackendAuditEvent = ContractAuditEvent
export type TotpEnrollment = ContractTotpEnrollment
export type RecoveryCodeSet = ContractRecoveryCodeSet
export type AdminSession = ContractAdminSession
export type PlatformSettings = ContractPlatformSettings
export type UpdatePlatformSettings = ContractUpdatePlatformSettings

const baseUrl = (import.meta.env.VITE_ADMIN_API_BASE_URL ?? 'http://localhost:8080/api/admin/v1').replace(/\/$/, '')

function randomRequestId(): string {
  if (typeof crypto.randomUUID === 'function') return crypto.randomUUID()
  const bytes = crypto.getRandomValues(new Uint8Array(16))
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x40
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80
  const hex = Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`
}

const contractClient = createContractClient({
  baseUrl: new URL(baseUrl).origin,
  onResponse: captureAdminCsrfToken,
  getCsrfToken: getAdminCsrfToken,
})

function cursorQuery(pagination: CursorPageRequest = {}): { cursor?: string; limit?: number } {
  if (
    pagination.limit !== undefined
    && (!Number.isInteger(pagination.limit) || pagination.limit < 1 || pagination.limit > 100)
  ) {
    throw new RangeError('分页大小必须是 1 到 100 之间的整数。')
  }
  return {
    ...(pagination.cursor ? { cursor: pagination.cursor } : {}),
    ...(pagination.limit === undefined ? {} : { limit: pagination.limit }),
  }
}

function revisionEtag(revision: number | undefined): string {
  if (!Number.isInteger(revision) || (revision ?? 0) < 1) {
    throw new TypeError('A positive entity revision is required for this update.')
  }
  return `"revision-${revision}"`
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
    case 'audit.read':
    case 'settings.manage':
    case 'operations.run':
      return value
    default:
      return devtoolsPermissions.find((candidate) => candidate === value)
  }
}

function operationKind(value: string): ContractOperationKind {
  switch (value) {
    case 'migrationPreflight':
    case 'migrationApply':
    case 'backup':
    case 'restoreValidate':
    case 'retentionApply':
    case 'searchReindex':
    case 'cacheInvalidate':
    case 'feishuSync':
    case 'productImport':
      return value
    default:
      throw new TypeError(`Unsupported operation kind: ${value}`)
  }
}

function adminAbsoluteUrl(path: string): string {
  const base = new URL(baseUrl)
  const url = new URL(path, base.origin)
  if (url.origin !== base.origin || !url.pathname.startsWith('/api/admin/v1/')) {
    throw new Error('The operation stream URL is outside the configured Admin API boundary.')
  }
  return url.toString()
}

async function waitForOperationByPolling(id: string, deadline: number): Promise<BackendOperation> {
  while (Date.now() < deadline) {
    const result = await contractClient.get('/api/admin/v1/operations/{id}', {
      parameters: { path: { id } },
    })
    const operation = result.data
    if (operation.status === 'completed' || operation.status === 'failed') return operation
    await new Promise((resolve) => window.setTimeout(resolve, 750))
  }
  throw new Error('The background operation did not finish before the local timeout.')
}

export async function waitForOperation(
  accepted: ProductImportAccepted,
  timeoutMilliseconds = 120_000,
): Promise<BackendOperation> {
  const deadline = Date.now() + timeoutMilliseconds
  if (typeof EventSource === 'undefined') {
    return waitForOperationByPolling(accepted.operationId, deadline)
  }
  return new Promise<BackendOperation>((resolve, reject) => {
    const source = new EventSource(adminAbsoluteUrl(accepted.eventsUrl), { withCredentials: true })
    const timeout = window.setTimeout(() => {
      source.close()
      reject(new Error('The background operation did not finish before the local timeout.'))
    }, timeoutMilliseconds)
    source.addEventListener('operation', (event) => {
      try {
        const operation = JSON.parse(event.data) as BackendOperation
        if (operation.id !== accepted.operationId) throw new Error('The operation stream returned a different operation.')
        if (operation.status === 'completed' || operation.status === 'failed') {
          window.clearTimeout(timeout)
          source.close()
          resolve(operation)
        }
      } catch (error) {
        window.clearTimeout(timeout)
        source.close()
        reject(error)
      }
    })
    source.addEventListener('error', () => {
      source.close()
      void waitForOperationByPolling(accepted.operationId, deadline).then(resolve, reject)
    }, { once: true })
  })
}

export function productImportResult(operation: BackendOperation): ProductImportResult {
  if (operation.status === 'failed') throw new Error('Product Master import failed. Review the operation audit record.')
  const value = operation.result
  if (!value || typeof value !== 'object' || !('import' in value)) {
    throw new Error('The completed operation did not contain a Product Master import report.')
  }
  return (value as { import: ProductImportResult }).import
}

export const adminApi = {
  async session(): Promise<SessionUser | null> {
    try {
      const result = await contractClient.get('/api/admin/v1/auth/session')
      return sessionUser(result.data)
    } catch (error) {
      const status = error instanceof ContractApiError ? error.problem.status : undefined
      if (status === 401) return null
      throw error
    }
  },

  async login(email: string, password: string, otp?: string): Promise<SessionUser> {
    const result = await contractClient.post('/api/admin/v1/auth/login', {
      body: { email, password, ...(otp === undefined ? {} : { otp }) },
    })
    return sessionUser(result.data)
  },

  async setup(displayName: string, email: string, password: string, bootstrapToken: string): Promise<SessionUser> {
    const result = await contractClient.post('/api/admin/v1/auth/setup', {
      body: { displayName, email, password, bootstrapToken },
    })
    return sessionUser(result.data)
  },

  async acceptInvitation(payload: AcceptInvitationRequest): Promise<InvitationAcceptance> {
    const result = await contractClient.post('/api/admin/v1/auth/invitations/accept', { body: payload })
    return result.data
  },

  async logout(): Promise<void> {
    try {
      await contractClient.post('/api/admin/v1/auth/logout')
    } finally {
      clearAdminCsrfToken()
    }
  },

  async startTotpEnrollment(): Promise<TotpEnrollment> {
    const result = await contractClient.post('/api/admin/v1/auth/totp/enrollment')
    return result.data
  },
  async confirmTotpEnrollment(code: string): Promise<RecoveryCodeSet> {
    const result = await contractClient.post('/api/admin/v1/auth/totp/confirm', { body: { code } })
    return result.data
  },
  async regenerateRecoveryCodes(code: string): Promise<RecoveryCodeSet> {
    const result = await contractClient.post('/api/admin/v1/auth/recovery-codes/regenerate', {
      body: { code },
    })
    return result.data
  },
  async listSessions(): Promise<AdminSession[]> {
    const result = await contractClient.get('/api/admin/v1/auth/sessions')
    return result.data
  },
  async revokeSession(id: string): Promise<void> {
    await contractClient.delete('/api/admin/v1/auth/sessions/{id}', {
      parameters: { path: { id } },
    })
  },

  async listContent(pagination?: CursorPageRequest): Promise<CursorPage<BackendContentEntry>> {
    const result = await contractClient.get('/api/admin/v1/content', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  findContent: (idOrSlug: string) => findInCursorPages(
    async (pagination) => {
      const result = await contractClient.get('/api/admin/v1/content', {
        parameters: { query: cursorQuery(pagination) },
      })
      return result.data
    },
    (entry) => entry.id === idOrSlug || entry.slug === idOrSlug,
  ),
  async saveContent(payload: ContentDraftPayload, id?: string, revision?: number): Promise<{ entry: BackendContentEntry; etag: string }> {
    const result = id
      ? await contractClient.patch('/api/admin/v1/content/{id}', {
          parameters: {
            path: { id },
            header: {
              'Idempotency-Key': randomRequestId(),
              'If-Match': revisionEtag(revision),
            },
          },
          body: payload,
        })
      : await contractClient.post('/api/admin/v1/content', {
          parameters: { header: { 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async publishContent(id: string, revision: number): Promise<{ entry: BackendContentEntry; etag: string }> {
    const result = await contractClient.post('/api/admin/v1/content/{id}/publish', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) },
      },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async createContentPreview(id: string, revision: number, expiresInSeconds = 600): Promise<ContentPreviewLink> {
    const result = await contractClient.post('/api/admin/v1/content/{id}/preview', {
      parameters: { path: { id }, header: { 'If-Match': revisionEtag(revision) } },
      body: { revision, expiresInSeconds },
    })
    return result.data
  },
  async listNews(pagination?: CursorPageRequest): Promise<CursorPage<BackendNewsEntry>> {
    const result = await contractClient.get('/api/admin/v1/news', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async getNews(id: string): Promise<BackendNewsEntry> {
    const result = await contractClient.get('/api/admin/v1/news/{id}', { parameters: { path: { id } } })
    return result.data
  },
  async saveNews(payload: NewsDraftPayload, id?: string, revision?: number): Promise<{ entry: BackendNewsEntry; etag: string }> {
    const result = id
      ? await contractClient.patch('/api/admin/v1/news/{id}', {
          parameters: { path: { id }, header: { 'If-Match': revisionEtag(revision), 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
      : await contractClient.post('/api/admin/v1/news', {
          parameters: { header: { 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async publishNews(id: string, revision: number): Promise<{ entry: BackendNewsEntry; etag: string }> {
    const result = await contractClient.post('/api/admin/v1/news/{id}/publish', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) } },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async rollbackNews(id: string, revision: number, targetRevision: number, reason: string): Promise<{ entry: BackendNewsEntry; etag: string }> {
    const result = await contractClient.post('/api/admin/v1/news/{id}/rollback', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) } },
      body: { revision: targetRevision, reason },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async listNewsRevisions(id: string): Promise<NewsRevision[]> {
    const result = await contractClient.get('/api/admin/v1/news/{id}/revisions', {
      parameters: { path: { id } },
    })
    return result.data.items
  },
  async getGeneralInformation(): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const result = await contractClient.get('/api/admin/v1/general-information', { parameters: { query: { locale: 'en' } } })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async saveGeneralInformation(payload: { locale: 'en'; payload: GeneralInformationPayload; isPlaceholder: boolean }, id?: string, revision?: number): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const result = id
      ? await contractClient.patch('/api/admin/v1/general-information/{id}', {
          parameters: { path: { id }, header: { 'If-Match': revisionEtag(revision), 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
      : await contractClient.post('/api/admin/v1/general-information', {
          parameters: { header: { 'Idempotency-Key': randomRequestId() } },
          body: payload,
        })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async publishGeneralInformation(id: string, revision: number): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const result = await contractClient.post('/api/admin/v1/general-information/{id}/publish', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) } },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async rollbackGeneralInformation(id: string, revision: number, targetRevision: number, reason: string): Promise<{ entry: BackendGeneralInformation; etag: string }> {
    const result = await contractClient.post('/api/admin/v1/general-information/{id}/rollback', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) } },
      body: { revision: targetRevision, reason },
    })
    return { entry: result.data, etag: result.etag ?? '' }
  },
  async listGeneralInformationRevisions(id: string): Promise<GeneralInformationRevision[]> {
    const result = await contractClient.get('/api/admin/v1/general-information/{id}/revisions', {
      parameters: { path: { id } },
    })
    return result.data.items
  },
  async listProducts(pagination?: CursorPageRequest): Promise<CursorPage<BackendProduct>> {
    const result = await contractClient.get('/api/admin/v1/products', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async getProduct(id: string): Promise<BackendProduct> {
    const result = await contractClient.get('/api/admin/v1/products/{id}', { parameters: { path: { id } } })
    return result.data
  },
  findProduct: (id: string) => findInCursorPages(
    async (pagination) => {
      const result = await contractClient.get('/api/admin/v1/products', {
        parameters: { query: cursorQuery(pagination) },
      })
      return result.data
    },
    (product) => product.id === id,
  ),
  async updateProductPresentation(id: string, revision: number, payload: ProductPresentationPayload): Promise<{ product: BackendProduct; etag: string }> {
    const result = await contractClient.patch('/api/admin/v1/products/{id}/presentation', {
      parameters: { path: { id }, header: { 'If-Match': revisionEtag(revision), 'Idempotency-Key': randomRequestId() } },
      body: payload,
    })
    return { product: result.data, etag: result.etag ?? '' }
  },
  async getProductPrivatePricing(id: string): Promise<ProductPrivatePricing> {
    const result = await contractClient.get('/api/admin/v1/products/{id}/private-pricing', {
      parameters: { path: { id } },
      cache: 'no-store',
    })
    return result.data
  },
  async listTemporaryOverrides(productId: string, pagination?: CursorPageRequest): Promise<CursorPage<BackendTemporaryOverride>> {
    const result = await contractClient.get('/api/admin/v1/products/{id}/temporary-overrides', {
      parameters: { path: { id: productId }, query: cursorQuery(pagination) },
    })
    return result.data
  },
  listAllTemporaryOverrides: (productId: string) => collectCursorPages(
    async (pagination) => {
      const result = await contractClient.get('/api/admin/v1/products/{id}/temporary-overrides', {
        parameters: { path: { id: productId }, query: cursorQuery(pagination) },
      })
      return result.data
    },
  ),
  async publishProduct(id: string, revision: number, idempotencyKey: string = randomRequestId()): Promise<{ product: BackendProduct; etag: string }> {
    const result = await contractClient.post('/api/admin/v1/products/{id}/publish', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': idempotencyKey, 'If-Match': revisionEtag(revision) },
      },
    })
    return { product: result.data, etag: result.etag ?? '' }
  },
  async listProductImports(pagination?: CursorPageRequest): Promise<CursorPage<ProductImportResult>> {
    const result = await contractClient.get('/api/admin/v1/products/imports', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async importProductMaster(csv: string, mappingVersion?: string): Promise<ProductImportAccepted> {
    const result = await contractClient.post('/api/admin/v1/products/imports', {
      parameters: { header: { 'Idempotency-Key': randomRequestId() } },
      body: mappingVersion ? { csv, mappingVersion } : { csv },
    })
    return result.data
  },
  async listSyncRuns(pagination?: CursorPageRequest): Promise<CursorPage<BackendSyncRun>> {
    const result = await contractClient.get('/api/admin/v1/feishu/sync-runs', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async listConflicts(pagination?: CursorPageRequest): Promise<CursorPage<BackendSyncConflict>> {
    const result = await contractClient.get('/api/admin/v1/feishu/conflicts', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async startSync(dryRun: boolean, mappingVersion = 'v1'): Promise<BackendSyncRun> {
    const result = await contractClient.post('/api/admin/v1/feishu/sync-runs', {
      parameters: { header: { 'Idempotency-Key': randomRequestId() } },
      body: { dryRun, mappingVersion },
    })
    return result.data
  },
  async listRfqs(pagination?: CursorPageRequest): Promise<CursorPage<Record<string, unknown>>> {
    const result = await contractClient.get('/api/admin/v1/rfqs', {
      parameters: { query: cursorQuery(pagination) },
    })
    return {
      items: result.data.items.map((item) => ({ ...item })),
      nextCursor: result.data.nextCursor,
    }
  },
  async listContacts(pagination?: CursorPageRequest): Promise<CursorPage<Record<string, unknown>>> {
    const result = await contractClient.get('/api/admin/v1/contacts', {
      parameters: { query: cursorQuery(pagination) },
    })
    return {
      items: result.data.items.map((item) => ({ ...item })),
      nextCursor: result.data.nextCursor,
    }
  },
  async analyticsSummary(): Promise<{ acceptedEventCount: number; rfqCount: number; contactCount: number; containsPii: false; source: 'firstParty' }> {
    const result = await contractClient.get('/api/admin/v1/analytics/summary')
    return result.data
  },
  async listGuestVisits(pagination?: CursorPageRequest): Promise<CursorPage<GuestVisitAggregate>> {
    const result = await contractClient.get('/api/admin/v1/analytics/visits', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async listGuestSources(pagination?: CursorPageRequest): Promise<CursorPage<GuestSourceDaily>> {
    const result = await contractClient.get('/api/admin/v1/analytics/sources', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async listUsers(pagination?: CursorPageRequest): Promise<CursorPage<AdminUserRecord>> {
    const result = await contractClient.get('/api/admin/v1/users', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async getUser(id: string): Promise<{ user: AdminUserRecord; etag: string }> {
    const result = await contractClient.get('/api/admin/v1/users/{id}', {
      parameters: { path: { id } },
    })
    return { user: result.data, etag: result.etag ?? `"revision-${result.data.revision}"` }
  },
  async listUserInvitations(): Promise<CursorPage<UserInvitation>> {
    const result = await contractClient.get('/api/admin/v1/user-invitations')
    return result.data
  },
  async inviteUser(payload: { email: string; displayName: string; roleKeys: string[] }): Promise<UserInvitation> {
    const result = await contractClient.post('/api/admin/v1/user-invitations', {
      parameters: { header: { 'Idempotency-Key': randomRequestId() } },
      body: payload,
    })
    return result.data
  },
  async revokeInvitation(id: string, reason: string): Promise<void> {
    await contractClient.post('/api/admin/v1/user-invitations/{id}/revoke', {
      parameters: { path: { id }, header: { 'Idempotency-Key': randomRequestId() } },
      body: { reason },
    })
  },
  async updateUser(id: string, revision: number, payload: UpdateAdminUser): Promise<{ user: AdminUserRecord; etag: string }> {
    const result = await contractClient.patch('/api/admin/v1/users/{id}', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) },
      },
      body: payload,
    })
    return { user: result.data, etag: result.etag ?? `"revision-${result.data.revision}"` }
  },
  async revokeUserSessions(id: string): Promise<void> {
    await contractClient.delete('/api/admin/v1/users/{id}/sessions', {
      parameters: { path: { id } },
    })
  },
  async listRoles(_pagination?: CursorPageRequest): Promise<CursorPage<AdminRoleRecord>> {
    void _pagination
    const result = await contractClient.get('/api/admin/v1/roles')
    return result.data
  },
  async getRole(id: string): Promise<{ role: AdminRoleRecord; etag: string }> {
    const result = await contractClient.get('/api/admin/v1/roles/{id}', {
      parameters: { path: { id } },
    })
    return { role: result.data, etag: result.etag ?? `"revision-${result.data.revision}"` }
  },
  async updateRole(id: string, revision: number, payload: UpdateAdminRole): Promise<{ role: AdminRoleRecord; etag: string }> {
    const result = await contractClient.patch('/api/admin/v1/roles/{id}', {
      parameters: {
        path: { id },
        header: { 'Idempotency-Key': randomRequestId(), 'If-Match': revisionEtag(revision) },
      },
      body: payload,
    })
    return { role: result.data, etag: result.etag ?? `"revision-${result.data.revision}"` }
  },
  async getSettings(): Promise<{ settings: PlatformSettings; etag: string }> {
    const result = await contractClient.get('/api/admin/v1/settings')
    return { settings: result.data, etag: result.etag ?? '' }
  },
  async updateSettings(payload: UpdatePlatformSettings, revision: number): Promise<{ settings: PlatformSettings; etag: string }> {
    const result = await contractClient.patch('/api/admin/v1/settings', {
      parameters: { header: { 'If-Match': revisionEtag(revision) } },
      body: payload,
    })
    return { settings: result.data, etag: result.etag ?? '' }
  },
  async listOperations(pagination?: CursorPageRequest): Promise<CursorPage<BackendOperation>> {
    const result = await contractClient.get('/api/admin/v1/operations', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
  async getOperation(id: string): Promise<BackendOperation> {
    const result = await contractClient.get('/api/admin/v1/operations/{id}', {
      parameters: { path: { id } },
    })
    return result.data
  },
  async createOperation(kind: string, reason: string, confirmation: string, otp: string): Promise<BackendOperation> {
    const result = await contractClient.post('/api/admin/v1/operations', {
      parameters: {
        header: { 'Idempotency-Key': randomRequestId(), 'X-TOTP-Code': otp },
      },
      body: { kind: operationKind(kind), reason, confirmation },
    })
    return result.data
  },
  async listAudit(pagination?: CursorPageRequest): Promise<CursorPage<BackendAuditEvent>> {
    const result = await contractClient.get('/api/admin/v1/audit', {
      parameters: { query: cursorQuery(pagination) },
    })
    return result.data
  },
}
