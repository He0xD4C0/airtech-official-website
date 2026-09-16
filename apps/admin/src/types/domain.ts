export type Permission =
  | 'dashboard.read'
  | 'content.read'
  | 'content.write'
  | 'content.publish'
  | 'product.read'
  | 'product.write'
  | 'product.publish'
  | 'product.pricing.read'
  | 'integration.run'
  | 'media.write'
  | 'rfq.read'
  | 'rfq.read_pii'
  | 'rfq.assign'
  | 'analytics.read'
  | 'identity.manage'
  | 'audit.read'
  | 'settings.manage'
  | 'devtools.shell'

export interface SessionUser {
  id: string
  displayName: string
  email: string
  role: string
  permissions: Permission[]
  environment: 'development' | 'staging' | 'production'
  totpEnabled: boolean
}

export type ProductStatus = 'draft' | 'scheduled' | 'published' | 'archived'
export type FactState =
  | 'verified'
  | 'missing'
  | 'notApplicable'
  | 'notTested'
  | 'confidential'
  | 'pendingVerification'

export interface ProductSummary {
  id: string
  model: string
  family: string
  sourceState: 'loaded' | 'pending'
  publishState: ProductStatus
  verifiedFields: number
  totalFields: number
  overrideExpiresAt?: string
}

export interface SyncConflict {
  id: string
  field: string
  entity: string
  base: string
  local: string
  incoming: string
  severity: 'blocking' | 'warning'
}

export type ApiProblem = ProblemDetails
import type { ProblemDetails } from '@airtek/contracts'
