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
  | 'identity.roles.manage'
  | 'audit.read'
  | 'settings.manage'
  | 'mail.manage'
  | 'sms.manage'
  | 'captcha.manage'
  | 'devtools.shell'

export interface SessionUser {
  id: string
  displayName: string
  email: string
  role: string
  roleKeys: string[]
  permissions: Permission[]
  environment: 'development' | 'staging' | 'production'
  totpEnabled: boolean
  phoneVerified: boolean
  mustChangePassword: boolean
  mustConfirmRecoveryKey: boolean
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

export type ApiProblem = ProblemDetails
import type { ProblemDetails } from '@airtek/contracts'
